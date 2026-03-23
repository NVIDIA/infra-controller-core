/*
 * SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use async_trait::async_trait;
use eyre::WrapErr;
use reqwest::Client;

use super::NvueApplier;

/// Apply NVUE config via the NVUE REST API over HTTP. No files are written
/// to the HBN filesystem except for the static ones that don't have an
/// API equivalent (yet).
#[derive(Debug)]
pub struct ApiApplier {
    client: Client,
    base_url: String,
    credentials: Option<(String, String)>,
    hbn_root: std::path::PathBuf,
    skip_reload: bool,
    /// Hash of the last successfully applied config, used for diff detection.
    /// Behind a Mutex because `apply()` takes `&self` (required by the async trait)
    /// but needs to update this value. In practice this is only called sequentially
    /// from the main loop — the lock is never contended.
    last_applied_hash: std::sync::Mutex<Option<blake3::Hash>>,
}

/// Resolve a credential string with format: `env:<VAR>`, `file:<path>`, `value:<literal>`,
/// or a bare string (treated as a literal value).
fn resolve_credential(raw: &str) -> eyre::Result<String> {
    if let Some(var) = raw.strip_prefix("env:") {
        std::env::var(var)
            .wrap_err_with(|| format!("reading env var {var} for NVUE API credential"))
    } else if let Some(path) = raw.strip_prefix("file:") {
        std::fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .wrap_err_with(|| format!("reading file {path} for NVUE API credential"))
    } else if let Some(val) = raw.strip_prefix("value:") {
        Ok(val.to_string())
    } else {
        Ok(raw.to_string())
    }
}

impl ApiApplier {
    pub fn new(
        address: &str,
        user: Option<&str>,
        passwd: Option<&str>,
        insecure: bool,
        hbn_root: std::path::PathBuf,
        skip_reload: bool,
    ) -> eyre::Result<Self> {
        let credentials = match (user, passwd) {
            (Some(u), Some(p)) => {
                let user = resolve_credential(u)?;
                let passwd = resolve_credential(p)?;
                Some((user, passwd))
            }
            (None, None) => None,
            _ => eyre::bail!(
                "--nvue-api-user and --nvue-api-passwd must both be set, or both be unset"
            ),
        };

        let client = Client::builder()
            .danger_accept_invalid_certs(insecure)
            .build()
            .wrap_err("building NVUE API HTTP client")?;

        let base_url = address.trim_end_matches('/').to_string();

        Ok(Self {
            client,
            base_url,
            credentials,
            hbn_root,
            skip_reload,
            last_applied_hash: std::sync::Mutex::new(None),
        })
    }

    /// Create a new NVUE revision. All config changes are staged against a revision
    /// before being applied. Returns the revision ID.
    async fn create_revision(&self) -> eyre::Result<String> {
        let url = format!("{}/nvue_v1/revision", self.base_url);
        let resp = self
            .request(reqwest::Method::POST, &url)
            .send()
            .await
            .wrap_err("POST /nvue_v1/revision")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            eyre::bail!("POST /nvue_v1/revision returned {status}: {body}");
        }

        // Response is a JSON object like {"changeset/cumulus/2026-03-23_...": {"state": "pending", ...}}
        // The key is the revision ID.
        let body: serde_json::Value = resp
            .json()
            .await
            .wrap_err("parsing revision response JSON")?;
        let revision_id = body
            .as_object()
            .and_then(|obj| obj.keys().next())
            .ok_or_else(|| eyre::eyre!("unexpected revision response format: {body}"))?
            .clone();

        Ok(revision_id)
    }

    /// Delete all config in the revision, giving us a clean slate. This is
    /// necessary for full-replacement semantics matching `nv config replace`.
    /// Without this, the subsequent PATCH would merge with existing config,
    /// potentially leaving stale entries from previous applies.
    async fn delete_config(&self, revision_id: &str) -> eyre::Result<()> {
        let url = format!(
            "{}/nvue_v1/?rev={}",
            self.base_url,
            urlencoding::encode(revision_id)
        );
        let resp = self
            .request(reqwest::Method::DELETE, &url)
            .send()
            .await
            .wrap_err("DELETE /nvue_v1/")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            eyre::bail!("DELETE /nvue_v1/ returned {status}: {body}");
        }
        Ok(())
    }

    /// Write the full config JSON into the revision. This is a root-level PATCH
    /// that sets the entire NVUE configuration tree.
    async fn patch_config(
        &self,
        revision_id: &str,
        config: &serde_json::Value,
    ) -> eyre::Result<()> {
        let url = format!(
            "{}/nvue_v1/?rev={}",
            self.base_url,
            urlencoding::encode(revision_id)
        );
        let resp = self
            .request(reqwest::Method::PATCH, &url)
            .header("Content-Type", "application/json")
            .json(config)
            .send()
            .await
            .wrap_err("PATCH /nvue_v1/")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            eyre::bail!("PATCH /nvue_v1/ returned {status}: {body}");
        }
        Ok(())
    }

    /// Apply the revision, transitioning it from "pending" to "applied". This
    /// is the equivalent of `nv config apply -y` and makes the config take effect.
    async fn apply_revision(&self, revision_id: &str) -> eyre::Result<()> {
        let url = format!(
            "{}/nvue_v1/revision/{}",
            self.base_url,
            urlencoding::encode(revision_id)
        );
        let body = serde_json::json!({
            "state": "apply",
            "auto-prompt": {
                "ays": "ays_yes"
            }
        });
        let resp = self
            .request(reqwest::Method::PATCH, &url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .wrap_err("PATCH /nvue_v1/revision (apply)")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            eyre::bail!("PATCH /nvue_v1/revision/{revision_id} (apply) returned {status}: {body}");
        }
        Ok(())
    }

    fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.request(method, url);
        if let Some((ref user, ref passwd)) = self.credentials {
            req = req.basic_auth(user, Some(passwd));
        }
        req
    }
}

#[async_trait]
impl NvueApplier for ApiApplier {
    fn hbn_root(&self) -> &std::path::Path {
        &self.hbn_root
    }

    fn skip_reload(&self) -> bool {
        self.skip_reload
    }

    async fn apply(&self, yaml_content: String) -> eyre::Result<bool> {
        let content_hash = blake3::hash(yaml_content.as_bytes());
        let last_hash = self
            .last_applied_hash
            .lock()
            .map_err(|e| eyre::eyre!("last_applied_hash lock poisoned: {e}"))?
            .as_ref()
            .copied();
        if last_hash == Some(content_hash) {
            return Ok(false);
        }

        let yaml_value: serde_yaml::Value =
            serde_yaml::from_str(&yaml_content).wrap_err("parsing NVUE YAML config")?;
        let json_value: serde_json::Value = serde_json::to_value(yaml_to_json(yaml_value))
            .wrap_err("converting NVUE YAML to JSON")?;

        // First, we create a new revision.
        let revision_id = self.create_revision().await?;
        tracing::info!(revision_id, "created NVUE API revision");

        // Then, we DELETE existing config in the revision. This gives us
        // full-replacement semantics matching `nv config replace`. Without
        // this, PATCH does a merge, which could leave stale config entries,
        // from previous applies. A metaphor here would be applying updated
        // mlxconfig parameters without first doing a reset, and then laying
        // our config over it.
        self.delete_config(&revision_id).await?;

        // And now we PATCH the new config into the revision.
        self.patch_config(&revision_id, &json_value).await?;

        // Finally, apply the revision.
        self.apply_revision(&revision_id).await?;
        tracing::info!(revision_id, "applied NVUE API revision");

        *self
            .last_applied_hash
            .lock()
            .map_err(|e| eyre::eyre!("last_applied_hash lock poisoned: {e}"))? = Some(content_hash);

        Ok(true)
    }
}

/// Convert a serde_yaml::Value to a serde_json::Value.
/// serde_yaml uses different key types (particularly mapping keys can be non-string),
/// so we walk the tree to ensure clean JSON output.
fn yaml_to_json(yaml: serde_yaml::Value) -> serde_json::Value {
    match yaml {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                serde_json::Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        serde_yaml::Value::String(s) => serde_json::Value::String(s),
        serde_yaml::Value::Sequence(seq) => {
            serde_json::Value::Array(seq.into_iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let obj = map
                .into_iter()
                .map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s,
                        other => serde_yaml::to_string(&other)
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                    };
                    (key, yaml_to_json(v))
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json(tagged.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_credential_bare_value() {
        assert_eq!(resolve_credential("myuser").unwrap(), "myuser");
    }

    #[test]
    fn test_resolve_credential_value_prefix() {
        assert_eq!(resolve_credential("value:myuser").unwrap(), "myuser");
    }

    #[test]
    fn test_resolve_credential_env_prefix() {
        // SAFETY: This test is not run in parallel with other tests that use this env var.
        unsafe { std::env::set_var("TEST_NVUE_CRED", "from_env") };
        assert_eq!(
            resolve_credential("env:TEST_NVUE_CRED").unwrap(),
            "from_env"
        );
        unsafe { std::env::remove_var("TEST_NVUE_CRED") };
    }

    #[test]
    fn test_resolve_credential_env_missing() {
        assert!(resolve_credential("env:NONEXISTENT_NVUE_VAR_12345").is_err());
    }

    #[test]
    fn test_resolve_credential_file_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cred.txt");
        std::fs::write(&path, "from_file\n").unwrap();
        assert_eq!(
            resolve_credential(&format!("file:{}", path.display())).unwrap(),
            "from_file"
        );
    }

    #[test]
    fn test_resolve_credential_file_missing() {
        assert!(resolve_credential("file:/nonexistent/path/cred.txt").is_err());
    }

    #[test]
    fn test_yaml_to_json_conversion() {
        let yaml = r#"
system:
  hostname: my-dpu
interface:
  lo:
    ip:
      address:
        10.0.0.1/32: {}
  swp1:
    type: swp
router:
  bgp:
    autonomous-system: 65001
    enable: on
"#;
        let yaml_val: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let json_val = yaml_to_json(yaml_val);

        assert_eq!(
            json_val["system"]["hostname"],
            serde_json::Value::String("my-dpu".to_string())
        );
        assert_eq!(
            json_val["router"]["bgp"]["autonomous-system"],
            serde_json::json!(65001)
        );
        assert_eq!(
            json_val["interface"]["lo"]["ip"]["address"]["10.0.0.1/32"],
            serde_json::json!({})
        );
    }

    #[test]
    fn test_new_requires_both_credentials() {
        let result = ApiApplier::new(
            "https://localhost:8765",
            Some("user"),
            None,
            false,
            "/tmp".into(),
            false,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must both be set"));
    }

    #[test]
    fn test_new_no_credentials() {
        let client = ApiApplier::new(
            "https://localhost:8765",
            None,
            None,
            false,
            "/tmp".into(),
            false,
        )
        .unwrap();
        assert!(client.credentials.is_none());
    }

    #[test]
    fn test_new_with_credentials() {
        let client = ApiApplier::new(
            "https://localhost:8765",
            Some("nvidia"),
            Some("nvidia"),
            false,
            "/tmp".into(),
            false,
        )
        .unwrap();
        assert_eq!(
            client.credentials,
            Some(("nvidia".to_string(), "nvidia".to_string()))
        );
    }
}

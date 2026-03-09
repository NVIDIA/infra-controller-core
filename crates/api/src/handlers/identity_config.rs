/*
 * SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use it except in compliance with the License.
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

//! gRPC handlers for tenant_identity_config table.
//! Identity config: issuer, audiences, TTL, signing key (Get/Set/Delete).
//! Token delegation: token exchange config for external IdP (Get/Set/Delete).

use std::collections::BTreeMap;

use ::rpc::Timestamp;
use ::rpc::forge::{
    GetIdentityConfigRequest, GetTokenDelegationRequest, IdentityConfigRequest,
    IdentityConfigResponse, TokenDelegationRequest, TokenDelegationResponse,
};
use db::{WithTransaction, tenant, tenant_identity_config};
use prost_types::value::Kind;
use prost_types::{Struct, Value};
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};

use crate::api::{Api, log_request_data};
use crate::auth::AuthContext;

// --- Token delegation: Struct/JSON conversion and secret hashing ---

/// Secret keys to omit from response. Hash (client_secret_hash) is stored in blob at write time.
const SECRET_KEYS_TO_OMIT: &[&str] = &["client_secret"];

/// Hex chars to show in get_token_delegation response (8 chars + ".." suffix).
const HASH_DISPLAY_HEX_LEN: usize = 8;

/// Computes full sha256:XXXXXXXXXXXXXXXX... (64 hex chars). Stored in blob at write time.
fn compute_secret_hash(cleartext: &str) -> String {
    let hash = Sha256::digest(cleartext.as_bytes());
    format!("sha256:{}", hex::encode(hash))
}

/// Truncates hash for display in get_token_delegation: algorithm-prefix:XXXXXXXX.. (algorithm-prefix is "sha256" or "sha512" etc.)
fn truncate_hash_for_display(full_hash: &str) -> String {
    full_hash
        .split_once(':')
        .map(|(prefix, rest)| {
            format!(
                "{}:{}..",
                prefix,
                rest.chars().take(HASH_DISPLAY_HEX_LEN).collect::<String>()
            )
        })
        .unwrap_or_else(|| full_hash.to_string())
}

fn struct_to_json(pb: &Struct) -> serde_json::Value {
    let obj: serde_json::Map<String, serde_json::Value> = pb
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), value_to_json(v)))
        .collect();
    serde_json::Value::Object(obj)
}

fn value_to_json(pb: &Value) -> serde_json::Value {
    match &pb.kind {
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::NumberValue(n)) => serde_json::Value::Number(
            serde_json::Number::from_f64(*n).unwrap_or(serde_json::Number::from(0)),
        ),
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::StructValue(s)) => struct_to_json(s),
        Some(Kind::ListValue(l)) => {
            let arr: Vec<serde_json::Value> = l.values.iter().map(value_to_json).collect();
            serde_json::Value::Array(arr)
        }
        None => serde_json::Value::Null,
    }
}

fn json_to_struct(json: &serde_json::Value) -> Option<Struct> {
    let obj = json.as_object()?;
    let fields: BTreeMap<String, Value> = obj
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), json_to_value(v)?)))
        .collect();
    Some(Struct { fields })
}

fn json_to_value(json: &serde_json::Value) -> Option<Value> {
    Some(Value {
        kind: Some(match json {
            serde_json::Value::Null => Kind::NullValue(0),
            serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64()?),
            serde_json::Value::String(s) => Kind::StringValue(s.clone()),
            serde_json::Value::Bool(b) => Kind::BoolValue(*b),
            serde_json::Value::Array(a) => {
                let values: Vec<Value> = a.iter().filter_map(json_to_value).collect();
                Kind::ListValue(prost_types::ListValue { values })
            }
            serde_json::Value::Object(o) => {
                let fields: BTreeMap<String, Value> = o
                    .iter()
                    .filter_map(|(k, v)| Some((k.clone(), json_to_value(v)?)))
                    .collect();
                Kind::StructValue(Struct { fields })
            }
        }),
    })
}

/// Builds response auth_method_config: omits secrets, passes through *_hash fields (stored in blob).
/// Truncates *_hash values to 8 hex chars + ".." for display in get_token_delegation.
fn build_response_auth_config(config: &serde_json::Value) -> serde_json::Value {
    let obj = match config {
        serde_json::Value::Object(o) => o,
        _ => return config.clone(),
    };
    let mut out: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    for (k, v) in obj.iter() {
        let key_lower = k.to_lowercase();
        let omit = SECRET_KEYS_TO_OMIT
            .iter()
            .any(|secret| key_lower == *secret);
        if omit {
            continue;
        }
        let value = if key_lower.ends_with("_hash") {
            if let Some(s) = v.as_str() {
                serde_json::Value::String(truncate_hash_for_display(s))
            } else {
                build_response_auth_config(v)
            }
        } else {
            build_response_auth_config(v)
        };
        out.insert(k.clone(), value);
    }
    serde_json::Value::Object(out)
}

// --- Identity configuration handlers ---

/// Handles GetIdentityConfiguration: fetches per-org identity config.
pub(crate) async fn get_identity_configuration(
    api: &Api,
    request: Request<GetIdentityConfigRequest>,
) -> Result<Response<IdentityConfigResponse>, Status> {
    log_request_data(&request);

    request
        .extensions()
        .get::<AuthContext>()
        .ok_or_else(|| Status::unauthenticated("No authentication context found"))?;

    if !api.runtime_config.machine_identity.enabled {
        return Err(Status::unavailable(
            "Machine identity must be enabled in site config",
        ));
    }

    let req = request.into_inner();
    let org_id = req.org_id.trim();
    if org_id.is_empty() {
        return Err(Status::invalid_argument("org_id is required"));
    }

    let cfg = api
        .database_connection
        .with_txn(|txn| Box::pin(async move { tenant_identity_config::find(org_id, txn).await }))
        .await??;

    let cfg = match cfg {
        Some(c) => c,
        None => return Err(Status::not_found("Identity config not found for org")),
    };

    let allowed_audiences: Vec<String> =
        serde_json::from_value(cfg.allowed_audiences.clone()).unwrap_or_default();

    Ok(Response::new(IdentityConfigResponse {
        org_id: cfg.organization_id,
        enabled: cfg.enabled,
        issuer: cfg.issuer,
        default_audience: cfg.default_audience,
        allowed_audiences,
        token_ttl: cfg.token_ttl as u32,
        subject_domain: cfg.subject_domain_prefix,
        created_at: Some(Timestamp::from(cfg.created_at)),
        updated_at: Some(Timestamp::from(cfg.updated_at)),
        key_id: cfg.key_id,
    }))
}

/// Handles DeleteIdentityConfiguration: removes per-org identity config.
pub(crate) async fn delete_identity_configuration(
    api: &Api,
    request: Request<GetIdentityConfigRequest>,
) -> Result<Response<()>, Status> {
    log_request_data(&request);

    request
        .extensions()
        .get::<AuthContext>()
        .ok_or_else(|| Status::unauthenticated("No authentication context found"))?;

    if !api.runtime_config.machine_identity.enabled {
        return Err(Status::unavailable(
            "Machine identity must be enabled in site config",
        ));
    }

    let req = request.into_inner();
    let org_id = req.org_id.trim();
    if org_id.is_empty() {
        return Err(Status::invalid_argument("org_id is required"));
    }

    let deleted = api
        .database_connection
        .with_txn(|txn| Box::pin(async move { tenant_identity_config::delete(org_id, txn).await }))
        .await??;

    if !deleted {
        return Err(Status::not_found("Identity config not found for org"));
    }

    Ok(Response::new(()))
}

/// Handles SetIdentityConfiguration: upserts per-org identity config into tenant_identity_config.
/// Requires auth. Tenant must exist. Key generation is placeholder until Vault integration.
pub(crate) async fn set_identity_configuration(
    api: &Api,
    request: Request<IdentityConfigRequest>,
) -> Result<Response<IdentityConfigResponse>, Status> {
    log_request_data(&request);

    request
        .extensions()
        .get::<AuthContext>()
        .ok_or_else(|| Status::unauthenticated("No authentication context found"))?;

    if !api.runtime_config.machine_identity.enabled {
        return Err(Status::unavailable(
            "Machine identity must be enabled in site config before setting identity configuration",
        ));
    }

    let req = request.into_inner();
    let org_id = req.org_id.trim();
    if org_id.is_empty() {
        return Err(Status::invalid_argument("org_id is required"));
    }
    if req.issuer.is_empty() {
        return Err(Status::invalid_argument("issuer is required"));
    }
    if req.default_audience.is_empty() {
        return Err(Status::invalid_argument("default_audience is required"));
    }
    if req.subject_domain.is_empty() {
        return Err(Status::invalid_argument("subject_domain is required"));
    }
    let mi = &api.runtime_config.machine_identity;
    if req.token_ttl == 0 {
        return Err(Status::invalid_argument(format!(
            "token_ttl is required (must be between {} and {} seconds)",
            mi.token_ttl_min, mi.token_ttl_max
        )));
    }

    let allowed: Vec<String> = req.allowed_audiences.into_iter().collect();
    let token_ttl = req.token_ttl;
    if token_ttl < mi.token_ttl_min || token_ttl > mi.token_ttl_max {
        return Err(Status::invalid_argument(format!(
            "token_ttl must be between {} and {} seconds",
            mi.token_ttl_min, mi.token_ttl_max
        )));
    }
    let algorithm = mi.algorithm.as_str();
    let master_key_id = "placeholder-master-key";

    let cfg = api
        .database_connection
        .with_txn(|txn| {
            Box::pin(async move {
                let tenant_exists = tenant::find(org_id, false, txn).await?;
                if tenant_exists.is_none() {
                    return Err(db::DatabaseError::NotFoundError {
                        kind: "Tenant",
                        id: org_id.to_string(),
                    });
                }
                tenant_identity_config::set(
                    org_id,
                    &req.issuer,
                    &req.default_audience,
                    &allowed,
                    token_ttl,
                    &req.subject_domain,
                    req.enabled,
                    req.rotate_key,
                    algorithm,
                    master_key_id,
                    txn,
                )
                .await
            })
        })
        .await??;

    let allowed_audiences: Vec<String> =
        serde_json::from_value(cfg.allowed_audiences.clone()).unwrap_or_default();

    Ok(Response::new(IdentityConfigResponse {
        org_id: cfg.organization_id,
        enabled: cfg.enabled,
        issuer: cfg.issuer,
        default_audience: cfg.default_audience,
        allowed_audiences,
        token_ttl: cfg.token_ttl as u32,
        subject_domain: cfg.subject_domain_prefix,
        created_at: Some(Timestamp::from(cfg.created_at)),
        updated_at: Some(Timestamp::from(cfg.updated_at)),
        key_id: cfg.key_id,
    }))
}

// --- Token delegation handlers ---

pub(crate) async fn get_token_delegation(
    api: &Api,
    request: Request<GetTokenDelegationRequest>,
) -> Result<Response<TokenDelegationResponse>, Status> {
    log_request_data(&request);

    request
        .extensions()
        .get::<AuthContext>()
        .ok_or_else(|| Status::unauthenticated("No authentication context found"))?;

    if !api.runtime_config.machine_identity.enabled {
        return Err(Status::unavailable(
            "Machine identity must be enabled in site config",
        ));
    }

    let req = request.into_inner();
    let org_id = req.org_id.trim();
    if org_id.is_empty() {
        return Err(Status::invalid_argument("org_id is required"));
    }

    let cfg = api
        .database_connection
        .with_txn(|txn| Box::pin(async move { tenant_identity_config::find(org_id, txn).await }))
        .await??;

    let cfg = match cfg {
        Some(c) => c,
        None => return Err(Status::not_found("Identity config not found for org")),
    };

    let auth_method_config = cfg
        .encrypted_auth_method_config
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let (token_endpoint, auth_method) = match (&cfg.token_endpoint, &cfg.auth_method) {
        (Some(te), Some(am)) => (te.clone(), am.clone()),
        _ => return Err(Status::not_found("Token delegation not configured for org")),
    };

    let redacted = build_response_auth_config(&auth_method_config);
    let pb_struct = json_to_struct(&redacted).unwrap_or_default();

    let created_at = cfg
        .token_delegation_created_at
        .map(Timestamp::from)
        .or_else(|| Some(Timestamp::from(cfg.updated_at)));

    Ok(Response::new(TokenDelegationResponse {
        org_id: cfg.organization_id,
        token_endpoint,
        auth_method,
        auth_method_config: Some(pb_struct),
        subject_token_audience: cfg.subject_token_audience.unwrap_or_default(),
        created_at,
        updated_at: Some(Timestamp::from(cfg.updated_at)),
    }))
}

pub(crate) async fn set_token_delegation(
    api: &Api,
    request: Request<TokenDelegationRequest>,
) -> Result<Response<TokenDelegationResponse>, Status> {
    log_request_data(&request);

    request
        .extensions()
        .get::<AuthContext>()
        .ok_or_else(|| Status::unauthenticated("No authentication context found"))?;

    if !api.runtime_config.machine_identity.enabled {
        return Err(Status::unavailable(
            "Machine identity must be enabled in site config",
        ));
    }

    let req = request.into_inner();
    let org_id = req.org_id.trim();
    if org_id.is_empty() {
        return Err(Status::invalid_argument("org_id is required"));
    }
    if req.token_endpoint.is_empty() {
        return Err(Status::invalid_argument("token_endpoint is required"));
    }
    if req.auth_method.is_empty() {
        return Err(Status::invalid_argument("auth_method is required"));
    }
    let auth_method_config = req
        .auth_method_config
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("auth_method_config is required"))?;

    let config = struct_to_json(auth_method_config);
    let mut config_json = config.clone();

    // Store client_secret_hash in blob at write time (computed from cleartext).
    let client_secret = config
        .as_object()
        .and_then(|o| o.iter().find(|(k, _)| k.to_lowercase() == "client_secret"))
        .and_then(|(_, v)| v.as_str());
    if let Some(client_secret) = client_secret
        && let Some(obj) = config_json.as_object_mut()
    {
        obj.insert(
            "client_secret_hash".to_string(),
            serde_json::Value::String(compute_secret_hash(client_secret)),
        );
    }

    let config_str = serde_json::to_string(&config_json).unwrap_or_else(|_| "{}".to_string());

    let cfg = api
        .database_connection
        .with_txn(|txn| {
            Box::pin(async move {
                let tenant_exists = tenant::find(org_id, false, txn).await?;
                if tenant_exists.is_none() {
                    return Err(db::DatabaseError::NotFoundError {
                        kind: "Tenant",
                        id: org_id.to_string(),
                    });
                }
                tenant_identity_config::set_token_delegation(
                    org_id,
                    &req.token_endpoint,
                    &req.auth_method,
                    &config_str,
                    if req.subject_token_audience.is_empty() {
                        None
                    } else {
                        Some(req.subject_token_audience.as_str())
                    },
                    txn,
                )
                .await
            })
        })
        .await??;

    // Response uses stored config (has client_secret_hash in blob).
    let redacted = build_response_auth_config(&config_json);
    let pb_struct = json_to_struct(&redacted).unwrap_or_default();

    let created_at = cfg
        .token_delegation_created_at
        .map(Timestamp::from)
        .or_else(|| Some(Timestamp::from(cfg.updated_at)));

    Ok(Response::new(TokenDelegationResponse {
        org_id: cfg.organization_id,
        token_endpoint: cfg.token_endpoint.unwrap_or_default(),
        auth_method: cfg.auth_method.unwrap_or_default(),
        auth_method_config: Some(pb_struct),
        subject_token_audience: cfg.subject_token_audience.unwrap_or_default(),
        created_at,
        updated_at: Some(Timestamp::from(cfg.updated_at)),
    }))
}

pub(crate) async fn delete_token_delegation(
    api: &Api,
    request: Request<GetTokenDelegationRequest>,
) -> Result<Response<()>, Status> {
    log_request_data(&request);

    request
        .extensions()
        .get::<AuthContext>()
        .ok_or_else(|| Status::unauthenticated("No authentication context found"))?;

    if !api.runtime_config.machine_identity.enabled {
        return Err(Status::unavailable(
            "Machine identity must be enabled in site config",
        ));
    }

    let req = request.into_inner();
    let org_id = req.org_id.trim();
    if org_id.is_empty() {
        return Err(Status::invalid_argument("org_id is required"));
    }

    api.database_connection
        .with_txn(|txn| {
            Box::pin(
                async move { tenant_identity_config::delete_token_delegation(org_id, txn).await },
            )
        })
        .await??;

    Ok(Response::new(()))
}

#[cfg(test)]
mod tests {
    use prost_types::value::Kind;
    use prost_types::{Struct, Value};

    use super::*;

    #[test]
    fn test_compute_secret_hash() {
        let h = compute_secret_hash("");
        assert!(h.starts_with("sha256:"));
        assert_eq!(h.len(), 7 + 64); // "sha256:" + 64 hex chars
        assert!(h[7..].chars().all(|c| c.is_ascii_hexdigit()));

        let h2 = compute_secret_hash("secret");
        assert!(h2.starts_with("sha256:"));
        assert_ne!(h, h2);
    }

    #[test]
    fn test_truncate_hash_for_display() {
        assert_eq!(
            truncate_hash_for_display("sha256:abcd1234567890abcdef"),
            "sha256:abcd1234.."
        );
        assert_eq!(truncate_hash_for_display("sha512:xyz"), "sha512:xyz..");
        assert_eq!(truncate_hash_for_display("no-colon"), "no-colon");
    }

    #[test]
    fn test_struct_to_json_and_value_to_json() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "s".to_string(),
            Value {
                kind: Some(Kind::StringValue("hello".to_string())),
            },
        );
        fields.insert(
            "n".to_string(),
            Value {
                kind: Some(Kind::NumberValue(42.0)),
            },
        );
        let pb = Struct { fields };

        let json = struct_to_json(&pb);
        let obj = json.as_object().unwrap();
        assert_eq!(obj.get("s").unwrap().as_str().unwrap(), "hello");
        assert_eq!(obj.get("n").unwrap().as_f64().unwrap(), 42.0);
    }

    #[test]
    fn test_json_to_struct_roundtrip() {
        let json = serde_json::json!({"a": "x", "b": 1.0});
        let pb = json_to_struct(&json).unwrap();
        let back = struct_to_json(&pb);
        assert_eq!(json, back);
    }

    #[test]
    fn test_json_to_struct_empty_object() {
        let json = serde_json::json!({});
        let pb = json_to_struct(&json).unwrap();
        assert!(pb.fields.is_empty());
    }

    #[test]
    fn test_json_to_struct_non_object_returns_none() {
        assert!(json_to_struct(&serde_json::json!(null)).is_none());
        assert!(json_to_struct(&serde_json::json!("x")).is_none());
        assert!(json_to_struct(&serde_json::json!(1)).is_none());
    }

    #[test]
    fn test_build_response_auth_config_omits_client_secret() {
        let config = serde_json::json!({
            "client_id": "my-client",
            "client_secret": "secret123"
        });
        let out = build_response_auth_config(&config);
        let obj = out.as_object().unwrap();
        assert!(obj.get("client_secret").is_none());
        assert_eq!(obj.get("client_id").unwrap().as_str().unwrap(), "my-client");
    }

    #[test]
    fn test_build_response_auth_config_truncates_hash() {
        let config = serde_json::json!({
            "client_id": "my-client",
            "client_secret_hash": "sha256:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
        });
        let out = build_response_auth_config(&config);
        let obj = out.as_object().unwrap();
        assert!(obj.get("client_secret").is_none());
        assert_eq!(
            obj.get("client_secret_hash").unwrap().as_str().unwrap(),
            "sha256:abcdef12.."
        );
    }

    #[test]
    fn test_build_response_auth_config_passes_through_non_secret() {
        let config = serde_json::json!({
            "client_id": "cid",
            "extra_field": "value"
        });
        let out = build_response_auth_config(&config);
        let obj = out.as_object().unwrap();
        assert_eq!(obj.get("client_id").unwrap().as_str().unwrap(), "cid");
        assert_eq!(obj.get("extra_field").unwrap().as_str().unwrap(), "value");
    }

    #[test]
    fn test_build_response_auth_config_non_object_returns_clone() {
        let config = serde_json::json!("string");
        let out = build_response_auth_config(&config);
        assert_eq!(out, config);
    }
}

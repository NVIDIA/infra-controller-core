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

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use super::grpcurl::grpcurl;

/// Set identity configuration for an org. Tenant must exist first.
#[allow(clippy::too_many_arguments)]
pub async fn set_identity_configuration(
    carbide_api_addrs: &[SocketAddr],
    org_id: &str,
    issuer: &str,
    default_audience: &str,
    allowed_audiences: &[&str],
    token_ttl_sec: u32,
    subject_prefix: &str,
    enabled: bool,
) -> eyre::Result<()> {
    tracing::info!("Setting identity configuration for org {org_id}");

    let data = serde_json::json!({
        "organization_id": org_id,
        "config": {
            "enabled": enabled,
            "issuer": issuer,
            "default_audience": default_audience,
            "allowed_audiences": allowed_audiences,
            "token_ttl_sec": token_ttl_sec,
            "subject_prefix": subject_prefix,
            "rotate_key": false
        }
    });
    grpcurl(
        carbide_api_addrs,
        "SetIdentityConfiguration",
        Some(data.to_string()),
    )
    .await?;
    tracing::info!("Identity configuration set for org {org_id}");
    Ok(())
}

/// Get identity configuration for an org.
pub async fn get_identity_configuration(
    carbide_api_addrs: &[SocketAddr],
    org_id: &str,
) -> eyre::Result<IdentityConfigResponse> {
    let data = serde_json::json!({ "organization_id": org_id });
    let response = grpcurl(
        carbide_api_addrs,
        "GetIdentityConfiguration",
        Some(data.to_string()),
    )
    .await?;
    let parsed: IdentityConfigResponse = serde_json::from_str(&response)?;
    Ok(parsed)
}

/// Delete identity configuration for an org.
pub async fn delete_identity_configuration(
    carbide_api_addrs: &[SocketAddr],
    org_id: &str,
) -> eyre::Result<()> {
    tracing::info!("Deleting identity configuration for org {org_id}");

    let data = serde_json::json!({ "organization_id": org_id });
    grpcurl(
        carbide_api_addrs,
        "DeleteIdentityConfiguration",
        Some(data.to_string()),
    )
    .await?;
    tracing::info!("Identity configuration deleted for org {org_id}");
    Ok(())
}

/// Auth method config for token delegation. Matches proto oneof.
#[derive(Clone, Debug)]
pub enum AuthMethodConfig {
    None,
    ClientSecretBasic {
        client_id: String,
        client_secret: String,
    },
}

/// Set token delegation for an org. Identity config must exist first.
pub async fn set_token_delegation(
    carbide_api_addrs: &[SocketAddr],
    org_id: &str,
    token_endpoint: &str,
    auth_method_config: AuthMethodConfig,
    subject_token_audience: &str,
) -> eyre::Result<()> {
    tracing::info!("Setting token delegation for org {org_id}");

    let mut config = serde_json::json!({
        "token_endpoint": token_endpoint,
        "subject_token_audience": subject_token_audience,
    });
    // Proto oneof: variant fields are flat at config level. For "none", omit entirely.
    match &auth_method_config {
        AuthMethodConfig::None => {}
        AuthMethodConfig::ClientSecretBasic {
            client_id,
            client_secret,
        } => {
            config["clientSecretBasic"] = serde_json::json!({
                "client_id": client_id,
                "client_secret": client_secret
            });
        }
    }
    let data = serde_json::json!({
        "organization_id": org_id,
        "config": config
    });
    grpcurl(
        carbide_api_addrs,
        "SetTokenDelegation",
        Some(data.to_string()),
    )
    .await?;
    tracing::info!("Token delegation set for org {org_id}");
    Ok(())
}

/// Get token delegation for an org.
pub async fn get_token_delegation(
    carbide_api_addrs: &[SocketAddr],
    org_id: &str,
) -> eyre::Result<TokenDelegationResponse> {
    let data = serde_json::json!({ "organization_id": org_id });
    let response = grpcurl(
        carbide_api_addrs,
        "GetTokenDelegation",
        Some(data.to_string()),
    )
    .await?;
    let parsed: TokenDelegationResponse = serde_json::from_str(&response)?;
    Ok(parsed)
}

/// Delete token delegation for an org.
pub async fn delete_token_delegation(
    carbide_api_addrs: &[SocketAddr],
    org_id: &str,
) -> eyre::Result<()> {
    tracing::info!("Deleting token delegation for org {org_id}");

    let data = serde_json::json!({ "organization_id": org_id });
    grpcurl(
        carbide_api_addrs,
        "DeleteTokenDelegation",
        Some(data.to_string()),
    )
    .await?;
    tracing::info!("Token delegation deleted for org {org_id}");
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityConfigResponse {
    pub organization_id: String,
    pub enabled: bool,
    pub issuer: String,
    pub default_audience: String,
    pub allowed_audiences: Vec<String>,
    pub token_ttl_sec: u32,
    pub subject_prefix: String,
    #[serde(default)]
    pub created_at: Option<serde_json::Value>,
    #[serde(default)]
    pub updated_at: Option<serde_json::Value>,
    #[serde(default)]
    pub key_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenDelegationResponse {
    pub organization_id: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub auth_method_config: Option<serde_json::Value>,
    #[serde(default)]
    pub subject_token_audience: Option<String>,
    #[serde(default)]
    pub created_at: Option<serde_json::Value>,
    #[serde(default)]
    pub updated_at: Option<serde_json::Value>,
}

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

//! gRPC handler for per-org identity configuration (SetIdentityConfiguration).
//! Persists config to tenant_identity_config table.

use ::rpc::forge::{IdentityConfigRequest, IdentityConfigResponse};
use ::rpc::Timestamp;
use db::tenant;
use db::tenant_identity_config;
use db::WithTransaction;
use tonic::{Request, Response, Status};

use crate::api::{Api, log_request_data};
use crate::auth::AuthContext;

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

    let allowed: Vec<String> = req.allowed_audiences.into_iter().map(|s| s).collect();
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

    let allowed_audiences: Vec<String> = serde_json::from_value(cfg.allowed_audiences.clone())
        .unwrap_or_default();

    Ok(Response::new(IdentityConfigResponse {
        org_id: cfg.organization_id,
        enabled: cfg.enabled,
        issuer: cfg.issuer,
        default_audience: cfg.default_audience,
        allowed_audiences,
        token_ttl: cfg.token_ttl as u32,
        subject_domain: cfg.subject_domain_prefix,
        updated_at: Some(Timestamp::from(cfg.updated_at)),
        key_id: cfg.key_id,
    }))
}

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

//! Tenant identity config for SPIFFE JWT-SVID machine identity.
//! Stores per-org identity config and signing keys in `tenant_identity_config` table.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use crate::{DatabaseError, DatabaseResult};

#[derive(Debug, sqlx::FromRow)]
pub struct TenantIdentityConfig {
    pub organization_id: String,
    pub issuer: String,
    pub default_audience: String,
    pub allowed_audiences: serde_json::Value,
    pub token_ttl: i32,
    pub subject_domain_prefix: String,
    pub enabled: bool,
    pub updated_at: DateTime<Utc>,
    pub encrypted_signing_key: String,
    pub signing_key_public: String,
    pub key_id: String,
    pub algorithm: String,
    pub master_key_id: String,
}

/// Set identity config for an org. On first create, generates a placeholder key.
/// Caller must ensure tenant exists and global machine-identity is enabled.
pub async fn set(
    org_id: &str,
    issuer: &str,
    default_audience: &str,
    allowed_audiences: &[String],
    token_ttl: u32,
    subject_domain_prefix: &str,
    enabled: bool,
    rotate_key: bool,
    algorithm: &str,
    master_key_id: &str,
    txn: &mut PgConnection,
) -> DatabaseResult<TenantIdentityConfig> {
    let allowed: Vec<String> = if allowed_audiences.is_empty() {
        vec![default_audience.to_string()]
    } else {
        if !allowed_audiences.iter().any(|a| a == default_audience) {
            return Err(DatabaseError::InvalidArgument(
                "default_audience must be in allowed_audiences".into(),
            ));
        }
        allowed_audiences.to_vec()
    };
    let allowed_json = serde_json::to_value(&allowed)
        .map_err(|e| DatabaseError::InvalidArgument(e.to_string()))?;

    let token_ttl_i32: i32 = token_ttl
        .try_into()
        .map_err(|_| DatabaseError::InvalidArgument("token_ttl out of range".into()))?;

    // Bounds validation is done by the handler using site config (token_ttl_min, token_ttl_max).

    let existing = find(org_id, &mut *txn).await?;
    let (key_id, encrypted_key, public_key) = if existing.is_none() || rotate_key {
        // Generate new key pair (placeholder: use deterministic placeholder for rough impl)
        let key_id = uuid::Uuid::new_v4().to_string();
        let encrypted_key = "PLACEHOLDER_ENCRYPTED_KEY".to_string();
        let public_key = "PLACEHOLDER_PUBLIC_KEY".to_string();
        (key_id, encrypted_key, public_key)
    } else {
        let ex = existing.unwrap();
        (ex.key_id, ex.encrypted_signing_key, ex.signing_key_public)
    };

    let query = r#"
        INSERT INTO tenant_identity_config (
            organization_id, issuer, default_audience, allowed_audiences,
            token_ttl, subject_domain_prefix, enabled, updated_at,
            encrypted_signing_key, signing_key_public, key_id, algorithm, master_key_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), $8, $9, $10, $11, $12)
        ON CONFLICT (organization_id) DO UPDATE SET
            issuer = EXCLUDED.issuer,
            default_audience = EXCLUDED.default_audience,
            allowed_audiences = EXCLUDED.allowed_audiences,
            token_ttl = EXCLUDED.token_ttl,
            subject_domain_prefix = EXCLUDED.subject_domain_prefix,
            enabled = EXCLUDED.enabled,
            updated_at = NOW(),
            encrypted_signing_key = EXCLUDED.encrypted_signing_key,
            signing_key_public = EXCLUDED.signing_key_public,
            key_id = EXCLUDED.key_id,
            algorithm = EXCLUDED.algorithm,
            master_key_id = EXCLUDED.master_key_id
        RETURNING *
    "#;

    sqlx::query_as(query)
        .bind(org_id)
        .bind(issuer)
        .bind(default_audience)
        .bind(&allowed_json)
        .bind(token_ttl_i32)
        .bind(subject_domain_prefix)
        .bind(enabled)
        .bind(&encrypted_key)
        .bind(&public_key)
        .bind(&key_id)
        .bind(algorithm)
        .bind(master_key_id)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

pub async fn find(
    org_id: &str,
    txn: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
) -> DatabaseResult<Option<TenantIdentityConfig>> {
    let query = "SELECT * FROM tenant_identity_config WHERE organization_id = $1";
    sqlx::query_as(query)
        .bind(org_id)
        .fetch_optional(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

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

use carbide_uuid::machine::MachineId;
use chrono::{Duration as ChronoDuration, Utc};
use model::tenant::identity_config::SigningKeyPublicV1;
use model::tenant::{
    EncryptedTokenDelegationAuthConfig, IdentityConfig, SigningKeyMaterial, TenantIdentityConfig,
    TenantIdentityCurrentSigningKeySlot, TenantOrganizationId, TokenDelegation,
    TokenDelegationAuthMethod,
};
use sqlx::PgConnection;
use sqlx::types::Json;

use crate::{DatabaseError, DatabaseResult};

/// Column expressions for mapping a row into [`TenantIdentityConfig`], optionally qualified
/// (e.g. `Some("tic")` → `tic.organization_id`, …) for `FROM tenant_identity_config tic` joins.
fn tenant_identity_row_select_expr(table_alias: Option<&str>) -> String {
    const COLS: &[&str] = &[
        "organization_id",
        "issuer::text AS issuer",
        "default_audience::text AS default_audience",
        "allowed_audiences",
        "token_ttl_sec",
        "subject_prefix::text AS subject_prefix",
        "enabled",
        "created_at",
        "updated_at",
        "encrypted_signing_key_1",
        "encrypted_signing_key_2",
        "signing_key_public_1",
        "signing_key_public_2",
        "current_signing_key_slot",
        "signing_key_overlap_sec",
        "non_active_slot_expires_at",
        "encryption_key_id::text AS encryption_key_id",
        "token_endpoint::text AS token_endpoint",
        "auth_method",
        "encrypted_auth_method_config",
        "subject_token_audience::text AS subject_token_audience",
        "token_delegation_created_at",
    ];
    let prefix = table_alias.map_or_else(String::new, |a| format!("{a}."));
    COLS.iter()
        .map(|c| format!("{prefix}{c}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// After `non_active_slot_expires_at`, clears the non-current slot (public + private ciphertext).
pub async fn gc_expired_non_active_signing_key(
    org_id: &TenantOrganizationId,
    txn: &mut PgConnection,
) -> DatabaseResult<()> {
    let Some(row) = find(org_id, txn).await? else {
        return Ok(());
    };
    let Some(expires) = row.non_active_slot_expires_at else {
        return Ok(());
    };
    if expires > Utc::now() {
        return Ok(());
    }

    let slot_to_clear = row.current_signing_key_slot.other();
    let stmt = match slot_to_clear {
        TenantIdentityCurrentSigningKeySlot::SigningKey1 => {
            "UPDATE tenant_identity_config SET \
                signing_key_public_1 = NULL, \
                encrypted_signing_key_1 = NULL, \
                non_active_slot_expires_at = NULL, \
                updated_at = NOW() \
                WHERE organization_id = $1"
        }
        TenantIdentityCurrentSigningKeySlot::SigningKey2 => {
            "UPDATE tenant_identity_config SET \
                signing_key_public_2 = NULL, \
                encrypted_signing_key_2 = NULL, \
                non_active_slot_expires_at = NULL, \
                updated_at = NOW() \
                WHERE organization_id = $1"
        }
    };
    sqlx::query(stmt)
        .bind(org_id.as_str())
        .execute(txn)
        .await
        .map_err(|e| DatabaseError::query("gc_expired_non_active_signing_key", e))?;
    Ok(())
}

fn signing_public_json_from_material(
    km: &SigningKeyMaterial,
) -> DatabaseResult<Json<SigningKeyPublicV1>> {
    let doc = SigningKeyPublicV1::es256_from_public_pem(km.signing_key_public.as_str())
        .map_err(DatabaseError::InvalidArgument)?;
    doc.validate().map_err(DatabaseError::InvalidArgument)?;
    Ok(Json(doc))
}

/// Set identity config for an org.
/// When creating new or rotating key, caller must provide `key_material` (generated key pair, encrypted private key).
/// Caller must ensure tenant exists and global machine-identity is enabled.
/// `site_signing_key_overlap_default_sec` is used when `config.signing_key_overlap_sec` is `None` to compute
/// [`non_active_slot_expires_at`] on `rotate_key`.
pub async fn set(
    org_id: &TenantOrganizationId,
    config: &IdentityConfig,
    key_material: Option<SigningKeyMaterial>,
    site_signing_key_overlap_default_sec: u32,
    txn: &mut PgConnection,
) -> DatabaseResult<TenantIdentityConfig> {
    gc_expired_non_active_signing_key(org_id, txn).await?;

    let allowed: Vec<String> = if config.allowed_audiences.is_empty() {
        vec![config.default_audience.clone()]
    } else {
        config.allowed_audiences.clone()
    };

    let token_ttl_i32: i32 = config
        .token_ttl_sec
        .try_into()
        .map_err(|_| DatabaseError::InvalidArgument("token_ttl out of range".into()))?;

    let existing = find(org_id, &mut *txn).await?;

    let overlap_for_expiry = match config.signing_key_overlap_sec {
        None => site_signing_key_overlap_default_sec,
        Some(v) => u32::try_from(v).map_err(|_| {
            DatabaseError::InvalidArgument(
                "signing_key_overlap_sec must be non-negative and fit in u32".into(),
            )
        })?,
    };

    let overlap_i32 = config.signing_key_overlap_sec;

    let (enc1, enc2, pub1, pub2, current_slot, non_active_expires): (
        Option<_>,
        Option<_>,
        Option<_>,
        Option<_>,
        TenantIdentityCurrentSigningKeySlot,
        Option<_>,
    ) = match (&existing, config.rotate_key, key_material) {
        (None, _, None) | (_, true, None) => {
            return Err(DatabaseError::InvalidArgument(
                "key_material is required when creating or rotating signing key".into(),
            ));
        }
        (Some(ex), true, Some(km)) => {
            if ex.signing_key_public_1.is_some()
                && ex.signing_key_public_2.is_some()
                && ex
                    .non_active_slot_expires_at
                    .is_some_and(|t| t > Utc::now())
            {
                return Err(DatabaseError::InvalidArgument(
                    "cannot rotate signing key while the previous key is still in the overlap period"
                        .into(),
                ));
            }
            let pub_doc = signing_public_json_from_material(&km)?;
            let new_enc = km.encrypted_signing_key;
            let other = ex.current_signing_key_slot.other();
            let expires = Some(Utc::now() + ChronoDuration::seconds(i64::from(overlap_for_expiry)));
            match other {
                TenantIdentityCurrentSigningKeySlot::SigningKey1 => (
                    Some(new_enc),
                    ex.encrypted_signing_key_2.clone(),
                    Some(pub_doc),
                    ex.signing_key_public_2.clone(),
                    TenantIdentityCurrentSigningKeySlot::SigningKey1,
                    expires,
                ),
                TenantIdentityCurrentSigningKeySlot::SigningKey2 => (
                    ex.encrypted_signing_key_1.clone(),
                    Some(new_enc),
                    ex.signing_key_public_1.clone(),
                    Some(pub_doc),
                    TenantIdentityCurrentSigningKeySlot::SigningKey2,
                    expires,
                ),
            }
        }
        (None, _, Some(km)) => {
            let pub_doc = signing_public_json_from_material(&km)?;
            let new_enc = km.encrypted_signing_key;
            (
                Some(new_enc),
                None,
                Some(pub_doc),
                None,
                TenantIdentityCurrentSigningKeySlot::SigningKey1,
                None,
            )
        }
        (Some(ex), false, None) => (
            ex.encrypted_signing_key_1.clone(),
            ex.encrypted_signing_key_2.clone(),
            ex.signing_key_public_1.clone(),
            ex.signing_key_public_2.clone(),
            ex.current_signing_key_slot,
            ex.non_active_slot_expires_at,
        ),
        (Some(_), false, Some(_)) => {
            return Err(DatabaseError::InvalidArgument(
                "key_material must not be set when rotate_key is false".into(),
            ));
        }
    };

    let returning = tenant_identity_row_select_expr(None);
    let query = format!(
        r#"
        INSERT INTO tenant_identity_config (
            organization_id, issuer, default_audience, allowed_audiences,
            token_ttl_sec, subject_prefix, enabled, created_at, updated_at,
            encrypted_signing_key_1, encrypted_signing_key_2,
            signing_key_public_1, signing_key_public_2,
            current_signing_key_slot, signing_key_overlap_sec, non_active_slot_expires_at,
            encryption_key_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW(), $8, $9, $10, $11, $12, $13, $14, $15)
        ON CONFLICT (organization_id) DO UPDATE SET
            issuer = EXCLUDED.issuer,
            default_audience = EXCLUDED.default_audience,
            allowed_audiences = EXCLUDED.allowed_audiences,
            token_ttl_sec = EXCLUDED.token_ttl_sec,
            subject_prefix = EXCLUDED.subject_prefix,
            enabled = EXCLUDED.enabled,
            updated_at = NOW(),
            encrypted_signing_key_1 = EXCLUDED.encrypted_signing_key_1,
            encrypted_signing_key_2 = EXCLUDED.encrypted_signing_key_2,
            signing_key_public_1 = EXCLUDED.signing_key_public_1,
            signing_key_public_2 = EXCLUDED.signing_key_public_2,
            current_signing_key_slot = EXCLUDED.current_signing_key_slot,
            signing_key_overlap_sec = EXCLUDED.signing_key_overlap_sec,
            non_active_slot_expires_at = EXCLUDED.non_active_slot_expires_at,
            encryption_key_id = EXCLUDED.encryption_key_id
        RETURNING {returning}
    "#
    );

    sqlx::query_as(&query)
        .bind(org_id.as_str())
        .bind(&config.issuer)
        .bind(&config.default_audience)
        .bind(Json(allowed))
        .bind(token_ttl_i32)
        .bind(&config.subject_prefix)
        .bind(config.enabled)
        .bind(enc1)
        .bind(enc2)
        .bind(pub1)
        .bind(pub2)
        .bind(current_slot)
        .bind(overlap_i32)
        .bind(non_active_expires)
        .bind(&config.encryption_key_id)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))
}

pub async fn find(
    org_id: &TenantOrganizationId,
    txn: &mut PgConnection,
) -> DatabaseResult<Option<TenantIdentityConfig>> {
    let select_list = tenant_identity_row_select_expr(None);
    let query =
        format!("SELECT {select_list} FROM tenant_identity_config WHERE organization_id = $1");
    sqlx::query_as(&query)
        .bind(org_id.as_str())
        .fetch_optional(txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))
}

pub async fn find_by_machine_id(
    txn: &mut PgConnection,
    machine_id: &MachineId,
) -> DatabaseResult<TenantIdentityConfig> {
    const ORG_FOR_GC: &str = r#"
SELECT tic.organization_id::text
FROM tenant_identity_config tic
INNER JOIN instances i ON tic.organization_id = i.tenant_org
WHERE i.machine_id = $1 AND i.deleted IS NULL AND tic.enabled = true
"#;
    if let Some(org_raw) = sqlx::query_scalar::<_, String>(ORG_FOR_GC)
        .bind(machine_id)
        .fetch_optional(&mut *txn)
        .await
        .map_err(|e| DatabaseError::query(ORG_FOR_GC, e))?
    {
        match org_raw.parse::<TenantOrganizationId>() {
            Ok(oid) => {
                gc_expired_non_active_signing_key(&oid, txn).await?;
            }
            Err(e) => {
                tracing::warn!(
                    %machine_id,
                    organization_id = %org_raw,
                    error = %e,
                    "tenant_identity_config.organization_id from join failed TenantOrganizationId parse; skipping non-active signing key GC"
                );
            }
        }
    }

    let select_list = tenant_identity_row_select_expr(Some("tic"));
    let query = format!(
        r#"
SELECT {select_list}
FROM tenant_identity_config tic
INNER JOIN instances i ON tic.organization_id = i.tenant_org
WHERE i.machine_id = $1 AND i.deleted IS NULL AND tic.enabled = true
"#
    );
    let row = sqlx::query_as::<_, TenantIdentityConfig>(&query)
        .bind(machine_id)
        .fetch_optional(&mut *txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))?;
    let Some(cfg) = row else {
        return Err(DatabaseError::NotFoundError {
            kind: "machine_identity",
            id: machine_id.to_string(),
        });
    };
    Ok(cfg)
}

/// Set token delegation for an org. Identity config must exist first.
/// `encrypted_auth_method_config` must be standard base64 of JSON envelope v1 from `key_encryption::encrypt`
/// over the UTF-8 JSON produced by [`TokenDelegation::to_db_format`].
pub async fn set_token_delegation(
    org_id: &TenantOrganizationId,
    config: &TokenDelegation,
    auth_method: TokenDelegationAuthMethod,
    encrypted_auth_method_config: &EncryptedTokenDelegationAuthConfig,
    txn: &mut PgConnection,
) -> DatabaseResult<TenantIdentityConfig> {
    let returning = tenant_identity_row_select_expr(None);
    let query = format!(
        r#"
        UPDATE tenant_identity_config
        SET token_endpoint = $2, auth_method = $3, encrypted_auth_method_config = $4,
            subject_token_audience = $5, updated_at = NOW(),
            token_delegation_created_at = COALESCE(token_delegation_created_at, NOW())
        WHERE organization_id = $1
        RETURNING {returning}
    "#
    );
    let row = sqlx::query_as(&query)
        .bind(org_id.as_str())
        .bind(&config.token_endpoint)
        .bind(auth_method)
        .bind(encrypted_auth_method_config.as_str())
        .bind(Some(config.subject_token_audience.as_str()))
        .fetch_optional(txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))?;
    row.ok_or_else(|| DatabaseError::NotFoundError {
        kind: "tenant_identity_config",
        id: org_id.as_str().to_string(),
    })
}

/// Delete identity config for an org (removes the entire row).
pub async fn delete(org_id: &TenantOrganizationId, txn: &mut PgConnection) -> DatabaseResult<bool> {
    let result = sqlx::query("DELETE FROM tenant_identity_config WHERE organization_id = $1")
        .bind(org_id.as_str())
        .execute(txn)
        .await
        .map_err(|e| DatabaseError::query("DELETE tenant_identity_config", e))?;
    Ok(result.rows_affected() > 0)
}

/// Clear token delegation for an org.
pub async fn delete_token_delegation(
    org_id: &TenantOrganizationId,
    txn: &mut PgConnection,
) -> DatabaseResult<Option<TenantIdentityConfig>> {
    let returning = tenant_identity_row_select_expr(None);
    let query = format!(
        r#"
        UPDATE tenant_identity_config
        SET token_endpoint = NULL, auth_method = NULL, encrypted_auth_method_config = NULL,
            subject_token_audience = NULL, token_delegation_created_at = NULL, updated_at = NOW()
        WHERE organization_id = $1
        RETURNING {returning}
    "#
    );
    sqlx::query_as(&query)
        .bind(org_id.as_str())
        .fetch_optional(txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_secrets::key_encryption;
    use model::metadata::Metadata;
    use model::tenant::identity_config::SigningAlgorithm;
    use model::tenant::{
        IdentityConfig, KeyId, TokenDelegation, TokenDelegationAuthMethod,
        TokenDelegationAuthMethodConfig,
    };

    use super::*;
    use crate::tenant;

    fn test_org_id() -> TenantOrganizationId {
        "IdentityConfigTestOrg".parse().unwrap()
    }

    async fn ensure_tenant(txn: &mut PgConnection, org_id: &TenantOrganizationId) {
        if tenant::find(org_id.as_str(), false, txn)
            .await
            .unwrap()
            .is_none()
        {
            tenant::create_and_persist(
                org_id.as_str().to_string(),
                Metadata {
                    name: "Test Org".to_string(),
                    description: "".to_string(),
                    labels: HashMap::new(),
                },
                None,
                txn,
            )
            .await
            .unwrap();
        }
    }

    fn placeholder_key_material() -> SigningKeyMaterial {
        let pem = "PLACEHOLDER_PUBLIC_KEY";
        SigningKeyMaterial {
            key_id: KeyId::from_public_key_material(pem),
            encrypted_signing_key: "PLACEHOLDER_ENCRYPTED_KEY".parse().unwrap(),
            signing_key_public: pem.parse().unwrap(),
        }
    }

    #[crate::sqlx_test]
    async fn test_tenant_identity_config_set_find_delete(pool: sqlx::PgPool) {
        let mut txn = pool.begin().await.unwrap();
        let org_id = test_org_id();
        ensure_tenant(&mut txn, &org_id).await;

        let config = IdentityConfig {
            issuer: "https://issuer.example.com".parse().unwrap(),
            default_audience: "api".to_string(),
            allowed_audiences: vec!["api".to_string(), "audience2".to_string()],
            token_ttl_sec: 3600,
            subject_prefix: "spiffe://issuer.example.com/org-x".to_string(),
            enabled: true,
            rotate_key: false,
            algorithm: SigningAlgorithm::Es256,
            encryption_key_id: "test-master".parse().unwrap(),
            signing_key_overlap_sec: None,
        };

        let key_material = placeholder_key_material();
        let cfg = set(&org_id, &config, Some(key_material), 3600, &mut txn)
            .await
            .unwrap();
        assert_eq!(cfg.issuer.as_str(), "https://issuer.example.com");
        assert_eq!(cfg.default_audience, "api");
        assert_eq!(cfg.allowed_audiences.0, ["api", "audience2"]);
        assert_eq!(cfg.token_ttl_sec, 3600);
        assert_eq!(cfg.subject_prefix, "spiffe://issuer.example.com/org-x");
        assert!(cfg.enabled);
        assert_eq!(cfg.encryption_key_id.as_str(), "test-master");
        assert_eq!(
            cfg.current_signing_key_slot,
            TenantIdentityCurrentSigningKeySlot::SigningKey1
        );
        assert!(cfg.signing_key_public_1.is_some());

        let found = find(&org_id, &mut txn).await.unwrap().unwrap();
        assert_eq!(found.issuer, cfg.issuer);
        assert_eq!(found.default_audience, cfg.default_audience);
        assert_eq!(found.allowed_audiences.0, cfg.allowed_audiences.0);
        assert_eq!(found.token_ttl_sec, cfg.token_ttl_sec);
        assert_eq!(found.subject_prefix, cfg.subject_prefix);
        assert_eq!(found.enabled, cfg.enabled);
        assert_eq!(found.encryption_key_id, cfg.encryption_key_id);
        assert_eq!(found.current_signing_key_slot, cfg.current_signing_key_slot);

        let deleted = delete(&org_id, &mut txn).await.unwrap();
        assert!(deleted);

        let not_found = find(&org_id, &mut txn).await.unwrap();
        assert!(not_found.is_none());
    }

    #[crate::sqlx_test]
    async fn test_token_delegation_set_get_delete(pool: sqlx::PgPool) {
        let mut txn = pool.begin().await.unwrap();
        let org_id = test_org_id();
        ensure_tenant(&mut txn, &org_id).await;

        let config = IdentityConfig {
            issuer: "https://issuer.example.com".parse().unwrap(),
            default_audience: "api".to_string(),
            allowed_audiences: vec!["api".to_string()],
            token_ttl_sec: 3600,
            subject_prefix: "spiffe://issuer.example.com".to_string(),
            enabled: true,
            rotate_key: false,
            algorithm: SigningAlgorithm::Es256,
            encryption_key_id: "test-master".parse().unwrap(),
            signing_key_overlap_sec: None,
        };
        let key_material = placeholder_key_material();
        set(&org_id, &config, Some(key_material), 3600, &mut txn)
            .await
            .unwrap();

        let token_delegation = TokenDelegation {
            token_endpoint: "https://auth.example.com/token".to_string(),
            subject_token_audience: "https://api.example.com".to_string(),
            auth_method_config: TokenDelegationAuthMethodConfig::ClientSecretBasic {
                client_id: "test-client".to_string(),
                client_secret: "test-secret".to_string(),
            },
        };
        let (auth_method, plaintext_json) = token_delegation.to_db_format();
        let enc_key: key_encryption::Aes256Key = [0u8; 32];
        let enc =
            key_encryption::encrypt(plaintext_json.as_bytes(), &enc_key, "test-master").unwrap();
        let enc: EncryptedTokenDelegationAuthConfig = enc.try_into().unwrap();
        let cfg = set_token_delegation(&org_id, &token_delegation, auth_method, &enc, &mut txn)
            .await
            .unwrap();
        assert_eq!(
            cfg.token_endpoint.as_deref(),
            Some("https://auth.example.com/token")
        );
        assert_eq!(
            cfg.auth_method,
            Some(TokenDelegationAuthMethod::ClientSecretBasic)
        );
        assert_eq!(
            cfg.subject_token_audience.as_deref(),
            Some("https://api.example.com")
        );

        let cleared = delete_token_delegation(&org_id, &mut txn)
            .await
            .unwrap()
            .unwrap();
        assert!(cleared.token_endpoint.is_none());
        assert!(cleared.auth_method.is_none());
    }
}

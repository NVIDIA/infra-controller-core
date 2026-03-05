-- Tenant identity config table for SPIFFE JWT-SVID machine identity.
-- Stores per-org identity config, signing key pairs, and optional token delegation.
-- Private key is encrypted with a master key.
-- Token delegation columns are nullable when an org does not use delegation.

CREATE TABLE tenant_identity_config (
    organization_id   VARCHAR(255) PRIMARY KEY REFERENCES tenants(organization_id) ON DELETE CASCADE,
    -- Identity config (from PUT identity/config)
    issuer                   VARCHAR(512) NOT NULL,
    default_audience         VARCHAR(255) NOT NULL,
    allowed_audiences        JSONB NOT NULL,
    token_ttl                INTEGER NOT NULL,
    subject_domain_prefix    VARCHAR(255) NOT NULL,
    enabled                  BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Signing key (generated on first PUT identity/config)
    encrypted_signing_key    TEXT NOT NULL,
    signing_key_public       VARCHAR(255) NOT NULL,
    key_id                   VARCHAR(255) NOT NULL,
    algorithm                VARCHAR(255) NOT NULL,
    master_key_id            VARCHAR(255) NOT NULL,
    -- Token delegation (from PUT identity/token-delegation, optional)
    token_endpoint           VARCHAR(512),
    auth_method              VARCHAR(64),
    client_id                VARCHAR(255),
    encrypted_client_secret  TEXT,
    subject_token_audience   VARCHAR(255)
);

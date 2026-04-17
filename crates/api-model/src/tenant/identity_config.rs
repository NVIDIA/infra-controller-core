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
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// JWT `alg` for per-tenant signing keys. Only ES256 (ECDSA P-256) is implemented end-to-end.
pub const TENANT_IDENTITY_SIGNING_JWT_ALG: &str = "ES256";

/// Per-tenant JWT signing algorithm persisted in `tenant_identity_config.algorithm` and site config.
/// Only [`SigningAlgorithm::Es256`] is implemented end-to-end today; the enum leaves room for more JOSE `alg` values later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SigningAlgorithm {
    Es256,
}

impl Default for SigningAlgorithm {
    fn default() -> Self {
        Self::Es256
    }
}

impl SigningAlgorithm {
    #[must_use]
    pub const fn as_jwt_alg_str(self) -> &'static str {
        match self {
            Self::Es256 => TENANT_IDENTITY_SIGNING_JWT_ALG,
        }
    }
}

impl Display for SigningAlgorithm {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_jwt_alg_str())
    }
}

/// Unsupported or unknown `algorithm` string from config or the database.
#[derive(thiserror::Error, Debug)]
#[error(
    "unsupported tenant identity signing algorithm {0:?} (only {TENANT_IDENTITY_SIGNING_JWT_ALG} is implemented)"
)]
pub struct UnsupportedTenantSigningAlgorithm(pub String);

impl FromStr for SigningAlgorithm {
    type Err = UnsupportedTenantSigningAlgorithm;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            TENANT_IDENTITY_SIGNING_JWT_ALG => Ok(Self::Es256),
            other => Err(UnsupportedTenantSigningAlgorithm(other.to_string())),
        }
    }
}

impl sqlx::Type<sqlx::Postgres> for SigningAlgorithm {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("VARCHAR")
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for SigningAlgorithm {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <String as sqlx::Encode<'_, sqlx::Postgres>>::encode_by_ref(&self.to_string(), buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for SigningAlgorithm {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        s.parse()
            .map_err(|e: UnsupportedTenantSigningAlgorithm| sqlx::Error::Decode(Box::new(e)).into())
    }
}

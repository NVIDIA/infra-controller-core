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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::types::Json;
use sqlx::{FromRow, Row};

use crate::rack_type::RackHardwareType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RackFirmware {
    pub id: String,
    pub rack_hardware_type: RackHardwareType,
    pub config: Json<serde_json::Value>,
    pub available: bool,
    pub is_default: bool,
    pub parsed_components: Option<Json<serde_json::Value>>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl<'r> FromRow<'r, PgRow> for RackFirmware {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(RackFirmware {
            id: row.try_get("id")?,
            rack_hardware_type: row.try_get("rack_hardware_type")?,
            config: row.try_get("config")?,
            available: row.try_get("available")?,
            is_default: row.try_get("is_default")?,
            parsed_components: row.try_get("parsed_components")?,
            created: row.try_get("created")?,
            updated: row.try_get("updated")?,
        })
    }
}

/// Filter criteria for searching rack firmware configurations.
#[derive(Clone, Debug, Default)]
pub struct RackFirmwareSearchFilter {
    pub only_available: bool,
    pub rack_hardware_type: Option<RackHardwareType>,
}

/// A record of a rack firmware apply operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RackFirmwareApplyHistoryRecord {
    pub firmware_id: String,
    pub rack_id: String,
    pub firmware_type: String,
    pub rack_hardware_type: RackHardwareType,
    pub applied_at: DateTime<Utc>,
    pub firmware_available: bool,
}

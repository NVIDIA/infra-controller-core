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

use crate::rack_firmware::{
    RackFirmware, RackFirmwareApplyHistoryRecord, RackFirmwareSearchFilter,
};
use crate::rack_type::RackHardwareType;

impl From<&RackFirmware> for rpc::forge::RackFirmware {
    fn from(db: &RackFirmware) -> Self {
        let parsed_components = db
            .parsed_components
            .as_ref()
            .map(|p| p.0.to_string())
            .unwrap_or_else(|| "{}".to_string());

        rpc::forge::RackFirmware {
            id: db.id.clone(),
            config_json: db.config.0.to_string(),
            available: db.available,
            created: db.created.format("%Y-%m-%d %H:%M:%S").to_string(),
            updated: db.updated.format("%Y-%m-%d %H:%M:%S").to_string(),
            parsed_components,
            rack_hardware_type: Some(db.rack_hardware_type.clone().into()),
            is_default: db.is_default,
        }
    }
}

impl From<rpc::forge::RackFirmwareSearchFilter> for RackFirmwareSearchFilter {
    fn from(filter: rpc::forge::RackFirmwareSearchFilter) -> Self {
        let rack_hardware_type = filter
            .rack_hardware_type
            .filter(|t| !t.value.is_empty())
            .map(RackHardwareType::from);

        RackFirmwareSearchFilter {
            only_available: filter.only_available,
            rack_hardware_type,
        }
    }
}

impl From<RackFirmwareApplyHistoryRecord> for rpc::forge::RackFirmwareHistoryRecord {
    fn from(record: RackFirmwareApplyHistoryRecord) -> Self {
        rpc::forge::RackFirmwareHistoryRecord {
            firmware_id: record.firmware_id,
            rack_id: record.rack_id,
            firmware_type: record.firmware_type,
            applied_at: record.applied_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            firmware_available: record.firmware_available,
            rack_hardware_type: Some(record.rack_hardware_type.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_filter_from_proto_with_hardware_type() {
        let proto = rpc::forge::RackFirmwareSearchFilter {
            only_available: true,
            rack_hardware_type: Some(rpc::common::RackHardwareType {
                value: "any".to_string(),
            }),
        };
        let filter = RackFirmwareSearchFilter::from(proto);
        assert!(filter.only_available);
        assert_eq!(filter.rack_hardware_type, Some(RackHardwareType::any()));
    }

    #[test]
    fn test_search_filter_from_proto_none_becomes_none() {
        let proto = rpc::forge::RackFirmwareSearchFilter {
            only_available: false,
            rack_hardware_type: None,
        };
        let filter = RackFirmwareSearchFilter::from(proto);
        assert!(!filter.only_available);
        assert_eq!(filter.rack_hardware_type, None);
    }

    #[test]
    fn test_search_filter_from_proto_empty_value_becomes_none() {
        let proto = rpc::forge::RackFirmwareSearchFilter {
            only_available: false,
            rack_hardware_type: Some(rpc::common::RackHardwareType {
                value: String::new(),
            }),
        };
        let filter = RackFirmwareSearchFilter::from(proto);
        assert!(!filter.only_available);
        assert_eq!(filter.rack_hardware_type, None);
    }

    #[test]
    fn test_search_filter_from_proto_specific_type() {
        let proto = rpc::forge::RackFirmwareSearchFilter {
            only_available: true,
            rack_hardware_type: Some(rpc::common::RackHardwareType {
                value: "dsx_gb200nvl_72x1".to_string(),
            }),
        };
        let filter = RackFirmwareSearchFilter::from(proto);
        assert!(filter.only_available);
        assert_eq!(
            filter.rack_hardware_type,
            Some(RackHardwareType::from("dsx_gb200nvl_72x1"))
        );
    }
}

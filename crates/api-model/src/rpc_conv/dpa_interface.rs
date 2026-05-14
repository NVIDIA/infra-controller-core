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

use std::convert::TryFrom;
use std::str::FromStr;

use config_version::Versioned;
use itertools::Itertools;
use mac_address::MacAddress;
use rpc::errors::RpcDataConversionError;

use crate::dpa_interface::{DpaInterface, DpaInterfaceSnapshotPgJson, NewDpaInterface};
use crate::state_history::StateHistoryRecord;

impl TryFrom<rpc::forge::DpaInterfaceCreationRequest> for NewDpaInterface {
    type Error = RpcDataConversionError;

    fn try_from(value: rpc::forge::DpaInterfaceCreationRequest) -> Result<Self, Self::Error> {
        let machine_id = value
            .machine_id
            .ok_or(RpcDataConversionError::MissingArgument("id"))?;
        let mac_address = MacAddress::from_str(&value.mac_addr)
            .map_err(|_| RpcDataConversionError::InvalidMacAddress(value.mac_addr.to_string()))?;
        Ok(NewDpaInterface {
            machine_id,
            mac_address,
            device_type: value.device_type,
            pci_name: value.pci_name,
        })
    }
}

impl From<DpaInterface> for rpc::forge::DpaInterface {
    fn from(src: DpaInterface) -> Self {
        let (controller_state, controller_state_version) = src.controller_state.take();
        let (network_config, network_config_version) = src.network_config.take();

        let outcome = match src.controller_state_outcome {
            Some(psho) => psho.to_string(),
            None => "None".to_string(),
        };

        let network_status_observation = match src.network_status_observation {
            Some(nso) => nso.to_string(),
            None => "None".to_string(),
        };

        let cstate = match src.card_state {
            Some(cs) => cs.to_string(),
            None => "None".to_string(),
        };

        let underlay = match src.underlay_ip {
            Some(ip) => ip.to_string(),
            None => String::new(),
        };

        let overlay = match src.overlay_ip {
            Some(ip) => ip.to_string(),
            None => String::new(),
        };

        let history: Vec<rpc::forge::StateHistoryRecord> = src
            .history
            .into_iter()
            .sorted_by(|s1: &StateHistoryRecord, s2: &StateHistoryRecord| {
                Ord::cmp(&s1.state_version.timestamp(), &s2.state_version.timestamp())
            })
            .map(Into::into)
            .collect();

        rpc::forge::DpaInterface {
            id: Some(src.id),
            created: Some(src.created.into()),
            updated: Some(src.updated.into()),
            deleted: src.deleted.map(|t| t.into()),
            last_hb_time: Some(src.last_hb_time.into()),
            mac_addr: src.mac_address.to_string(),
            machine_id: Some(src.machine_id),
            controller_state: controller_state.to_string(),
            controller_state_version: controller_state_version.to_string(),
            network_config: network_config.to_string(),
            network_config_version: network_config_version.to_string(),
            controller_state_outcome: outcome,
            network_status_observation,
            history,
            card_state: cstate,
            pci_name: src.pci_name,
            underlay_ip: underlay,
            overlay_ip: overlay,
            mlxconfig_profile: src.mlxconfig_profile,
        }
    }
}

impl TryFrom<DpaInterfaceSnapshotPgJson> for DpaInterface {
    type Error = sqlx::Error;

    fn try_from(value: DpaInterfaceSnapshotPgJson) -> sqlx::Result<Self> {
        Ok(Self {
            id: value.id,
            machine_id: value.machine_id,
            mac_address: value.mac_address,
            created: value.created,
            updated: value.updated,
            deleted: value.deleted,
            last_hb_time: value.last_hb_time,
            controller_state: Versioned {
                value: value.controller_state,
                version: value.controller_state_version.parse().map_err(|e| {
                    sqlx::error::Error::ColumnDecode {
                        index: "controller_state_version".to_string(),
                        source: Box::new(e),
                    }
                })?,
            },
            controller_state_outcome: value.controller_state_outcome,
            network_config: Versioned {
                value: value.network_config,
                version: value.network_config_version.parse().map_err(|e| {
                    sqlx::error::Error::ColumnDecode {
                        index: "network_config_version".to_string(),
                        source: Box::new(e),
                    }
                })?,
            },
            network_status_observation: value.network_status_observation,
            card_state: value.card_state,
            device_info: value.device_info,
            device_info_ts: value.device_info_ts,
            mlxconfig_profile: value.mlxconfig_profile,
            history: value.history,
            pci_name: value.pci_name,
            underlay_ip: value.underlay_ip,
            overlay_ip: value.overlay_ip,
        })
    }
}

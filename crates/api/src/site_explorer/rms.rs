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

use carbide_uuid::rack::RackId;
use librms::RmsApi;
use librms::protos::rack_manager::{NewNodeInfo, NodeType as RmsNodeType};
use mac_address::MacAddress;

use crate::CarbideError;

const RMS_PORT: i32 = 443;

pub async fn add_node_to_rms(
    rms_client: &dyn RmsApi,
    new_node: NewNodeInfo,
) -> Result<(), CarbideError> {
    let request = librms::protos::rack_manager::AddNodeRequest {
        metadata: None,
        node_info: vec![new_node],
    };
    rms_client
        .add_node(request)
        .await
        .map_err(CarbideError::RackManagerError)?;

    Ok(())
}

pub async fn add_switch_to_rms(
    rms_client: &dyn RmsApi,
    rack_id: RackId,
    node_id: String,
    ip_address: String,
    mac_address: MacAddress,
) -> Result<(), CarbideError> {
    let new_node = NewNodeInfo {
        rack_id: rack_id.to_string(),
        node_id,
        mac_address: mac_address.to_string(),
        ip_address,
        port: RMS_PORT,
        username: None,
        password: None,
        r#type: Some(RmsNodeType::Switch.into()),
        vault_path: format!("switch_nvos/{mac_address}/admin"),
    };
    add_node_to_rms(rms_client, new_node).await
}

pub async fn add_compute_tray_to_rms(
    rms_client: &dyn RmsApi,
    rack_id: RackId,
    node_id: String,
    ip_address: String,
    mac_address: MacAddress,
) -> Result<(), CarbideError> {
    let new_node = NewNodeInfo {
        rack_id: rack_id.to_string(),
        node_id,
        mac_address: mac_address.to_string(),
        ip_address,
        port: RMS_PORT,
        username: None,
        password: None,
        r#type: Some(RmsNodeType::Compute.into()),
        vault_path: String::new(),
    };
    add_node_to_rms(rms_client, new_node).await
}

pub async fn add_power_shelf_to_rms(
    rms_client: &dyn RmsApi,
    rack_id: RackId,
    node_id: String,
    ip_address: String,
    mac_address: MacAddress,
) -> Result<(), CarbideError> {
    let new_node = NewNodeInfo {
        rack_id: rack_id.to_string(),
        node_id,
        mac_address: mac_address.to_string(),
        ip_address,
        port: RMS_PORT,
        username: None,
        password: None,
        r#type: Some(RmsNodeType::Powershelf.into()),
        vault_path: String::new(),
    };
    add_node_to_rms(rms_client, new_node).await
}

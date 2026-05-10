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

use std::collections::HashMap;

use ::rpc::forge as rpc;
use libnmxc::nmxc_model::{
    self, GetComputeNodeInfoListRequest, GetGpuInfoListRequest, GpuAttr, Location,
};
use libnmxc::{Endpoint, NMX_C_GATEWAY_ID, Nmxc, NmxcError};
use serde_json::json;
use tonic::{Request, Response, Status};

use crate::CarbideError;
use crate::api::{Api, log_request_data};

fn nmxc_context() -> nmxc_model::Context {
    nmxc_model::Context {
        context: String::new(),
    }
}

fn map_nmxc_err(e: NmxcError) -> CarbideError {
    match e {
        NmxcError::Status(s) => CarbideError::internal(s.to_string()),
        other => CarbideError::internal(other.to_string()),
    }
}

async fn compute_node_info_list_json(
    nmxc: &dyn Nmxc,
) -> Result<(String, i32, HashMap<String, String>), CarbideError> {
    let resp = nmxc
        .get_compute_node_info_list(GetComputeNodeInfoListRequest {
            context: Some(nmxc_context()),
            loc_list: vec![],
            gateway_id: NMX_C_GATEWAY_ID.to_string(),
        })
        .await
        .map_err(map_nmxc_err)?;

    let domain_uuid = resp
        .server_header
        .as_ref()
        .map(|h| h.domain_uuid.as_str())
        .unwrap_or("")
        .to_string();

    let mut nodes = Vec::with_capacity(resp.node_info_list.len());
    for node in resp.node_info_list {
        let Some(ref li) = node.loc else {
            continue;
        };
        let gpu_id_list: Vec<String> = if let Some(ref loc) = li.location {
            let gresp = nmxc
                .get_gpu_info_list(GetGpuInfoListRequest {
                    context: Some(nmxc_context()),
                    attr: GpuAttr::NmxGpuAttrLocation as i32,
                    num_gpus: 0,
                    loc: Some(Location {
                        chassis_id: loc.chassis_id,
                        slot_id: loc.slot_id,
                        host_id: loc.host_id,
                    }),
                    partition_id: None,
                    gateway_id: NMX_C_GATEWAY_ID.to_string(),
                    gpu_health: 0,
                })
                .await
                .map_err(map_nmxc_err)?;
            gresp
                .gpu_info_list
                .iter()
                .map(|g| g.gpu_uid.to_string())
                .collect()
        } else {
            vec![]
        };

        nodes.push(json!({
            "LocationInfo": {
                "ChassisSerialNumber": li.chassis_serial_number,
                "TrayIndex": li.tray_index as i64,
            },
            "DomainUUID": domain_uuid,
            "GpuIDList": gpu_id_list,
        }));
    }

    let body = serde_json::to_string(&nodes)
        .map_err(|e| CarbideError::internal(format!("serialize compute nodes: {e}")))?;
    Ok((body, 200, HashMap::new()))
}

async fn gpu_info_json(
    nmxc: &dyn Nmxc,
    uid: u64,
) -> Result<(String, i32, HashMap<String, String>), CarbideError> {
    let gresp = nmxc
        .get_gpu_info_list(GetGpuInfoListRequest {
            context: Some(nmxc_context()),
            attr: GpuAttr::NmxGpuAttrAll as i32,
            num_gpus: 0,
            loc: None,
            partition_id: None,
            gateway_id: NMX_C_GATEWAY_ID.to_string(),
            gpu_health: 0,
        })
        .await
        .map_err(map_nmxc_err)?;

    let Some(gpu) = gresp.gpu_info_list.iter().find(|g| g.gpu_uid == uid) else {
        return Err(CarbideError::NotFoundError {
            kind: "nmxc_gpu",
            id: uid.to_string(),
        });
    };

    let (tray_index, slot_id) = gpu
        .loc
        .as_ref()
        .map(|l| {
            let tray = l.tray_index as i64;
            let slot = l
                .location
                .as_ref()
                .map(|loc| loc.slot_id as i64)
                .unwrap_or(0);
            (tray, slot)
        })
        .unwrap_or((0, 0));

    let body = serde_json::to_string(&json!({
        "ID": gpu.gpu_uid.to_string(),
        "DeviceID": gpu.gpu_id as i64,
        "DeviceUID": gpu.gpu_uid,
        "LocationInfo": {
            "TrayIndex": tray_index,
            "SlotID": slot_id,
        },
    }))
    .map_err(|e| CarbideError::internal(format!("serialize gpu: {e}")))?;
    Ok((body, 200, HashMap::new()))
}

async fn gpu_info_list_json(
    nmxc: &dyn Nmxc,
) -> Result<(String, i32, HashMap<String, String>), CarbideError> {
    let gresp = nmxc
        .get_gpu_info_list(GetGpuInfoListRequest {
            context: Some(nmxc_context()),
            attr: GpuAttr::NmxGpuAttrAll as i32,
            num_gpus: 0,
            loc: None,
            partition_id: None,
            gateway_id: NMX_C_GATEWAY_ID.to_string(),
            gpu_health: 0,
        })
        .await
        .map_err(map_nmxc_err)?;

    let domain_uuid = gresp
        .server_header
        .as_ref()
        .map(|h| h.domain_uuid.as_str())
        .unwrap_or("")
        .to_string();

    let mut gpus = Vec::with_capacity(gresp.gpu_info_list.len());
    for gpu in gresp.gpu_info_list {
        let (tray_index, slot_id) = gpu
            .loc
            .as_ref()
            .map(|l| {
                let tray = l.tray_index as i64;
                let slot = l
                    .location
                    .as_ref()
                    .map(|loc| loc.slot_id as i64)
                    .unwrap_or(0);
                (tray, slot)
            })
            .unwrap_or((0, 0));

        gpus.push(json!({
            "DeviceID": gpu.gpu_id as i64,
            "DeviceUID": gpu.gpu_uid,
            "LocationInfo": {
                "TrayIndex": tray_index,
                "SlotID": slot_id,
            },
        }));
    }

    let body = serde_json::to_string(&json!({
        "DomainUUID": domain_uuid,
        "Gpus": gpus,
    }))
    .map_err(|e| CarbideError::internal(format!("serialize gpu list: {e}")))?;
    Ok((body, 200, HashMap::new()))
}

pub(crate) async fn nmxc_browse(
    api: &Api,
    request: Request<rpc::NmxcBrowseRequest>,
) -> Result<Response<rpc::NmxcBrowseResponse>, Status> {
    log_request_data(&request);

    let request = request.into_inner();

    let chassis_serial = request.chassis_serial.trim();
    if chassis_serial.is_empty() {
        return Err(CarbideError::MissingArgument("chassis_serial").into());
    }

    let op = rpc::NmxcBrowseOperation::try_from(request.operation)
        .unwrap_or(rpc::NmxcBrowseOperation::Unspecified);

    if let Some(nvlink_config) = api.runtime_config.nvlink_config.as_ref()
        && nvlink_config.enabled
    {
        let endpoint_row = db::nvlink_nmxc_endpoints::find_by_chassis_serial(
            &api.database_connection,
            chassis_serial,
        )
        .await?;

        let Some(row) = endpoint_row else {
            return Err(CarbideError::NotFoundError {
                kind: "nvlink_nmxc_endpoint",
                id: chassis_serial.to_string(),
            }
            .into());
        };

        let nmxc = api
            .nmxc_client_pool
            .create_client(Endpoint::new(row.endpoint.clone()))
            .await
            .map_err(|e| CarbideError::internal(format!("Failed to connect to NMX-C: {e}")))?;

        let result = match op {
            rpc::NmxcBrowseOperation::Unspecified => Err(CarbideError::InvalidArgument(
                "operation must be set to a supported NmxcBrowseOperation".to_string(),
            )),
            rpc::NmxcBrowseOperation::ComputeNodeInfoList => {
                compute_node_info_list_json(nmxc.as_ref()).await
            }
            rpc::NmxcBrowseOperation::GpuInfo => {
                if request.gpu_uid == 0 {
                    Err(CarbideError::InvalidArgument(
                        "gpu_uid is required for GPU_INFO operation".to_string(),
                    ))
                } else {
                    gpu_info_json(nmxc.as_ref(), request.gpu_uid).await
                }
            }
            rpc::NmxcBrowseOperation::GpuInfoList => gpu_info_list_json(nmxc.as_ref()).await,
        };

        match result {
            Ok((body, code, headers)) => Ok(Response::new(rpc::NmxcBrowseResponse {
                body,
                code,
                headers,
            })),
            Err(CarbideError::NotFoundError { kind, id }) if kind == "nmxc_gpu" => {
                Ok(Response::new(rpc::NmxcBrowseResponse {
                    body: format!("GPU not found: {id}"),
                    code: 404,
                    headers: HashMap::new(),
                }))
            }
            Err(CarbideError::InvalidArgument(msg)) => Ok(Response::new(rpc::NmxcBrowseResponse {
                body: msg,
                code: 400,
                headers: HashMap::new(),
            })),
            Err(e) => Err(e.into()),
        }
    } else {
        Err(CarbideError::internal("nvlink config not enabled".to_string()).into())
    }
}

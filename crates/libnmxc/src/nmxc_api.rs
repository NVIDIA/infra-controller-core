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
use tonic::transport::Channel;

use crate::nmxc_model::nmx_controller_client::NmxControllerClient;
use crate::{Nmxc, NmxcError, nmxc_model};

fn default_context() -> nmxc_model::Context {
    nmxc_model::Context {
        context: String::new(),
    }
}

#[derive(Clone)]
pub struct NmxcApi {
    client: NmxControllerClient<Channel>,
}

impl NmxcApi {
    pub fn new(client: NmxControllerClient<Channel>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl Nmxc for NmxcApi {
    async fn hello(&self, gateway_id: &str) -> Result<nmxc_model::ServerHello, NmxcError> {
        let req = nmxc_model::ClientHello {
            gateway_id: gateway_id.to_string(),
            major_version: nmxc_model::ProtoMsgMajorVersion::ProtoMsgMajorVersion as i32,
            minor_version: nmxc_model::ProtoMsgMinorVersion::ProtoMsgMinorVersion as i32,
        };
        let res = self.client.clone().hello(tonic::Request::new(req)).await?;
        Ok(res.into_inner())
    }

    async fn get_domain_properties(
        &self,
        context: Option<nmxc_model::Context>,
        gateway_id: &str,
    ) -> Result<nmxc_model::DomainProperties, NmxcError> {
        let req = nmxc_model::GetDomainPropertiesRequest {
            context: Some(context.unwrap_or_else(default_context)),
            gateway_id: gateway_id.to_string(),
        };
        let res = self
            .client
            .clone()
            .get_domain_properties(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_domain_state_info(
        &self,
        context: Option<nmxc_model::Context>,
        gateway_id: &str,
    ) -> Result<nmxc_model::DomainStateInfo, NmxcError> {
        let req = nmxc_model::GetDomainStateInfoRequest {
            context: Some(context.unwrap_or_else(default_context)),
            gateway_id: gateway_id.to_string(),
        };
        let res = self
            .client
            .clone()
            .get_domain_state_info(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_topology_info(
        &self,
        context: Option<nmxc_model::Context>,
        gateway_id: &str,
    ) -> Result<nmxc_model::FmTopologyInfo, NmxcError> {
        let req = nmxc_model::GetTopologyInfoRequest {
            context: Some(context.unwrap_or_else(default_context)),
            gateway_id: gateway_id.to_string(),
        };
        let res = self
            .client
            .clone()
            .get_topology_info(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_compute_node_count(
        &self,
        req: nmxc_model::GetComputeNodeCountRequest,
    ) -> Result<nmxc_model::GetComputeNodeCountResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .get_compute_node_count(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_compute_node_info_list(
        &self,
        req: nmxc_model::GetComputeNodeInfoListRequest,
    ) -> Result<nmxc_model::GetComputeNodeInfoListResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .get_compute_node_info_list(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_gpu_info_list(
        &self,
        req: nmxc_model::GetGpuInfoListRequest,
    ) -> Result<nmxc_model::GetGpuInfoListResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .get_gpu_info_list(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_switch_node_count(
        &self,
        req: nmxc_model::GetSwitchNodeCountRequest,
    ) -> Result<nmxc_model::GetSwitchNodeCountResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .get_switch_node_count(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_switch_node_info_list(
        &self,
        req: nmxc_model::GetSwitchNodeInfoListRequest,
    ) -> Result<nmxc_model::GetSwitchNodeInfoListResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .get_switch_node_info_list(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_partition_count(
        &self,
        req: nmxc_model::GetPartitionCountRequest,
    ) -> Result<nmxc_model::GetPartitionCountResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .get_partition_count(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_partition_id_list(
        &self,
        req: nmxc_model::GetPartitionIdListRequest,
    ) -> Result<nmxc_model::GetPartitionIdListResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .get_partition_id_list(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn get_partition_info_list(
        &self,
        req: nmxc_model::GetPartitionInfoListRequest,
    ) -> Result<nmxc_model::GetPartitionInfoListResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .get_partition_info_list(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn create_partition(
        &self,
        req: nmxc_model::CreatePartitionRequest,
    ) -> Result<nmxc_model::CreatePartitionResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .create_partition(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn delete_partition(
        &self,
        req: nmxc_model::DeletePartitionRequest,
    ) -> Result<nmxc_model::DeletePartitionResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .delete_partition(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn add_gpus_to_partition(
        &self,
        req: nmxc_model::UpdatePartitionRequest,
    ) -> Result<nmxc_model::UpdatePartitionResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .add_gpus_to_partition(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }

    async fn remove_gpus_from_partition(
        &self,
        req: nmxc_model::UpdatePartitionRequest,
    ) -> Result<nmxc_model::UpdatePartitionResponse, NmxcError> {
        let res = self
            .client
            .clone()
            .remove_gpus_from_partition(tonic::Request::new(req))
            .await?;
        Ok(res.into_inner())
    }
}

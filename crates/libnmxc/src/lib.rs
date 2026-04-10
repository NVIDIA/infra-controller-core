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
mod nmxc_api;

// Generated gRPC types and client from nmx_c.proto
pub mod nmxc_model {
    #![allow(clippy::all, non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/nmx_c.rs"));
}

use std::time::Duration;

use tonic::transport::Channel;
use tracing::debug;

use crate::nmxc_api::NmxcApi;
use crate::nmxc_model::nmx_controller_client::NmxControllerClient;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// `gateway_id` sent on NMX-C gRPC requests from Carbide and the `nmxc` test client.
pub const NMX_C_GATEWAY_ID: &str = "carbide";

#[derive(thiserror::Error, Debug)]
pub enum NmxcError {
    #[error("Invalid endpoint URL: {0}")]
    InvalidEndpoint(String),

    #[error("Transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("gRPC status: {0}")]
    Status(#[from] tonic::Status),

    #[error("Connection not initialized")]
    Uninitialized,
}

impl NmxcError {
    /// Creates an error for invalid or missing response from the server.
    pub fn invalid_response(msg: impl Into<String>) -> Self {
        NmxcError::Status(tonic::Status::unknown(msg.into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Base URL for the NMX-C gRPC service (e.g. "https://host:50051" or "http://localhost:50051")
    pub url: String,
}

impl Endpoint {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

#[derive(Clone, Debug)]
pub struct NmxcClientPoolBuilder {
    pub timeout: Duration,
}

impl NmxcClientPoolBuilder {
    pub fn build(&self) -> Result<NmxcClientPool, NmxcError> {
        Ok(NmxcClientPool {
            timeout: self.timeout,
        })
    }
}

#[derive(Clone, Debug)]
pub struct NmxcClientPool {
    timeout: Duration,
}

impl NmxcClientPool {
    pub fn builder() -> NmxcClientPoolBuilder {
        NmxcClientPoolBuilder {
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub async fn create_client(&self, endpoint: Endpoint) -> Result<Box<dyn Nmxc>, NmxcError> {
        let channel = self.connect(&endpoint).await?;
        let client = NmxControllerClient::new(channel).max_decoding_message_size(usize::MAX);
        let nmxc = NmxcApi::new(client);
        Ok(Box::new(nmxc))
    }

    async fn connect(&self, endpoint: &Endpoint) -> Result<Channel, NmxcError> {
        let uri: tonic::transport::Uri = endpoint
            .url
            .parse()
            .map_err(|e| NmxcError::InvalidEndpoint(format!("{}: {}", endpoint.url, e)))?;

        let scheme = uri.scheme_str().unwrap_or("http");
        let channel = if scheme.eq_ignore_ascii_case("https") {
            // Note: tonic's ClientTlsConfig does not support accepting invalid certs.
            // For self-signed or invalid certs, use http:// (plain) or ensure the server
            // presents a cert trusted by the system.
            let endpoint_builder = tonic::transport::Endpoint::from_shared(endpoint.url.clone())
                .map_err(|e| NmxcError::InvalidEndpoint(e.to_string()))?
                .connect_timeout(self.timeout);

            endpoint_builder
                .tls_config(tonic::transport::ClientTlsConfig::new())
                .map_err(|e| NmxcError::InvalidEndpoint(e.to_string()))?
                .connect()
                .await?
        } else {
            tonic::transport::Channel::from_shared(endpoint.url.clone())
                .map_err(|e| NmxcError::InvalidEndpoint(e.to_string()))?
                .connect_timeout(self.timeout)
                .connect()
                .await?
        };

        debug!("Connected to NMX-C at {}", endpoint.url);
        println!("Connected to NMX-C at {}", endpoint.url);
        Ok(channel)
    }
}

/// Abstraction over [`NmxcClientPool`] and test doubles (e.g. `NmxcSimClient` in carbide-api).
#[async_trait::async_trait]
pub trait NmxcPool: Send + Sync + 'static {
    async fn create_client(&self, endpoint: Endpoint) -> Result<Box<dyn Nmxc>, NmxcError>;
}

#[async_trait::async_trait]
impl NmxcPool for NmxcClientPool {
    async fn create_client(&self, endpoint: Endpoint) -> Result<Box<dyn Nmxc>, NmxcError> {
        NmxcClientPool::create_client(self, endpoint).await
    }
}

#[async_trait::async_trait]
pub trait Nmxc: Send + Sync + 'static {
    /// Perform Hello handshake with the NMX-C controller.
    async fn hello(&self, gateway_id: &str) -> Result<nmxc_model::ServerHello, NmxcError>;

    async fn get_domain_properties(
        &self,
        context: Option<nmxc_model::Context>,
        gateway_id: &str,
    ) -> Result<nmxc_model::DomainProperties, NmxcError>;

    async fn get_domain_state_info(
        &self,
        context: Option<nmxc_model::Context>,
        gateway_id: &str,
    ) -> Result<nmxc_model::DomainStateInfo, NmxcError>;

    async fn get_topology_info(
        &self,
        context: Option<nmxc_model::Context>,
        gateway_id: &str,
    ) -> Result<nmxc_model::FmTopologyInfo, NmxcError>;

    async fn get_compute_node_count(
        &self,
        req: nmxc_model::GetComputeNodeCountRequest,
    ) -> Result<nmxc_model::GetComputeNodeCountResponse, NmxcError>;

    async fn get_compute_node_info_list(
        &self,
        req: nmxc_model::GetComputeNodeInfoListRequest,
    ) -> Result<nmxc_model::GetComputeNodeInfoListResponse, NmxcError>;

    async fn get_gpu_info_list(
        &self,
        req: nmxc_model::GetGpuInfoListRequest,
    ) -> Result<nmxc_model::GetGpuInfoListResponse, NmxcError>;

    async fn get_switch_node_count(
        &self,
        req: nmxc_model::GetSwitchNodeCountRequest,
    ) -> Result<nmxc_model::GetSwitchNodeCountResponse, NmxcError>;

    async fn get_switch_node_info_list(
        &self,
        req: nmxc_model::GetSwitchNodeInfoListRequest,
    ) -> Result<nmxc_model::GetSwitchNodeInfoListResponse, NmxcError>;

    async fn get_partition_count(
        &self,
        req: nmxc_model::GetPartitionCountRequest,
    ) -> Result<nmxc_model::GetPartitionCountResponse, NmxcError>;

    async fn get_partition_id_list(
        &self,
        req: nmxc_model::GetPartitionIdListRequest,
    ) -> Result<nmxc_model::GetPartitionIdListResponse, NmxcError>;

    async fn get_partition_info_list(
        &self,
        req: nmxc_model::GetPartitionInfoListRequest,
    ) -> Result<nmxc_model::GetPartitionInfoListResponse, NmxcError>;

    async fn create_partition(
        &self,
        req: nmxc_model::CreatePartitionRequest,
    ) -> Result<nmxc_model::CreatePartitionResponse, NmxcError>;

    async fn delete_partition(
        &self,
        req: nmxc_model::DeletePartitionRequest,
    ) -> Result<nmxc_model::DeletePartitionResponse, NmxcError>;

    async fn add_gpus_to_partition(
        &self,
        req: nmxc_model::UpdatePartitionRequest,
    ) -> Result<nmxc_model::UpdatePartitionResponse, NmxcError>;

    async fn remove_gpus_from_partition(
        &self,
        req: nmxc_model::UpdatePartitionRequest,
    ) -> Result<nmxc_model::UpdatePartitionResponse, NmxcError>;
}

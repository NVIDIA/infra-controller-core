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

use async_trait::async_trait;
use db::DatabaseError;
use forge_secrets::credentials::{CredentialKey, CredentialReader, Credentials};
use libnmxm::{Nmxm, NmxmApiError};

use crate::handlers::credential::DEFAULT_NMX_M_NAME;

#[allow(dead_code)]
#[derive(thiserror::Error, Debug)]
pub enum NvLinkPartitionError {
    #[error("Failed to look up credentials {0}")]
    MissingCredentials(eyre::Report),
    #[error("Failed NMX-M api request {0}")]
    NmxmApiError(NmxmApiError),
    #[error("Database error {0}")]
    DbError(DatabaseError),
    #[error("{0}: {1} in use / busy")]
    ObjectInUse(String, String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid arguments")]
    InvalidArguments,
    #[error("Invalid API response")]
    InvalidApiResponse,
}

#[async_trait]
pub trait NmxmClientPool: Send + Sync + 'static {
    async fn create_client(
        &self,
        endpoint: &str,
        nmxm_id: Option<String>,
    ) -> Result<Box<dyn Nmxm>, NvLinkPartitionError>;
}

#[derive(Debug)]
pub struct NmxmClientPoolImpl<C> {
    pool: libnmxm::NmxmClientPool,
    credential_reader: C,
}

impl<C: CredentialReader> NmxmClientPoolImpl<C> {
    pub fn new(credential_reader: C, pool: libnmxm::NmxmClientPool) -> Self {
        NmxmClientPoolImpl {
            credential_reader,
            pool,
        }
    }
}

#[async_trait]
impl<C: CredentialReader + 'static> NmxmClientPool for NmxmClientPoolImpl<C> {
    async fn create_client(
        &self,
        endpoint: &str,
        nmxm_id: Option<String>,
    ) -> Result<Box<dyn Nmxm>, NvLinkPartitionError> {
        let id = nmxm_id.unwrap_or(DEFAULT_NMX_M_NAME.to_string());
        let credentials = self
            .credential_reader
            .get_credentials(&CredentialKey::NmxM { nmxm_id: id })
            .await
            .map_err(|e| NvLinkPartitionError::MissingCredentials(eyre::Report::from(e)))?
            .ok_or(NvLinkPartitionError::MissingCredentials(eyre::Report::msg(
                "NMX-M credentials not found",
            )))?;
        let (user, pass) = match credentials {
            Credentials::UsernamePassword { username, password } => (username, password),
        };
        if endpoint.parse::<http::Uri>().is_err() {
            return Err(NvLinkPartitionError::InvalidArguments);
        };
        let endpoint = libnmxm::Endpoint {
            host: endpoint.to_string(),
            username: Some(user),
            password: Some(pass),
        };

        self.pool
            .create_client(endpoint)
            .await
            .map_err(NvLinkPartitionError::NmxmApiError)
    }
}

#[cfg(test)]
pub mod test_support {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    use libnmxc::nmxc_model::{
        self, GetComputeNodeCountResponse, GetComputeNodeInfoListResponse, GetGpuInfoListResponse,
        GetPartitionCountResponse, GetPartitionIdListResponse, GetPartitionInfoListResponse,
        GetSwitchNodeCountResponse, GetSwitchNodeInfoListResponse,
    };
    use libnmxc::{Endpoint, Nmxc, NmxcClientPool, NmxcError, NmxcPool};
    use uuid::Uuid;

    use super::*;

    // mock similar to RedfishSim
    #[derive(Debug)]
    pub struct NmxmSimClient {
        _state: Arc<Mutex<u32>>,
        _partitions: Arc<Mutex<Vec<libnmxm::nmxm_model::Partition>>>,
        _gpus: Arc<Mutex<Vec<libnmxm::nmxm_model::Gpu>>>,
        _fail_after_n_creates: Option<Arc<Mutex<usize>>>,
    }

    impl Default for NmxmSimClient {
        fn default() -> Self {
            NmxmSimClient {
                _state: Arc::new(Mutex::new(0)),
                _partitions: Arc::new(Mutex::new(Vec::new())),
                _gpus: Arc::new(Mutex::new(Self::default_gpus())),
                _fail_after_n_creates: None,
            }
        }
    }

    impl NmxmSimClient {
        // After n create_requests succeed, they will start failing.
        pub fn with_fail_after_n_creates(n: usize) -> Self {
            NmxmSimClient {
                _fail_after_n_creates: Some(Arc::new(Mutex::new(n))),
                ..Self::default()
            }
        }

        pub fn with_unknown_partition() -> Self {
            let client = Self::default();
            client.create_partition(
                12345,
                client
                    ._gpus
                    .lock()
                    .unwrap()
                    .iter()
                    .filter_map(|gpu| gpu.id.clone())
                    .collect(),
            );
            client
        }

        pub fn with_default_partition() -> Self {
            let client = Self::default();
            client.create_partition(
                32766, // default partition id.
                client
                    ._gpus
                    .lock()
                    .unwrap()
                    .iter()
                    .filter_map(|gpu| gpu.id.clone())
                    .collect(),
            );
            client
        }

        /// Creates a partition with given partition_id containing the specified GPU IDs.
        fn create_partition(&self, partition_id: i32, gpu_ids: Vec<String>) {
            let partition = libnmxm::nmxm_model::Partition {
                id: "default-partition".to_string(),
                partition_id,
                name: "Default Partition".to_string(),
                r#type: libnmxm::nmxm_model::PartitionType::PartitionTypeIDBased,
                health: libnmxm::nmxm_model::PartitionHealth::PartitionHealthHealthy,
                members: Box::new(libnmxm::nmxm_model::PartitionMembers::Ids(gpu_ids)),
                created_at: "2021-01-01T12:00:00Z".to_string(),
                updated_at: "2021-01-01T12:00:00Z".to_string(),
            };
            self._partitions.lock().unwrap().push(partition);
        }

        fn default_gpus() -> Vec<libnmxm::nmxm_model::Gpu> {
            let all_ones_uuid = Uuid::from_bytes([
                0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF,
            ]);
            let all_ones_minus_one_uuid = Uuid::from_bytes([
                0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF,
            ]);

            let location1 = libnmxm::nmxm_model::LocationInfo {
                chassis_id: Some(101),
                chassis_serial_number: Some(String::from("SN_WHATISTHIS")),
                slot_id: Some(0),
                tray_index: Some(0),
                host_id: Some(1),
            };

            let location2 = libnmxm::nmxm_model::LocationInfo {
                chassis_id: Some(101),
                chassis_serial_number: Some(String::from("SN_WHATISTHIS1")),
                slot_id: Some(0),
                tray_index: Some(0),
                host_id: Some(1),
            };

            let location3 = libnmxm::nmxm_model::LocationInfo {
                chassis_id: Some(101),
                chassis_serial_number: Some(String::from("SN_WHATISTHIS2")),
                slot_id: Some(0),
                tray_index: Some(0),
                host_id: Some(1),
            };

            let location4 = libnmxm::nmxm_model::LocationInfo {
                chassis_id: Some(101),
                chassis_serial_number: Some(String::from("SN_WHATISTHIS1")),
                slot_id: Some(1),
                tray_index: Some(1),
                host_id: Some(1),
            };

            vec![
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu1")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 1")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location1.clone())),
                    device_uid: 12345,
                    device_id: 1,
                    device_pcie_id: 1001,
                    system_uid: 10001,
                    vendor_id: 4318,
                    alid_list: vec![1111, 2222],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu2")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 2")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_minus_one_uuid),
                    location_info: Some(Box::new(location1.clone())),
                    device_uid: 12346,
                    device_id: 2,
                    device_pcie_id: 1002,
                    system_uid: 10002,
                    vendor_id: 4318,
                    alid_list: vec![3333, 4444],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu3")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 3")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location1.clone())),
                    device_uid: 12347,
                    device_id: 3,
                    device_pcie_id: 1003,
                    system_uid: 10003,
                    vendor_id: 4318,
                    alid_list: vec![5555, 6666],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu4")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 4")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location1)),
                    device_uid: 12348,
                    device_id: 4,
                    device_pcie_id: 1004,
                    system_uid: 10004,
                    vendor_id: 4318,
                    alid_list: vec![7777, 8888],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu11")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 11")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location2.clone())),
                    device_uid: 12345,
                    device_id: 1,
                    device_pcie_id: 1001,
                    system_uid: 10001,
                    vendor_id: 4318,
                    alid_list: vec![1111, 2222],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu12")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 12")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_minus_one_uuid),
                    location_info: Some(Box::new(location2.clone())),
                    device_uid: 12346,
                    device_id: 2,
                    device_pcie_id: 1002,
                    system_uid: 10002,
                    vendor_id: 4318,
                    alid_list: vec![3333, 4444],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu13")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 13")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location2.clone())),
                    device_uid: 12347,
                    device_id: 3,
                    device_pcie_id: 1003,
                    system_uid: 10003,
                    vendor_id: 4318,
                    alid_list: vec![5555, 6666],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu14")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 14")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location2)),
                    device_uid: 12348,
                    device_id: 4,
                    device_pcie_id: 1004,
                    system_uid: 10004,
                    vendor_id: 4318,
                    alid_list: vec![7777, 8888],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu21")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 21")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_minus_one_uuid),
                    location_info: Some(Box::new(location3.clone())),
                    device_uid: 12349,
                    device_id: 1,
                    device_pcie_id: 1005,
                    system_uid: 10005,
                    vendor_id: 4318,
                    alid_list: vec![9999, 9888],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu22")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 22")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_minus_one_uuid),
                    location_info: Some(Box::new(location3.clone())),
                    device_uid: 12350,
                    device_id: 2,
                    device_pcie_id: 1006,
                    system_uid: 10006,
                    vendor_id: 4318,
                    alid_list: vec![1212, 2121],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu23")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 23")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_minus_one_uuid),
                    location_info: Some(Box::new(location3.clone())),
                    device_uid: 12351,
                    device_id: 3,
                    device_pcie_id: 1007,
                    system_uid: 10007,
                    vendor_id: 4318,
                    alid_list: vec![1313, 1414],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu24")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 24")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_minus_one_uuid),
                    location_info: Some(Box::new(location3)),
                    device_uid: 12352,
                    device_id: 4,
                    device_pcie_id: 1008,
                    system_uid: 10008,
                    vendor_id: 4318,
                    alid_list: vec![1515, 1616],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu31")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 31")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location4.clone())),
                    device_uid: 12353,
                    device_id: 1,
                    device_pcie_id: 1009,
                    system_uid: 10009,
                    vendor_id: 4318,
                    alid_list: vec![1717, 1818],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu32")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 32")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location4.clone())),
                    device_uid: 12354,
                    device_id: 2,
                    device_pcie_id: 1010,
                    system_uid: 10010,
                    vendor_id: 4318,
                    alid_list: vec![1919, 2020],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu33")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 33")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location4.clone())),
                    device_uid: 12355,
                    device_id: 3,
                    device_pcie_id: 1011,
                    system_uid: 10011,
                    vendor_id: 4318,
                    alid_list: vec![2121, 2222],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
                libnmxm::nmxm_model::Gpu {
                    id: Some(String::from("gpu34")),
                    name: Some(String::from("NVIDIA GB200 NVL")),
                    description: Some(String::from("High-end gaming GPU")),
                    internal_description: Some(String::from("Internal description for GPU 34")),
                    created_at: Some(String::from("2021-01-01T12:00:00Z")),
                    updated_at: Some(String::from("2021-06-01T12:00:00Z")),
                    domain_uuid: Some(all_ones_uuid),
                    location_info: Some(Box::new(location4)),
                    device_uid: 12356,
                    device_id: 4,
                    device_pcie_id: 1012,
                    system_uid: 10012,
                    vendor_id: 4318,
                    alid_list: vec![2323, 2424],
                    partition_id: None,
                    port_id_list: None,
                    health: None,
                },
            ]
        }
    }

    #[async_trait]
    impl Nmxm for NmxmSimClient {
        async fn create(
            &self,
            _endpoint: libnmxm::Endpoint,
        ) -> Result<Box<dyn Nmxm>, NmxmApiError> {
            todo!()
        }

        async fn raw_get(
            &self,
            _api: &str,
        ) -> Result<libnmxm::nmxm_model::RawResponse, NmxmApiError> {
            todo!()
        }

        async fn get_chassis(
            &self,
            _id: String,
        ) -> Result<Vec<libnmxm::nmxm_model::Chassis>, NmxmApiError> {
            todo!()
        }

        async fn get_chassis_count(
            &self,
            _domain: Option<Vec<uuid::Uuid>>,
        ) -> Result<i64, NmxmApiError> {
            todo!()
        }

        async fn get_gpu(
            &self,
            _id: Option<String>,
        ) -> Result<Vec<libnmxm::nmxm_model::Gpu>, NmxmApiError> {
            Ok(self._gpus.lock().unwrap().clone())
        }

        async fn get_gpu_count(
            &self,
            _domain: Option<Vec<uuid::Uuid>>,
        ) -> Result<i64, NmxmApiError> {
            todo!()
        }

        async fn get_partition(
            &self,
            _id: String,
        ) -> Result<libnmxm::nmxm_model::Partition, NmxmApiError> {
            todo!()
        }

        async fn get_partitions_list(
            &self,
        ) -> Result<Vec<libnmxm::nmxm_model::Partition>, NmxmApiError> {
            let mut _state = self._state.lock().unwrap();
            let mut _p = self._partitions.lock().unwrap();
            let partitions = _p.clone();

            Ok(partitions)
        }

        async fn get_compute_node(
            &self,
            _id: Option<String>,
        ) -> Result<Vec<libnmxm::nmxm_model::ComputeNode>, NmxmApiError> {
            todo!()
        }

        async fn get_compute_nodes_count(
            &self,
            _domain: Option<Vec<uuid::Uuid>>,
        ) -> Result<i64, NmxmApiError> {
            todo!()
        }

        async fn get_port(
            &self,
            _id: Option<String>,
        ) -> Result<Vec<libnmxm::nmxm_model::Port>, NmxmApiError> {
            todo!()
        }

        async fn get_ports_count(
            &self,
            _domain: Option<Vec<uuid::Uuid>>,
        ) -> Result<i64, NmxmApiError> {
            todo!()
        }

        async fn get_switch_node(
            &self,
            _id: Option<String>,
        ) -> Result<Vec<libnmxm::nmxm_model::SwitchNode>, NmxmApiError> {
            todo!()
        }

        async fn get_switch_nodes_count(
            &self,
            _domain: Option<Vec<uuid::Uuid>>,
        ) -> Result<i64, NmxmApiError> {
            todo!()
        }

        async fn create_partition(
            &self,
            _req: Option<libnmxm::nmxm_model::CreatePartitionRequest>,
        ) -> Result<libnmxm::nmxm_model::AsyncResponse, NmxmApiError> {
            {
                if let Some(fail_counter) = &self._fail_after_n_creates {
                    let mut fail_counter = fail_counter.lock().unwrap();
                    if *fail_counter == 0 {
                        return Err(NmxmApiError::InvalidArguments);
                    }
                    *fail_counter -= 1;
                }
            }
            let r = _req.unwrap();
            let mut _p = self._partitions.lock().unwrap();
            let partition = libnmxm::nmxm_model::Partition {
                id: uuid::Uuid::new_v4().into(),
                partition_id: 1,
                name: r.name,
                r#type: libnmxm::nmxm_model::PartitionType::PartitionTypeIDBased,
                health: libnmxm::nmxm_model::PartitionHealth::PartitionHealthHealthy,
                members: r.members,
                created_at: String::from("2023-03-01T12:00:00.000Z"),
                updated_at: String::from("2023-03-01T12:00:00.000Z"),
            };

            _p.push(partition);

            Ok(libnmxm::nmxm_model::AsyncResponse {
                operation_id: "5151515151".to_string(),
            })
        }

        async fn delete_partition(
            &self,
            _id: String,
        ) -> Result<libnmxm::nmxm_model::AsyncResponse, NmxmApiError> {
            let mut _p = self._partitions.lock().unwrap();
            _p.retain(|partition| partition.id != _id);
            Ok(libnmxm::nmxm_model::AsyncResponse {
                operation_id: "5151515151".to_string(),
            })
        }

        async fn update_partition(
            &self,
            _id: String,
            _req: libnmxm::nmxm_model::UpdatePartitionRequest,
        ) -> Result<libnmxm::nmxm_model::AsyncResponse, NmxmApiError> {
            let mut _p = self._partitions.lock().unwrap();
            if let Some(partition) = _p.iter_mut().find(|p| p.id == _id) {
                partition.members = _req.members;
            }
            Ok(libnmxm::nmxm_model::AsyncResponse {
                operation_id: "5151515151".to_string(),
            })
        }

        async fn get_operation(
            &self,
            _id: String,
        ) -> Result<libnmxm::nmxm_model::Operation, NmxmApiError> {
            let operation_request = libnmxm::nmxm_model::OperationRequest {
                method: libnmxm::nmxm_model::OperationRequestMethod::Post,
                uri: String::from("/nmx/v1/create_partition"),
                body: Some(Some(serde_json::json!({"key": "value"}))),
                cancellable: true,
            };
            let operation = libnmxm::nmxm_model::Operation {
                id: String::from("5151515151"),
                created_at: String::from("2025-10-10T10:00:00Z"),
                updated_at: String::from("2025-10-10T11:00:00Z"),
                status: libnmxm::nmxm_model::OperationStatus::Completed,
                percentage: 75.0,
                current_step: String::from("Done"),
                request: Box::new(operation_request),
                result: None,
            };

            Ok(operation)
        }

        async fn get_operations_list(
            &self,
        ) -> Result<Vec<libnmxm::nmxm_model::Operation>, NmxmApiError> {
            todo!()
        }

        async fn cancel_operation(
            &self,
            _id: String,
        ) -> Result<libnmxm::nmxm_model::AsyncResponse, NmxmApiError> {
            todo!()
        }
    }

    #[async_trait]
    impl NmxmClientPool for NmxmSimClient {
        async fn create_client(
            &self,
            _endpoint: &str,
            _nmxm_id: Option<String>,
        ) -> Result<Box<dyn Nmxm>, NvLinkPartitionError> {
            Ok(Box::new(NmxmSimClient {
                _state: self._state.clone(),
                _partitions: self._partitions.clone(),
                _gpus: self._gpus.clone(),
                _fail_after_n_creates: self._fail_after_n_creates.clone(),
            }))
        }
    }

    #[derive(Debug)]
    struct SimPartition {
        partition_id: u32,
        name: String,
        gpu_uids: Vec<u64>,
    }

    /// In-memory NMX-C gRPC API mock, mirroring [`NmxmSimClient`] partition presets for tests.
    #[derive(Debug)]
    pub struct NmxcSimClient {
        _partitions: Arc<Mutex<Vec<SimPartition>>>,
        _next_partition_id: Arc<Mutex<u32>>,
        _fail_after_n_creates: Option<Arc<Mutex<usize>>>,
    }

    impl Default for NmxcSimClient {
        fn default() -> Self {
            NmxcSimClient {
                _partitions: Arc::new(Mutex::new(Vec::new())),
                _next_partition_id: Arc::new(Mutex::new(1)),
                _fail_after_n_creates: None,
            }
        }
    }

    impl NmxcSimClient {
        /// After n successful [`Nmxc::create_partition`] calls, further creates fail.
        pub fn with_fail_after_n_creates(n: usize) -> Self {
            NmxcSimClient {
                _fail_after_n_creates: Some(Arc::new(Mutex::new(n))),
                ..Self::default()
            }
        }

        pub fn with_unknown_partition() -> Self {
            let client = Self::default();
            client.push_partition(12345, "unknown-partition", Self::default_gpu_uids());
            client
        }

        pub fn with_default_partition() -> Self {
            let client = Self::default();
            client.push_partition(32766, "default-partition", Self::default_gpu_uids());
            client
        }

        fn default_gpu_uids() -> Vec<u64> {
            vec![
                12345, 12346, 12347, 12348, 12349, 12350, 12351, 12352, 12353, 12354, 12355, 12356,
            ]
        }

        fn push_partition(&self, partition_id: u32, name: impl Into<String>, gpu_uids: Vec<u64>) {
            self._partitions.lock().unwrap().push(SimPartition {
                partition_id,
                name: name.into(),
                gpu_uids,
            });
        }

        fn to_partition_info(p: &SimPartition) -> nmxc_model::PartitionInfo {
            nmxc_model::PartitionInfo {
                partition_id: Some(nmxc_model::PartitionId {
                    partition_id: p.partition_id,
                }),
                name: p.name.clone(),
                num_gpus: p.gpu_uids.len() as u32,
                gpu_location_list: vec![],
                gpu_uid_list: p.gpu_uids.clone(),
                health: nmxc_model::PartitionHealth::NmxPartitionHealthHealthy as i32,
                partition_type: nmxc_model::PartitionType::NmxPartitionTypeGpuuidBased as i32,
                num_allocated_multicast_groups: 0,
                attr: None,
            }
        }

        fn uids_from_resource_ids(ids: &[nmxc_model::GpuResourceId]) -> Vec<u64> {
            ids.iter()
                .filter_map(|r| match &r.resource_id {
                    Some(nmxc_model::gpu_resource_id::ResourceId::GpuUid(uid)) => Some(*uid),
                    _ => None,
                })
                .collect()
        }
    }

    #[async_trait]
    impl Nmxc for NmxcSimClient {
        async fn hello(&self, _gateway_id: &str) -> Result<nmxc_model::ServerHello, NmxcError> {
            Ok(nmxc_model::ServerHello {
                server_header: None,
                components_ver: vec![],
                capabilities: vec![],
                host_os_details: String::new(),
                major_version: nmxc_model::ProtoMsgMajorVersion::ProtoMsgMajorVersion as i32,
                minor_version: nmxc_model::ProtoMsgMinorVersion::ProtoMsgMinorVersion as i32,
            })
        }

        async fn get_domain_properties(
            &self,
            _context: Option<nmxc_model::Context>,
            _gateway_id: &str,
        ) -> Result<nmxc_model::DomainProperties, NmxcError> {
            Ok(nmxc_model::DomainProperties {
                server_header: None,
                context: None,
                max_compute_nodes: 0,
                max_compute_nodes_per_chassis: 0,
                max_gpus_per_compute_node: 0,
                max_gpu_nv_links: 0,
                line_rate_mbps: 0,
                max_switch_nodes: 0,
                max_switch_nodes_per_chassis: 0,
                max_switches_per_switch_node: 0,
                max_switch_nv_links: 0,
                min_gpus_per_partition: 0,
                max_num_partitions: 0,
                max_num_alids: 0,
                max_multicast_groups: 0,
                max_num_ports: 0,
            })
        }

        async fn get_domain_state_info(
            &self,
            _context: Option<nmxc_model::Context>,
            _gateway_id: &str,
        ) -> Result<nmxc_model::DomainStateInfo, NmxcError> {
            Ok(nmxc_model::DomainStateInfo {
                server_header: None,
                context: None,
                control_plane_state: 0,
                available_multicast_groups: 0,
                config_status_description: String::new(),
                nmx_controller_health: 0,
            })
        }

        async fn get_topology_info(
            &self,
            _context: Option<nmxc_model::Context>,
            _gateway_id: &str,
        ) -> Result<nmxc_model::FmTopologyInfo, NmxcError> {
            Ok(nmxc_model::FmTopologyInfo {
                server_header: None,
                context: None,
                device_topo_info: vec![],
            })
        }

        async fn get_compute_node_count(
            &self,
            _req: nmxc_model::GetComputeNodeCountRequest,
        ) -> Result<GetComputeNodeCountResponse, NmxcError> {
            Ok(GetComputeNodeCountResponse {
                server_header: None,
                context: None,
                num_nodes: 0,
            })
        }

        async fn get_compute_node_info_list(
            &self,
            _req: nmxc_model::GetComputeNodeInfoListRequest,
        ) -> Result<GetComputeNodeInfoListResponse, NmxcError> {
            Ok(GetComputeNodeInfoListResponse {
                server_header: None,
                context: None,
                node_info_list: vec![],
            })
        }

        async fn get_gpu_info_list(
            &self,
            _req: nmxc_model::GetGpuInfoListRequest,
        ) -> Result<GetGpuInfoListResponse, NmxcError> {
            Ok(GetGpuInfoListResponse {
                server_header: None,
                context: None,
                gpu_info_list: vec![],
            })
        }

        async fn get_switch_node_count(
            &self,
            _req: nmxc_model::GetSwitchNodeCountRequest,
        ) -> Result<GetSwitchNodeCountResponse, NmxcError> {
            Ok(GetSwitchNodeCountResponse {
                server_header: None,
                context: None,
                num_nodes: 0,
            })
        }

        async fn get_switch_node_info_list(
            &self,
            _req: nmxc_model::GetSwitchNodeInfoListRequest,
        ) -> Result<GetSwitchNodeInfoListResponse, NmxcError> {
            Ok(GetSwitchNodeInfoListResponse {
                server_header: None,
                context: None,
                node_info_list: vec![],
            })
        }

        async fn get_partition_count(
            &self,
            _req: nmxc_model::GetPartitionCountRequest,
        ) -> Result<GetPartitionCountResponse, NmxcError> {
            let n = self._partitions.lock().unwrap().len() as u32;
            Ok(GetPartitionCountResponse {
                server_header: None,
                context: None,
                num_partitions: n,
            })
        }

        async fn get_partition_id_list(
            &self,
            _req: nmxc_model::GetPartitionIdListRequest,
        ) -> Result<GetPartitionIdListResponse, NmxcError> {
            let parts = self._partitions.lock().unwrap();
            let partition_list = parts
                .iter()
                .map(|p| nmxc_model::Partition {
                    partition_id: Some(nmxc_model::PartitionId {
                        partition_id: p.partition_id,
                    }),
                    num_gpus: p.gpu_uids.len() as u32,
                })
                .collect();
            Ok(GetPartitionIdListResponse {
                server_header: None,
                context: None,
                partition_list,
            })
        }

        async fn get_partition_info_list(
            &self,
            req: nmxc_model::GetPartitionInfoListRequest,
        ) -> Result<GetPartitionInfoListResponse, NmxcError> {
            let parts = self._partitions.lock().unwrap();
            let partition_info_list: Vec<nmxc_model::PartitionInfo> =
                if req.partition_id_list.is_empty() {
                    parts.iter().map(Self::to_partition_info).collect()
                } else {
                    let wanted: HashSet<u32> = req
                        .partition_id_list
                        .iter()
                        .map(|p| p.partition_id)
                        .collect();
                    parts
                        .iter()
                        .filter(|p| wanted.contains(&p.partition_id))
                        .map(Self::to_partition_info)
                        .collect()
                };
            Ok(GetPartitionInfoListResponse {
                server_header: None,
                context: None,
                partition_info_list,
            })
        }

        async fn create_partition(
            &self,
            req: nmxc_model::CreatePartitionRequest,
        ) -> Result<nmxc_model::CreatePartitionResponse, NmxcError> {
            if let Some(fail_counter) = &self._fail_after_n_creates {
                let mut fail_counter = fail_counter.lock().unwrap();
                if *fail_counter == 0 {
                    return Err(NmxcError::invalid_response("fail after n creates"));
                }
                *fail_counter -= 1;
            }
            let gpu_uids = Self::uids_from_resource_ids(&req.gpu_resource_id);
            let partition_id = if let Some(ref pid) = req.partition_id {
                pid.partition_id
            } else {
                let mut next = self._next_partition_id.lock().unwrap();
                let id = *next;
                *next = next.saturating_add(1);
                id
            };
            self._partitions.lock().unwrap().push(SimPartition {
                partition_id,
                name: req.name,
                gpu_uids,
            });
            Ok(nmxc_model::CreatePartitionResponse {
                server_header: None,
                context: None,
                partition_id: Some(nmxc_model::PartitionId { partition_id }),
            })
        }

        async fn delete_partition(
            &self,
            req: nmxc_model::DeletePartitionRequest,
        ) -> Result<nmxc_model::DeletePartitionResponse, NmxcError> {
            let pid = req.partition_id.map(|p| p.partition_id).unwrap_or_default();
            self._partitions
                .lock()
                .unwrap()
                .retain(|p| p.partition_id != pid);
            Ok(nmxc_model::DeletePartitionResponse {
                server_header: None,
                context: None,
                partition_id: Some(nmxc_model::PartitionId { partition_id: pid }),
            })
        }

        async fn add_gpus_to_partition(
            &self,
            req: nmxc_model::UpdatePartitionRequest,
        ) -> Result<nmxc_model::UpdatePartitionResponse, NmxcError> {
            let pid = req
                .partition_id
                .as_ref()
                .ok_or_else(|| NmxcError::invalid_response("partition_id"))?
                .partition_id;
            let mut parts = self._partitions.lock().unwrap();
            let partition = parts
                .iter_mut()
                .find(|p| p.partition_id == pid)
                .ok_or_else(|| NmxcError::invalid_response("partition not found"))?;
            for u in &req.gpu_uid {
                if !partition.gpu_uids.contains(u) {
                    partition.gpu_uids.push(*u);
                }
            }
            Ok(nmxc_model::UpdatePartitionResponse {
                server_header: None,
                context: None,
                partition_id: Some(nmxc_model::PartitionId { partition_id: pid }),
            })
        }

        async fn remove_gpus_from_partition(
            &self,
            req: nmxc_model::UpdatePartitionRequest,
        ) -> Result<nmxc_model::UpdatePartitionResponse, NmxcError> {
            let pid = req
                .partition_id
                .as_ref()
                .ok_or_else(|| NmxcError::invalid_response("partition_id"))?
                .partition_id;
            let remove: HashSet<u64> = req.gpu_uid.iter().copied().collect();
            let mut parts = self._partitions.lock().unwrap();
            let partition = parts
                .iter_mut()
                .find(|p| p.partition_id == pid)
                .ok_or_else(|| NmxcError::invalid_response("partition not found"))?;
            partition.gpu_uids.retain(|u| !remove.contains(u));
            Ok(nmxc_model::UpdatePartitionResponse {
                server_header: None,
                context: None,
                partition_id: Some(nmxc_model::PartitionId { partition_id: pid }),
            })
        }
    }

    #[async_trait]
    impl NmxcPool for NmxcSimClient {
        async fn create_client(&self, _endpoint: Endpoint) -> Result<Box<dyn Nmxc>, NmxcError> {
            Ok(Box::new(NmxcSimClient {
                _partitions: self._partitions.clone(),
                _next_partition_id: self._next_partition_id.clone(),
                _fail_after_n_creates: self._fail_after_n_creates.clone(),
            }))
        }
    }

    /// [`NmxcPool`] that dials the NMX-C gRPC **simulator** on localhost (default port 9601).
    ///
    /// Ignores the `endpoint` argument on [`NmxcPool::create_client`] and always connects to
    /// [`Self::simulator_endpoint`], so carbide config’s `nmx_c_endpoint` does not need to match
    /// the simulator when using this pool in tests.
    #[derive(Debug, Clone)]
    pub struct NmxcSimClient2 {
        pool: NmxcClientPool,
        simulator_endpoint: Endpoint,
    }

    impl NmxcSimClient2 {
        /// Default simulator URL: plain gRPC on port 9601 (`http://localhost:9601`).
        pub const SIMULATOR_URL: &'static str = "http://localhost:9601";

        pub fn new() -> Self {
            Self {
                pool: NmxcClientPool::builder()
                    .build()
                    .expect("NmxcClientPool::builder default"),
                simulator_endpoint: Endpoint::new(Self::SIMULATOR_URL),
            }
        }

        /// Same as [`Self::new`] but uses a custom base URL (scheme/host/port).
        pub fn with_simulator_url(url: impl Into<String>) -> Self {
            Self {
                pool: NmxcClientPool::builder()
                    .build()
                    .expect("NmxcClientPool::builder default"),
                simulator_endpoint: Endpoint::new(url),
            }
        }

        pub fn simulator_endpoint(&self) -> &Endpoint {
            &self.simulator_endpoint
        }
    }

    #[async_trait]
    impl NmxcPool for NmxcSimClient2 {
        async fn create_client(&self, _endpoint: Endpoint) -> Result<Box<dyn Nmxc>, NmxcError> {
            self.pool
                .create_client(self.simulator_endpoint.clone())
                .await
        }
    }

    #[cfg(test)]
    mod nmxc_sim_client2_tests {
        use std::sync::Arc;

        use libnmxc::NmxcPool;

        use super::NmxcSimClient2;

        #[test]
        fn default_simulator_url_is_localhost_9601() {
            let s = NmxcSimClient2::new();
            assert_eq!(NmxcSimClient2::SIMULATOR_URL, "http://localhost:9601");
            assert_eq!(s.simulator_endpoint().url, NmxcSimClient2::SIMULATOR_URL);
        }

        #[test]
        fn with_simulator_url_overrides_endpoint() {
            let s = NmxcSimClient2::with_simulator_url("http://127.0.0.1:19999");
            assert_eq!(s.simulator_endpoint().url, "http://127.0.0.1:19999");
        }

        #[test]
        fn implements_nmxc_pool() {
            let _pool: Arc<dyn NmxcPool> = Arc::new(NmxcSimClient2::new());
        }
    }
}

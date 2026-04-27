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
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;
use std::time::Duration;

use carbide_uuid::machine::MachineId;
use carbide_uuid::nvlink::{NvLinkDomainId, NvLinkLogicalPartitionId, NvLinkPartitionId};
use chrono::Utc;
use config_version::Versioned;
use db::machine::find_machine_ids;
use db::managed_host::load_by_machine_ids;
use db::nvl_logical_partition::IdColumn as LpIdColumn;
use db::nvl_partition::IdColumn;
use db::work_lock_manager::WorkLockManagerHandle;
use db::{self, ObjectColumnFilter, machine};
use libnmxc::nmxc_model::{GetPartitionInfoListRequest, PartitionInfo};
use libnmxc::{Endpoint, NMX_C_GATEWAY_ID, Nmxc, NmxcPool};
use metrics::{AppliedChange, NmxmPartitionOperationStatus, NvlPartitionMonitorMetrics};
use model::hardware_info::{HardwareInfo, MachineNvLinkInfo};
use model::instance::status::SyncState;
use model::instance::status::nvlink::InstanceNvLinkStatus;
use model::machine::machine_search_config::MachineSearchConfig;
use model::machine::nvlink::{MachineNvLinkGpuStatusObservation, MachineNvLinkStatusObservation};
use model::machine::{HostHealthConfig, LoadSnapshotOptions, ManagedHostStateSnapshot};
use model::nvl_logical_partition::LogicalPartition;
use model::nvl_partition::{NvlPartition, NvlPartitionName};
use sqlx::PgPool;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use utils::periodic_timer::PeriodicTimer;

use crate::api::TransactionVending;
use crate::cfg::file::NvLinkConfig;
use crate::{CarbideError, CarbideResult};

mod metrics;

#[derive(Debug, Clone)]
struct NmxcPartitionOperation {
    domain_uuid: Option<NvLinkDomainId>,
    operation_type: NmxcPartitionOperationType,
    gpu_uids: Vec<u64>,
    name: String,
    db_partition_id: Option<NvLinkPartitionId>,
}

#[derive(Debug, Clone)]
pub enum NmxcPartitionOperationType {
    Create,
    Remove(u32),                 // NMX-C partition ID
    RemoveUnknownPartition(u32), // NMX-C partition ID
    Update(u32),                 // NMX-C partition ID
}

#[derive(Debug, Clone)]
enum GpuAction {
    AddToPartition,
    RemoveFromPartition,
    RemoveFromUnknownPartition,
    NoOp,
}

// Context for GPU helper functions in check_nv_link_partitions
struct GpuProcessingContext {
    gpu_guid: u64,
    domain_uuid: NvLinkDomainId,
    logical_partition_id: Option<NvLinkLogicalPartitionId>,
    partition_id: Option<NvLinkPartitionId>,
    partition_name: String,
    partition_nmx_c_id: libnmxc::nmxc_model::PartitionId,
}

impl Default for GpuProcessingContext {
    fn default() -> Self {
        Self {
            gpu_guid: 0,
            domain_uuid: NvLinkDomainId::default(),
            logical_partition_id: None,
            partition_id: None,
            partition_name: "".to_string(),
            partition_nmx_c_id: libnmxc::nmxc_model::PartitionId::default(),
        }
    }
}

// Context for partition helper functions in check_nv_link_partitions.
pub struct PartitionProcessingContext {
    nmx_c_partitions: HashMap<libnmxc::nmxc_model::PartitionId, PartitionInfo>,
    db_nvl_logical_partitions: HashMap<NvLinkLogicalPartitionId, LogicalPartition>,
    db_nvl_partitions: HashMap<u32, NvlPartition>, // NMX-C partition ID to NvlPartition
    machine_nvlink_info: HashMap<MachineId, Option<MachineNvLinkInfo>>,
    gpu_to_partition_map: HashMap<u64, PartitionInfo>, // GPU UID to NMX-C partition
    nmx_c_operations: HashMap<NvLinkLogicalPartitionId, Vec<NmxcPartitionOperation>>,
    unknown_partition_removal_operations: HashMap<u32, Vec<NmxcPartitionOperation>>,
    unknown_partition_addition_operations: HashMap<u32, NmxcPartitionOperation>,
}

fn nmx_c_partition_id_string(pi: &PartitionInfo) -> String {
    pi.partition_id
        .as_ref()
        .map(|id| id.partition_id.to_string())
        .unwrap_or_default()
}

/// First non-empty `GpuPlatformInfo::chassis_serial` from the **first** snapshot's host `HardwareInfo`.
fn chassis_serial_from_snapshots(snapshots: &[&ManagedHostStateSnapshot]) -> Option<String> {
    let snapshot = snapshots.first()?;
    chassis_serial_from_hardware_info(snapshot.host_snapshot.hardware_info.as_ref())
}

fn chassis_serial_from_hardware_info(hi: Option<&HardwareInfo>) -> Option<String> {
    let hi = hi?;
    for gpu in &hi.gpus {
        if let Some(ref pi) = gpu.platform_info {
            if !pi.chassis_serial.is_empty() {
                return Some(pi.chassis_serial.clone());
            }
        }
    }
    None
}

fn is_nmx_c_default_partition(partition: &PartitionInfo) -> bool {
    let id_is_default = partition
        .partition_id
        .as_ref()
        .is_some_and(|id| id.partition_id == 32766);
    id_is_default || partition.name.contains("Default")
}

impl PartitionProcessingContext {
    fn new(
        nmx_c_partitions: Vec<PartitionInfo>,
        db_nvl_logical_partitions: Vec<LogicalPartition>,
        db_nvl_partitions: Vec<NvlPartition>,
        machine_nvlink_info: HashMap<MachineId, Option<MachineNvLinkInfo>>,
    ) -> Self {
        let gpu_map = Self::build_gpu_to_partition_map(&nmx_c_partitions);
        let nmx_c_partitions = nmx_c_partitions
            .into_iter()
            .filter_map(|p| p.partition_id.map(|id| (id, p)))
            .collect();
        let db_nvl_logical_partitions = db_nvl_logical_partitions
            .into_iter()
            .map(|p| (p.id, p))
            .collect();
        let db_nvl_partitions = db_nvl_partitions
            .into_iter()
            .filter_map(|p| p.nmx_m_id.parse::<u32>().ok().map(|id| (id, p)))
            .collect();
        Self {
            nmx_c_partitions,
            db_nvl_logical_partitions,
            db_nvl_partitions,
            machine_nvlink_info,
            gpu_to_partition_map: gpu_map,
            nmx_c_operations: HashMap::new(),
            unknown_partition_removal_operations: HashMap::new(),
            unknown_partition_addition_operations: HashMap::new(),
        }
    }
    // Build a map from GPU UIDs (as string) to partition from NMX-C partition info list.
    fn build_gpu_to_partition_map(
        nmx_c_partitions: &[PartitionInfo],
    ) -> HashMap<u64, PartitionInfo> {
        let mut gpu_map = HashMap::new();
        for partition in nmx_c_partitions {
            for gpu_uid in &partition.gpu_uid_list {
                gpu_map.insert(*gpu_uid, partition.clone());
            }
        }
        gpu_map
    }

    // Validate that a logical partition exists and is not deleted
    fn validate_logical_partition(&self, logical_partition_id: &NvLinkLogicalPartitionId) -> bool {
        if let Some(matching_logical_partition) =
            self.db_nvl_logical_partitions.get(logical_partition_id)
        {
            if model::nvl_logical_partition::is_marked_as_deleted(matching_logical_partition) {
                tracing::error!(
                    "logical partition already marked as deleted, cannot modify physical partition"
                );
                return false;
            }
            true
        } else {
            tracing::error!("logical partition {} not found!!", logical_partition_id);
            false
        }
    }

    // Get partition information from the database for a given NMX-C partition ID (numeric key).
    fn get_db_partition_info(
        &self,
        nmx_c_partition_id: u32,
    ) -> Option<(
        Option<NvLinkPartitionId>,
        Option<NvLinkLogicalPartitionId>,
        String,
        libnmxc::nmxc_model::PartitionId,
    )> {
        self.db_nvl_partitions.get(&nmx_c_partition_id).map(|p| {
            (
                Some(p.id),
                p.logical_partition_id,
                p.name.clone().into(),
                libnmxc::nmxc_model::PartitionId {
                    partition_id: nmx_c_partition_id,
                },
            )
        })
    }

    // Get the list of GPUs that should remain in a partition after removing a specific GPU from a logical partition.
    // To remove a GPU from a partition in NMX-M, we need to do an update op with every other GPU in the partition except the one
    // getting removed.
    fn get_gpus_to_keep_after_removal(
        &self,
        logical_partition_id: Option<NvLinkLogicalPartitionId>,
        partition_nmx_c_id: &libnmxc::nmxc_model::PartitionId,
        gpu_guid: u64,
        machine_id: &MachineId,
        device_instance: u32,
    ) -> Option<Vec<u64>> {
        let Some(logical_partition_id) = logical_partition_id else {
            tracing::error!(
                "Logical partition ID is required for getting GPUs to keep after removal"
            );
            return None;
        };
        let gpus_to_keep = match self.nmx_c_operations.get(&logical_partition_id) {
            Some(ops) => {
                if let Some(op) = ops.iter().find(|op| op.gpu_uids.contains(&gpu_guid)) {
                    op.gpu_uids
                        .iter()
                        .copied()
                        .filter(|id| *id != gpu_guid)
                        .collect()
                } else {
                    // No operation found for this physical partition, so get the partition members from NMX-C.
                    match self.nmx_c_partitions.get(partition_nmx_c_id) {
                        Some(p) => p
                            .gpu_uid_list
                            .iter()
                            .copied()
                            .filter(|&id| id != gpu_guid)
                            .collect(),
                        None => {
                            tracing::error!(
                                "NMX-C partition not found for machine {}, GPU index {}",
                                machine_id,
                                device_instance
                            );
                            return None;
                        }
                    }
                }
            }
            None => {
                // No pending operations found, so get the GPUs from NMX-C.
                match self.nmx_c_partitions.get(partition_nmx_c_id) {
                    Some(p) => p
                        .gpu_uid_list
                        .iter()
                        .copied()
                        .filter(|id| *id != gpu_guid)
                        .collect(),
                    None => {
                        tracing::error!(
                            "NMX-C partition not found for machine {}, GPU index {}",
                            machine_id,
                            device_instance
                        );
                        return None;
                    }
                }
            }
        }; // Some(gpus_to_keep)
        Some(gpus_to_keep)
    }

    fn get_gpus_to_keep_in_unknown_partition_after_removal(
        &self,
        partition_nmx_c_id: &libnmxc::nmxc_model::PartitionId,
        gpu_guid: u64,
        machine_id: &MachineId,
        device_instance: u32,
    ) -> Option<Vec<u64>> {
        let gpus_to_keep = match self
            .unknown_partition_removal_operations
            .get(&partition_nmx_c_id.partition_id)
        {
            Some(ops) => {
                if let Some(op) = ops.iter().find(|op| op.gpu_uids.contains(&gpu_guid)) {
                    op.gpu_uids
                        .iter()
                        .copied()
                        .filter(|id| *id != gpu_guid)
                        .collect()
                } else {
                    // No operation found for this GPU, so get the GPUs from the default partition.
                    match self.nmx_c_partitions.get(partition_nmx_c_id) {
                        Some(p) => p
                            .gpu_uid_list
                            .iter()
                            .copied()
                            .filter(|id| *id != gpu_guid)
                            .collect(),
                        None => {
                            tracing::error!(
                                "NMX-C partition not found for machine {}, GPU index {}",
                                machine_id,
                                device_instance
                            );
                            return None;
                        }
                    }
                }
            }
            None => {
                // No removal operations found, so get the GPUs from the unknown partition.
                match self.nmx_c_partitions.get(partition_nmx_c_id) {
                    Some(p) => p
                        .gpu_uid_list
                        .iter()
                        .copied()
                        .filter(|id| *id != gpu_guid)
                        .collect(),
                    None => {
                        tracing::error!(
                            "NMX-C partition not found for machine {}, GPU index {}",
                            machine_id,
                            device_instance
                        );
                        return None;
                    }
                }
            }
        }; // Some(gpus_to_keep)
        Some(gpus_to_keep) // Some(gpus_to_keep)
    }

    // Handle GPU removal from a logical partition
    fn handle_gpu_removal(
        &mut self,
        ctx: &GpuProcessingContext,
        gpus_to_keep: Vec<u64>,
    ) -> CarbideResult<()> {
        let Some(logical_partition_id) = ctx.logical_partition_id else {
            return Err(CarbideError::internal(
                "Logical partition ID is required for GPU removal".to_string(),
            ));
        };
        if gpus_to_keep.is_empty() {
            // All members need to be removed, enqueue a Remove request
            let operation = NmxcPartitionOperation {
                domain_uuid: Some(ctx.domain_uuid),
                operation_type: NmxcPartitionOperationType::Remove(
                    ctx.partition_nmx_c_id.partition_id,
                ),
                gpu_uids: gpus_to_keep.clone(),
                name: ctx.partition_name.clone(),
                db_partition_id: ctx.partition_id,
            };

            self.nmx_c_operations
                .entry(logical_partition_id)
                .and_modify(|ops| {
                    if let Some(op) = ops
                        .iter_mut()
                        .find(|op| op.gpu_uids.contains(&ctx.gpu_guid))
                    {
                        op.operation_type =
                            NmxcPartitionOperationType::Remove(ctx.partition_nmx_c_id.partition_id);
                        op.gpu_uids = gpus_to_keep.clone();
                        op.name = ctx.partition_name.clone();
                    } else {
                        ops.push(operation.clone());
                    }
                })
                .or_insert(vec![operation]);
        } else {
            // Some members remain, enqueue an Update request
            let operation = NmxcPartitionOperation {
                domain_uuid: Some(ctx.domain_uuid),
                operation_type: NmxcPartitionOperationType::Update(
                    ctx.partition_nmx_c_id.partition_id,
                ),
                gpu_uids: gpus_to_keep.clone(),
                name: ctx.partition_name.clone(),
                db_partition_id: ctx.partition_id,
            };

            self.nmx_c_operations
                .entry(logical_partition_id)
                .and_modify(|ops| {
                    if let Some(op) = ops
                        .iter_mut()
                        .find(|op| op.gpu_uids.contains(&ctx.gpu_guid))
                    {
                        op.operation_type =
                            NmxcPartitionOperationType::Update(ctx.partition_nmx_c_id.partition_id);
                        op.gpu_uids = gpus_to_keep.clone();
                        op.name = ctx.partition_name.clone();
                    } else {
                        ops.push(operation.clone());
                    }
                })
                .or_insert(vec![operation]);
        }
        Ok(())
    }

    // Handle GPU removal from the unknown partition
    fn handle_gpu_removal_from_unknown_partition(
        &mut self,
        partition_nmx_c_id: &libnmxc::nmxc_model::PartitionId,
        gpu_guid: u64,
        gpus_to_keep: Vec<u64>,
    ) -> CarbideResult<()> {
        if gpus_to_keep.is_empty() {
            let operation = NmxcPartitionOperation {
                domain_uuid: None,
                operation_type: NmxcPartitionOperationType::RemoveUnknownPartition(
                    partition_nmx_c_id.partition_id,
                ),
                gpu_uids: gpus_to_keep.clone(),
                name: "".to_string(),
                db_partition_id: None,
            };

            self.unknown_partition_removal_operations
                .entry(partition_nmx_c_id.partition_id)
                .and_modify(|ops| {
                    if let Some(op) = ops.iter_mut().find(|op| op.gpu_uids.contains(&gpu_guid)) {
                        op.operation_type = NmxcPartitionOperationType::RemoveUnknownPartition(
                            partition_nmx_c_id.partition_id,
                        );
                        op.gpu_uids = gpus_to_keep.clone();
                    } else {
                        ops.push(operation.clone());
                    }
                })
                .or_insert(vec![operation.clone()]);
        } else {
            let operation = NmxcPartitionOperation {
                domain_uuid: None,
                operation_type: NmxcPartitionOperationType::Update(partition_nmx_c_id.partition_id),
                gpu_uids: gpus_to_keep.clone(),
                name: "".to_string(),
                db_partition_id: None,
            };
            self.unknown_partition_removal_operations
                .entry(partition_nmx_c_id.partition_id)
                .and_modify(|ops| {
                    if let Some(op) = ops.iter_mut().find(|op| op.gpu_uids.contains(&gpu_guid)) {
                        op.operation_type =
                            NmxcPartitionOperationType::Update(partition_nmx_c_id.partition_id);
                        op.gpu_uids = gpus_to_keep.clone();
                    } else {
                        ops.push(operation.clone());
                    }
                })
                .or_insert(vec![operation.clone()]);
        }
        Ok(())
    }

    fn handle_gpu_addition_to_unknown_partition(
        &mut self,
        partition_nmx_c_id: &libnmxc::nmxc_model::PartitionId,
        gpu_guid: u64,
        gpus_in_partition: Vec<u64>,
    ) -> CarbideResult<()> {
        let pid = partition_nmx_c_id.partition_id;
        let mut gpu_uids = gpus_in_partition;
        gpu_uids.push(gpu_guid);
        let operation = NmxcPartitionOperation {
            domain_uuid: None,
            operation_type: NmxcPartitionOperationType::Update(pid),
            gpu_uids,
            name: "".to_string(),
            db_partition_id: None,
        };
        self.unknown_partition_addition_operations
            .entry(pid)
            .and_modify(|op| {
                if !op.gpu_uids.contains(&gpu_guid) {
                    op.gpu_uids.push(gpu_guid);
                }
            })
            .or_insert(operation);
        Ok(())
    }

    // Handle GPU addition to a logical partition when no other partitions exist in the logical partition.
    fn handle_gpu_addition_new_partition(
        &mut self,
        ctx: &GpuProcessingContext,
    ) -> CarbideResult<()> {
        let Some(logical_partition_id) = ctx.logical_partition_id else {
            return Err(CarbideError::internal(
                "Logical partition ID is required for GPU addition to new partition".to_string(),
            ));
        };
        let operation = NmxcPartitionOperation {
            domain_uuid: Some(ctx.domain_uuid),
            operation_type: NmxcPartitionOperationType::Create,
            gpu_uids: vec![ctx.gpu_guid],
            name: format!("{}{}", logical_partition_id, ctx.gpu_guid),
            db_partition_id: None,
        };

        self.nmx_c_operations
            .entry(logical_partition_id)
            .and_modify(|ops| {
                if let Some(op) = ops
                    .iter_mut()
                    .find(|op| op.domain_uuid.unwrap_or_default() == ctx.domain_uuid)
                {
                    op.gpu_uids.push(ctx.gpu_guid);
                } else {
                    ops.push(operation.clone());
                }
            })
            .or_insert(vec![operation]);
        Ok(())
    }

    // Handle GPU addition to an existing partition in the same domain
    fn handle_gpu_addition_existing_partition(
        &mut self,
        ctx: &GpuProcessingContext,
        partition: &NvlPartition,
    ) -> CarbideResult<()> {
        let Some(logical_partition_id) = ctx.logical_partition_id else {
            return Err(CarbideError::internal(
                "Logical partition ID is required for GPU addition to existing partition"
                    .to_string(),
            ));
        };

        // Get the GPU IDs that are already in the partition, plus the GPU being added.
        let nmx_c_partition_id = partition.nmx_m_id.parse::<u32>().unwrap();
        let gpu_uids: Vec<u64> = if let Some(nmx_c_partition) =
            self.nmx_c_partitions
                .get(&libnmxc::nmxc_model::PartitionId {
                    partition_id: nmx_c_partition_id,
                }) {
            nmx_c_partition
                .gpu_uid_list
                .iter()
                .copied()
                .chain(std::iter::once(ctx.gpu_guid))
                .collect()
        } else {
            return Err(CarbideError::internal(
                "NMX-C partition not found for GPU addition to existing partition".to_string(),
            ));
        };

        let operation = NmxcPartitionOperation {
            domain_uuid: Some(ctx.domain_uuid),
            operation_type: NmxcPartitionOperationType::Update(nmx_c_partition_id),
            gpu_uids,
            name: partition.name.clone().into(),
            db_partition_id: ctx.partition_id, // TODO: should try to verify that these are not nil
        };

        self.nmx_c_operations
            .entry(logical_partition_id)
            .and_modify(|ops| {
                if let Some(op) = ops.iter_mut().find(|op| match &op.operation_type {
                    NmxcPartitionOperationType::Update(partition_id) => {
                        *partition_id == nmx_c_partition_id
                    }
                    _ => false,
                }) {
                    op.gpu_uids.push(ctx.gpu_guid);
                } else {
                    ops.push(operation.clone());
                }
            })
            .or_insert(vec![operation]);
        Ok(())
    }
}

pub struct NvlPartitionMonitor {
    db_pool: PgPool,
    nmxc_client_pool: Arc<dyn NmxcPool>,
    config: NvLinkConfig,
    host_health: HostHealthConfig,
    metric_holder: Arc<metrics::MetricHolder>,
    work_lock_manager_handle: WorkLockManagerHandle,
}

impl NvlPartitionMonitor {
    const ITERATION_WORK_KEY: &'static str = "NvlPartitionMonitor::run_single_iteration";

    pub fn new(
        db_pool: PgPool,
        nmxc_client_pool: Arc<dyn NmxcPool>,
        meter: opentelemetry::metrics::Meter,
        config: NvLinkConfig,
        host_health: HostHealthConfig,
        work_lock_manager_handle: WorkLockManagerHandle,
    ) -> Self {
        let hold_period = config
            .monitor_run_interval
            .saturating_add(std::time::Duration::from_secs(60));

        let metric_holder = Arc::new(metrics::MetricHolder::new(meter, hold_period));

        Self {
            db_pool,
            nmxc_client_pool,
            config,
            host_health,
            metric_holder,
            work_lock_manager_handle,
        }
    }

    pub fn start(
        self,
        join_set: &mut JoinSet<()>,
        cancel_token: CancellationToken,
    ) -> io::Result<()> {
        if self.config.enabled {
            join_set
                .build_task()
                .name("nvl-partition-monitor")
                .spawn(async move { self.run(cancel_token).await })?;
        }

        Ok(())
    }

    pub async fn run(&self, cancel_token: CancellationToken) {
        let timer = PeriodicTimer::new(self.config.monitor_run_interval);
        loop {
            let mut tick = timer.tick();
            match self.run_single_iteration().await {
                Ok(num_changes) => {
                    if num_changes > 0 {
                        // Decrease the interval if changes have been made.
                        tick.set_interval(Duration::from_millis(1000));
                    }
                }
                Err(e) => {
                    tracing::warn!("NvlPartitionMonitor error: {}", e);
                }
            }

            tokio::select! {
                _ = tick.sleep() => {},
                _ = cancel_token.cancelled() => {
                    tracing::info!("NvlPartitionMonitor stop was requested");
                    return;
                }
            }
        }
    }

    pub async fn run_single_iteration(&self) -> CarbideResult<usize> {
        let mut metrics = NvlPartitionMonitorMetrics::new();
        let span_id: String = format!("{:#x}", u64::from_le_bytes(rand::random::<[u8; 8]>()));
        let check_nvl_partition_span = tracing::span!(
            parent: None,
            tracing::Level::INFO,
            "nvl_partition_monitor",
            span_id,
            otel.status_code = tracing::field::Empty,
            otel.status_message = tracing::field::Empty,
            metrics = tracing::field::Empty,
        );
        let result = self
            .run_single_iteration_inner(&mut metrics)
            .instrument(check_nvl_partition_span.clone())
            .await;
        check_nvl_partition_span.record(
            "otel.status_code",
            if result.is_ok() { "ok" } else { "error" },
        );
        if let Err(ref e) = result {
            check_nvl_partition_span.record("otel.status_message", format!("{e:?}"));
        }
        check_nvl_partition_span.record("metrics", metrics.to_string());
        self.metric_holder.update_metrics(metrics);
        result
    }

    async fn run_single_iteration_inner(
        &self,
        metrics: &mut NvlPartitionMonitorMetrics,
    ) -> CarbideResult<usize> {
        let _lock = match self
            .work_lock_manager_handle
            .try_acquire_lock(Self::ITERATION_WORK_KEY.into())
            .await
        {
            Ok(lock) => lock,
            Err(e) => {
                tracing::warn!(
                    "NvlPartitionMonitor failed to acquire work lock: Another instance of carbide running? {e}"
                );
                return Ok(0);
            }
        };
        tracing::trace!(
            lock = Self::ITERATION_WORK_KEY,
            "NvlPartitionMonitor acquired the lock",
        );

        let mut txn = self.db_pool.txn_begin().await?;
        let managed_host_snapshots = self.load_mnnvl_managed_host_snapshots(txn.as_mut()).await?;
        let machine_nvlink_info = machine::find_nvlink_info_by_machine_ids(
            &mut txn,
            &managed_host_snapshots.keys().copied().collect::<Vec<_>>(),
        )
        .await?;
        let managed_host_snapshots_by_domain: HashMap<
            NvLinkDomainId,
            Vec<&ManagedHostStateSnapshot>,
        > = managed_host_snapshots.iter().fold(
            HashMap::new(),
            |mut acc, (_machine_id, snapshot)| {
                if let Some(nvlink_info) = snapshot.host_snapshot.nvlink_info.as_ref() {
                    acc.entry(nvlink_info.domain_uuid)
                        .or_default()
                        .push(snapshot);
                }
                acc
            },
        );
        let domain_to_chassis_serial: HashMap<NvLinkDomainId, String> =
            managed_host_snapshots_by_domain
                .iter()
                .map(|(domain_id, snapshots)| {
                    chassis_serial_from_snapshots(snapshots.as_slice())
                        .ok_or_else(|| {
                            CarbideError::internal(format!(
                                "Missing GpuPlatformInfo.chassis_serial in hardware info for NvLink domain {domain_id}"
                            ))
                        })
                        .map(|serial| (*domain_id, serial))
                })
                .collect::<Result<_, _>>()?;
        let db_nvl_partitions =
            db::nvl_partition::find_by(&mut txn, ObjectColumnFilter::<IdColumn>::All).await?;

        let db_nvl_logical_partitions =
            db::nvl_logical_partition::find_by(&mut txn, ObjectColumnFilter::<LpIdColumn>::All)
                .await?;

        let chassis_serial_to_endpoint: HashMap<String, String> =
            db::nvlink_nmxc_endpoints::find_all(&mut txn)
                .await?
                .into_iter()
                .map(|r| (r.chassis_serial, r.endpoint))
                .collect();
        let mut domain_to_endpoint: HashMap<NvLinkDomainId, String> =
            HashMap::with_capacity(domain_to_chassis_serial.len());
        for (domain_id, chassis_serial) in &domain_to_chassis_serial {
            if let Some(endpoint) = chassis_serial_to_endpoint.get(chassis_serial) {
                domain_to_endpoint.insert(*domain_id, endpoint.clone());
            } else {
                tracing::debug!(
                    domain_id = %domain_id,
                    %chassis_serial,
                    "nvlink_nmxc_endpoint not found; skipping NMX-C for this domain"
                );
            }
        }

        // Don't hold the transaction across unrelated awaits
        txn.commit().await?;

        metrics.num_logical_partitions = db_nvl_logical_partitions.len();
        metrics.num_physical_partitions = db_nvl_partitions.len();

        let mut total_completed_operations = 0;

        for (domain_id, domain_snapshots) in &managed_host_snapshots_by_domain {
            let Some(endpoint_url) = domain_to_endpoint.get(domain_id) else {
                tracing::debug!(
                    domain_id = %domain_id,
                    "No nvlink_nmxc_endpoints mapping for domain; skipping partition monitor work"
                );
                continue;
            };

            let nmxc_client = match self
                .nmxc_client_pool
                .create_client(Endpoint::new(endpoint_url))
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        domain_id = %domain_id,
                        endpoint = %endpoint_url,
                        error = %e,
                        "Failed to create NMX-C client; skipping partition monitor work for this domain"
                    );
                    continue;
                }
            };

            // filter managed host snapshots, nvlink info and db partitionsby domain
            let managed_host_snapshots_domain: HashMap<MachineId, ManagedHostStateSnapshot> =
                domain_snapshots
                    .iter()
                    .map(|s| (s.host_snapshot.id, (*s).clone()))
                    .collect();
            let machine_ids_in_domain: Vec<MachineId> =
                managed_host_snapshots_domain.keys().copied().collect();
            let machine_nvlink_info_domain: HashMap<MachineId, Option<MachineNvLinkInfo>> =
                machine_nvlink_info
                    .iter()
                    .filter(|(id, _)| machine_ids_in_domain.contains(id))
                    .map(|(k, v)| (*k, v.clone()))
                    .collect();
            let db_nvl_partitions_domain: Vec<NvlPartition> = db_nvl_partitions
                .iter()
                .filter(|p| p.domain_uuid == *domain_id)
                .cloned()
                .collect();

            let num_completed = self
                .check_partitions_and_apply_nmx_c_operations(
                    nmxc_client.as_ref(),
                    metrics,
                    db_nvl_logical_partitions.clone(),
                    db_nvl_partitions_domain,
                    machine_nvlink_info_domain,
                    managed_host_snapshots_domain,
                )
                .await?;
            total_completed_operations += num_completed;
        }

        metrics.num_completed_operations = total_completed_operations;

        Ok(total_completed_operations)
    }

    /// Fetches partition list from NMX-C, checks for needed create/update/delete operations,
    /// executes them, polls for completion, and updates the DB with the results.
    async fn check_partitions_and_apply_nmx_c_operations(
        &self,
        nmxc_client: &dyn Nmxc,
        metrics: &mut NvlPartitionMonitorMetrics,
        db_nvl_logical_partitions: Vec<LogicalPartition>,
        db_nvl_partitions: Vec<NvlPartition>,
        machine_nvlink_info: HashMap<MachineId, Option<MachineNvLinkInfo>>,
        managed_host_snapshots: HashMap<MachineId, ManagedHostStateSnapshot>,
    ) -> CarbideResult<usize> {
        let partition_info_list = nmxc_client
            .get_partition_info_list(GetPartitionInfoListRequest {
                context: Some(libnmxc::nmxc_model::Context {
                    context: String::new(),
                }),
                partition_id_list: vec![],
                partition_name_list: vec![],
                gateway_id: NMX_C_GATEWAY_ID.into(),
            })
            .await
            .map_err(|e| {
                metrics.nmxm.connect_error = "Failed to get NMX-C partition info list".to_string();
                CarbideError::internal(format!("Failed to get NMX-C partition info list: {e}"))
            })?
            .partition_info_list;

        metrics.nmxm.num_partitions += partition_info_list.len();

        let mut partition_processing_context = PartitionProcessingContext::new(
            partition_info_list,
            db_nvl_logical_partitions.clone(),
            db_nvl_partitions,
            machine_nvlink_info,
        );

        // Check if any partitions need to be created, updated, or deleted.
        let observations = self.check_nv_link_partitions(
            &mut partition_processing_context,
            managed_host_snapshots,
            metrics,
        )?;

        self.record_nvlink_status_observation(observations).await?;

        let nmx_c_operations = partition_processing_context.nmx_c_operations;

        if !nmx_c_operations.is_empty() {
            tracing::debug!("NMX-C operations: {:?}", nmx_c_operations);
        }

        // Execute any NMX-C operations and collect successful completions.
        let completed_nmx_c_operations = self
            .execute_nmx_c_operations(nmx_c_operations, metrics)
            .await?;

        if !completed_nmx_c_operations.is_empty() {
            tracing::debug!(
                "Completed NMX-C operations: {:?}",
                completed_nmx_c_operations
            );
        }

        let num_completed_operations = completed_nmx_c_operations
            .values()
            .map(|ops| ops.len())
            .sum::<usize>();

        // Get a fresh list of partitions from NMX-C.
        let partition_info_list = nmxc_client
            .get_partition_info_list(GetPartitionInfoListRequest {
                context: Some(libnmxc::nmxc_model::Context {
                    context: String::new(),
                }),
                partition_id_list: vec![],
                partition_name_list: vec![],
                gateway_id: NMX_C_GATEWAY_ID.into(),
            })
            .await
            .map_err(|e| {
                metrics.nmxm.connect_error =
                    "Failed to get NMX-C partition info list when updating db".to_string();
                CarbideError::internal(format!("Failed to get NMX-C partition info list: {e}"))
            })?
            .partition_info_list;
        let nmx_c_partitions: HashMap<String, PartitionInfo> = partition_info_list
            .into_iter()
            .map(|p| (nmx_c_partition_id_string(&p), p))
            .collect();

        // Update db with the operations that completed successfully.
        let mut txn = self.db_pool.txn_begin().await?;
        self.update_db_with_nmx_c_operations(
            &mut txn,
            completed_nmx_c_operations,
            &db_nvl_logical_partitions,
            &nmx_c_partitions,
        )
        .await?;
        txn.commit().await?;

        Ok(num_completed_operations)
    }

    // Check the passed NvLink partition "observations" (physical partition info from NMX-C supplemented by physical and logical partition info from DB)
    // against the instance config and generate NMX-C operations to bring the observations into alignment with the config.
    fn check_nv_link_partitions(
        &self,
        partition_ctx: &mut PartitionProcessingContext,
        mh_snapshots: HashMap<MachineId, ManagedHostStateSnapshot>,
        metrics: &mut NvlPartitionMonitorMetrics,
    ) -> CarbideResult<HashMap<MachineId, MachineNvLinkStatusObservation>> {
        let mut machine_gpu_statuses = HashMap::new();

        for mh in mh_snapshots.values() {
            metrics.num_machines_scanned += 1;
            let Some(instance) = &mh.instance else {
                // For machines with no instance, check if machine is in admin network and any cleanup is required
                let _ = self.check_machine_and_handle_gpu_removals(mh, partition_ctx);
                continue;
            };
            metrics.num_instances_scanned += 1;
            let mut instance_gpu_statuses = Vec::new();
            let Some(info) = partition_ctx
                .machine_nvlink_info
                .get(&instance.machine_id)
                .cloned()
            else {
                tracing::warn!("No nvlink_info found for machine {}", instance.machine_id);
                machine_gpu_statuses.insert(
                    instance.machine_id,
                    MachineNvLinkStatusObservation {
                        observed_at: Utc::now(),
                        nvlink_gpus: instance_gpu_statuses,
                    },
                );
                continue;
            };
            match info {
                Some(info) => {
                    for nvlink_gpu in &info.gpus {
                        metrics.num_gpus_scanned += 1;
                        // NMX-M ID is 1-based so subtract 1 to get our device_instance.
                        let device_instance: u32 = nvlink_gpu.device_id as u32 - 1;
                        let instance_gpu_config = &instance
                            .config
                            .nvlink
                            .gpu_configs
                            .iter()
                            .find(|gpu| gpu.device_instance == device_instance);
                        let mut gpu_status_observation = MachineNvLinkGpuStatusObservation {
                            device_instance,
                            domain_id: info.domain_uuid,
                            gpu_id: nvlink_gpu.guid.to_string(),
                            guid: nvlink_gpu.guid,
                            ..Default::default()
                        };
                        let mut gpu_ctx = GpuProcessingContext {
                            gpu_guid: nvlink_gpu.guid,
                            domain_uuid: info.domain_uuid,
                            ..Default::default()
                        };

                        let nmxc_partition = partition_ctx
                            .gpu_to_partition_map
                            .get(&nvlink_gpu.guid)
                            .cloned();

                        // Decide on what action the monitor will take with this GPU, and finish building the gpu_ctx.
                        let gpu_action: GpuAction;
                        if let Some(nmxc_partition) = nmxc_partition {
                            let partition_id = nmxc_partition
                                .partition_id
                                .map(|id| id.partition_id)
                                .unwrap_or_default();
                            match partition_ctx.get_db_partition_info(partition_id) {
                                Some((
                                    db_partition_id,
                                    db_logical_partition_id,
                                    db_partition_name,
                                    db_partition_nmx_c_id,
                                )) => {
                                    if let Some(gpu_config) = instance_gpu_config {
                                        gpu_ctx.logical_partition_id =
                                            gpu_config.logical_partition_id;
                                        if db_logical_partition_id.is_none() {
                                            // How can this happen?
                                            tracing::error!(
                                                "No logical partition ID associated with physical partition {:?}",
                                                partition_id.to_string()
                                            );
                                            continue;
                                        } else if gpu_config.logical_partition_id
                                            != db_logical_partition_id
                                        {
                                            // This covers both the case where the tenant has asked for the GPU to be removed from the partition
                                            // (i.e. gpu_config.logical_partition_id is None), and the case where the GPU is in logical partition
                                            // A and the tenant wants it to be in logical partition B. In the latter case, we need to remove the GPU
                                            // from the current partition before adding it to the new one.
                                            gpu_action = GpuAction::RemoveFromPartition;
                                        } else {
                                            gpu_action = GpuAction::NoOp;
                                        }
                                    } else {
                                        // There is no gpu config, which means the tenant does not want it to be part of a partition.
                                        gpu_action = GpuAction::RemoveFromPartition;
                                    }
                                    gpu_ctx.logical_partition_id = db_logical_partition_id;
                                    gpu_ctx.partition_id = db_partition_id;
                                    gpu_ctx.partition_name = db_partition_name;
                                    gpu_ctx.partition_nmx_c_id = db_partition_nmx_c_id;

                                    // Update the observation.
                                    gpu_status_observation.logical_partition_id =
                                        db_logical_partition_id;
                                    gpu_status_observation.partition_id = db_partition_id;
                                }
                                None => {
                                    // TODO: should we add the partition NMX-C ID to the status obs?
                                    if is_nmx_c_default_partition(&nmxc_partition) {
                                        if instance_gpu_config.is_some() {
                                            tracing::info!(
                                                "Removing GPU {} in machine {} and instance {} from default partition {}",
                                                nvlink_gpu.guid,
                                                instance.machine_id,
                                                instance.id,
                                                partition_id
                                            );
                                            gpu_action = GpuAction::RemoveFromUnknownPartition;
                                            gpu_ctx.partition_nmx_c_id =
                                                nmxc_partition.partition_id.unwrap_or_default();
                                        } else {
                                            // Do nothing if there is no config
                                            gpu_action = GpuAction::NoOp;
                                        }
                                    } else {
                                        // Monitor does not know about this partition, so just remove the GPU. On the next iteration
                                        // the monitor will put the GPU in the correct partition (or leave it if the config says no partition)
                                        tracing::warn!(
                                            "Removing GPU {} from unknown partition with NMX-C ID {}",
                                            nvlink_gpu.guid,
                                            partition_id
                                        );
                                        gpu_action = GpuAction::RemoveFromUnknownPartition;
                                        gpu_ctx.partition_nmx_c_id =
                                            libnmxc::nmxc_model::PartitionId { partition_id };
                                    }
                                }
                            }
                        } else {
                            // This GPU isn't in a partition yet.
                            if let Some(gpu_config) = instance_gpu_config
                                && let Some(logical_partition_id) = gpu_config.logical_partition_id
                            {
                                // Tenant has asked to put it in a partition
                                gpu_action = GpuAction::AddToPartition;
                                gpu_ctx.logical_partition_id = Some(logical_partition_id);
                            } else {
                                gpu_action = GpuAction::NoOp;
                            }
                        }

                        instance_gpu_statuses.push(gpu_status_observation);

                        if let Some(logical_partition_id) = gpu_ctx.logical_partition_id
                            && !partition_ctx.validate_logical_partition(&logical_partition_id)
                        {
                            tracing::warn!(
                                machine_id = %instance.machine_id,
                                gpu_guid = %gpu_ctx.gpu_guid,
                                logical_partition_id = %logical_partition_id,
                                "Logical partition is marked as deleted, skipping GPU action"
                            );
                            continue;
                        }

                        match gpu_action {
                            GpuAction::AddToPartition => {
                                // Check if there are other physical partitions in the logical partition
                                if let Some(partition) = partition_ctx
                                    .db_nvl_partitions
                                    .values()
                                    .find(|p| {
                                        p.logical_partition_id == gpu_ctx.logical_partition_id
                                            && p.domain_uuid == info.domain_uuid
                                    })
                                    .cloned()
                                {
                                    // Add to existing partition in the same domain
                                    if let Err(e) = partition_ctx
                                        .handle_gpu_addition_existing_partition(
                                            &gpu_ctx, &partition,
                                        )
                                    {
                                        tracing::error!(
                                            gpu_nmx_m_id = %gpu_ctx.gpu_guid,
                                            machine_id = %instance.machine_id,
                                            "Failed to handle GPU addition to existing partition: {e}"
                                        );
                                    }
                                } else {
                                    // Create new partition in a different domain
                                    if let Err(e) =
                                        partition_ctx.handle_gpu_addition_new_partition(&gpu_ctx)
                                    {
                                        tracing::error!(
                                            gpu_nmx_m_id = %gpu_ctx.gpu_guid,
                                            machine_id = %instance.machine_id,
                                            "Failed to handle GPU addition to new partition: {e}"
                                        );
                                    }
                                }
                            }
                            GpuAction::RemoveFromPartition => {
                                let Some(gpus_to_keep) = partition_ctx
                                    .get_gpus_to_keep_after_removal(
                                        gpu_ctx.logical_partition_id,
                                        &gpu_ctx.partition_nmx_c_id,
                                        gpu_ctx.gpu_guid,
                                        &instance.machine_id,
                                        device_instance,
                                    )
                                else {
                                    continue;
                                };

                                if let Err(e) =
                                    partition_ctx.handle_gpu_removal(&gpu_ctx, gpus_to_keep)
                                {
                                    tracing::error!(
                                        gpu_guid = %gpu_ctx.gpu_guid,
                                        machine_id = %instance.machine_id,
                                        "Failed to handle GPU removal from partition: {e}"
                                    );
                                }
                            }
                            GpuAction::RemoveFromUnknownPartition => {
                                if let Some(gpus_to_keep) = partition_ctx
                                    .get_gpus_to_keep_in_unknown_partition_after_removal(
                                        &gpu_ctx.partition_nmx_c_id,
                                        gpu_ctx.gpu_guid,
                                        &instance.machine_id,
                                        device_instance,
                                    )
                                {
                                    if let Err(e) = partition_ctx
                                        .handle_gpu_removal_from_unknown_partition(
                                            &gpu_ctx.partition_nmx_c_id,
                                            gpu_ctx.gpu_guid,
                                            gpus_to_keep,
                                        )
                                    {
                                        tracing::error!(
                                            gpu_guid = %gpu_ctx.gpu_guid,
                                            machine_id = %instance.machine_id,
                                            "Failed to handle GPU removal from unknown partition: {e}"
                                        );
                                    }
                                } else {
                                    tracing::error!(
                                        gpu_guid = %gpu_ctx.gpu_guid,
                                        machine_id = %instance.machine_id,
                                        "No default partition found with NMX-C ID = {}",
                                        gpu_ctx.partition_nmx_c_id.partition_id
                                    );
                                    continue;
                                }
                            }
                            GpuAction::NoOp => (),
                        }
                    }
                }
                None => {
                    tracing::warn!("No nvlink_info found for machine {}", instance.machine_id);
                }
            }
            // Now we've generated the operations, record an observation.
            let observation = MachineNvLinkStatusObservation {
                observed_at: Utc::now(),
                nvlink_gpus: instance_gpu_statuses,
            };
            machine_gpu_statuses.insert(instance.machine_id, observation);
        }

        self.record_nvlink_config_pending_durations(&mh_snapshots, &machine_gpu_statuses, metrics);

        metrics.num_machine_nvl_status_updates = machine_gpu_statuses.len();

        // Add all default partition removals to the normal list so they get executed.
        for (_partition_nmx_c_id, operations) in
            partition_ctx.unknown_partition_removal_operations.iter()
        {
            for operation in operations {
                partition_ctx
                    .nmx_c_operations
                    .entry(NvLinkLogicalPartitionId::default())
                    .and_modify(|ops| {
                        ops.push(operation.clone());
                    })
                    .or_insert(vec![operation.clone()]);
            }
        }
        for (_partition_nmx_c_id, operation) in
            partition_ctx.unknown_partition_addition_operations.iter()
        {
            partition_ctx
                .nmx_c_operations
                .entry(NvLinkLogicalPartitionId::default())
                .and_modify(|ops| {
                    ops.push(operation.clone());
                })
                .or_insert(vec![operation.clone()]);
        }
        Ok(machine_gpu_statuses)
    }

    /// Records time from nvlink_config_version for instances currently in Pending (time spent in Pending).
    fn record_nvlink_config_pending_durations(
        &self,
        mh_snapshots: &HashMap<MachineId, ManagedHostStateSnapshot>,
        machine_gpu_statuses: &HashMap<MachineId, MachineNvLinkStatusObservation>,
        metrics: &mut NvlPartitionMonitorMetrics,
    ) {
        for (machine_id, observation) in machine_gpu_statuses {
            let Some(mh) = mh_snapshots.get(machine_id) else {
                continue;
            };
            let Some(instance) = &mh.instance else {
                continue;
            };
            if instance.config.nvlink.gpu_configs.is_empty() {
                continue;
            }
            let nvlink_status = InstanceNvLinkStatus::from_config_and_observation(
                Versioned::new(&instance.config.nvlink, instance.nvlink_config_version),
                Some(observation),
            );
            if nvlink_status.configs_synced == SyncState::Pending {
                let duration_ms = (Utc::now() - instance.nvlink_config_version.timestamp())
                    .num_milliseconds()
                    .max(0) as f64;
                metrics.nvlink_config_apply_durations_ms.push(duration_ms);
            }
        }
    }

    // Managed hosts that are no longer an instance should not have GPUs in any logical partitions.
    // GPUs should be removed from the tenant partition and move to the default partition.
    pub fn check_machine_and_handle_gpu_removals(
        &self,
        mh: &ManagedHostStateSnapshot,
        partition_ctx: &mut PartitionProcessingContext,
    ) -> CarbideResult<()> {
        // If not in admin-network mode, skip processing. GPUs should stay
        // attached to tenant partitions, but zero-DPU hosts are always
        // considered admin network (since they don't have a DPU to put them
        // in an overlay network). In other words, zero-DPU hosts get GPU
        // removals, but hosts with DPUs in tenant networks don't.
        if !mh.use_admin_network() {
            return Ok(());
        }

        if let Some(nvlink_info) = &mh.host_snapshot.nvlink_info {
            for gpu in &nvlink_info.gpus {
                let nmxc_partition = match partition_ctx.gpu_to_partition_map.get(&gpu.guid) {
                    // GPU is in a partition, so we need to remove it from the partition.
                    Some(p) => p,
                    None => {
                        // GPU has been removed from  the partition it was previously in, now add it to the default partition.
                        let Some(default_nmxc) = partition_ctx
                            .nmx_c_partitions
                            .values()
                            .find(|p| is_nmx_c_default_partition(p))
                        else {
                            tracing::warn!(
                                machine_id = %mh.host_snapshot.id,
                                gpu_guid = %gpu.guid,
                                "GPU not in any NMX-C partition and no default partition found; skipping"
                            );
                            continue;
                        };
                        let Some(partition_id_struct) = default_nmxc.partition_id.clone() else {
                            tracing::warn!(
                                machine_id = %mh.host_snapshot.id,
                                gpu_guid = %gpu.guid,
                                "Default NMX-C partition has no partition_id; skipping"
                            );
                            continue;
                        };
                        let nmx_c_id = partition_id_struct.partition_id;

                        let gpus_in_partition = default_nmxc.gpu_uid_list.clone();

                        tracing::info!(
                            machine_id = %mh.host_snapshot.id,
                            gpu_guid = %gpu.guid,
                            nmx_c_id,
                            "GPU not in any partition; enqueueing add to default partition"
                        );
                        partition_ctx.handle_gpu_addition_to_unknown_partition(
                            &partition_id_struct,
                            gpu.guid,
                            gpus_in_partition,
                        )?;
                        continue;
                    }
                };

                let partition_id = nmxc_partition
                    .partition_id
                    .map(|id| id.partition_id)
                    .unwrap_or_default();
                let Some((
                    db_partition_id,
                    db_logical_partition_id,
                    db_partition_name,
                    db_partition_nmx_c_id,
                )) = partition_ctx.get_db_partition_info(partition_id)
                else {
                    if !is_nmx_c_default_partition(nmxc_partition) {
                        tracing::error!(
                            "No partition found with nmx_m_id = {} while processing removal of GPU {gpu:?} in admin network",
                            partition_id,
                        );
                    }
                    continue;
                };

                let gpu_ctx = GpuProcessingContext {
                    gpu_guid: gpu.guid,
                    domain_uuid: nvlink_info.domain_uuid,
                    partition_id: db_partition_id,
                    partition_name: db_partition_name.clone(),
                    logical_partition_id: db_logical_partition_id,
                    partition_nmx_c_id: db_partition_nmx_c_id,
                };

                let Some(gpus_to_keep) = partition_ctx.get_gpus_to_keep_after_removal(
                    db_logical_partition_id,
                    &gpu_ctx.partition_nmx_c_id,
                    gpu.guid,
                    &mh.host_snapshot.id,
                    gpu.device_id.try_into().unwrap(),
                ) else {
                    continue;
                };

                let logical_id = db_logical_partition_id.unwrap_or_default();
                tracing::info!(
                    machine_id = %mh.host_snapshot.id,
                    gpu_guid = %gpu.guid,
                    logical_partition_id = %logical_id,
                    gpus_to_keep = ?gpus_to_keep,
                    "Handling GPU removal for machine in admin network"
                );
                partition_ctx.handle_gpu_removal(&gpu_ctx, gpus_to_keep)?;
            }
        }
        Ok(())
    }

    // Use a separate transaction to record the observations to avoid blocking the main transaction when we poll NMX-M.
    async fn record_nvlink_status_observation(
        &self,
        observations: HashMap<MachineId, MachineNvLinkStatusObservation>,
    ) -> CarbideResult<()> {
        let mut obs_txn = self.db_pool.begin().await.map_err(|e| {
            CarbideError::internal(format!(
                "Failed to create transaction for nvlink status observation: {e}"
            ))
        })?;
        for (machine_id, observations) in observations {
            db::machine::update_nvlink_status_observation(&mut obs_txn, &machine_id, &observations)
                .await?;
        }
        obs_txn.commit().await.map_err(|e| {
            CarbideError::internal(format!(
                "Failed to commit transaction for nvlink status observation: {e}"
            ))
        })?;
        Ok(())
    }

    async fn execute_nmx_c_operations(
        &self,
        nmx_c_operations: HashMap<NvLinkLogicalPartitionId, Vec<NmxcPartitionOperation>>,
        metrics: &mut NvlPartitionMonitorMetrics,
    ) -> CarbideResult<HashMap<NvLinkLogicalPartitionId, Vec<NmxcPartitionOperation>>> {
        let nmxc_client = self
            .nmxc_client_pool
            .create_client(Endpoint::new(self.config.nmx_c_endpoint.clone()))
            .await
            .map_err(|e| CarbideError::internal(format!("Failed to create NMXC client: {e}")))?;

        let mut completed_operations: HashMap<
            NvLinkLogicalPartitionId,
            Vec<NmxcPartitionOperation>,
        > = HashMap::new();

        for (logical_partition_id, operations) in nmx_c_operations {
            for operation in operations {
                let start_time = std::time::Instant::now();
                let success = match &operation.operation_type {
                    NmxcPartitionOperationType::Create => {
                        let name = format!(
                            "{}{}",
                            logical_partition_id,
                            operation
                                .gpu_uids
                                .iter()
                                .map(|u| u.to_string())
                                .collect::<Vec<_>>()
                                .join(",")
                        );
                        let name: String = name.chars().take(240).collect();
                        let request = libnmxc::nmxc_model::CreatePartitionRequest {
                            context: None,
                            name,
                            gpu_resource_id: operation
                                .gpu_uids
                                .iter()
                                .map(|&uid| libnmxc::nmxc_model::GpuResourceId {
                                    resource_id: Some(
                                        libnmxc::nmxc_model::gpu_resource_id::ResourceId::GpuUid(
                                            uid,
                                        ),
                                    ),
                                })
                                .collect(),
                            attr: None,
                            partition_id: None,
                            gateway_id: NMX_C_GATEWAY_ID.into(),
                        };
                        match nmxc_client.create_partition(request).await {
                            Ok(_) => true,
                            Err(e) => {
                                tracing::warn!(
                                    %logical_partition_id,
                                    "Failed to issue create partition to NMX-C, continuing with other operations: {e}"
                                );
                                false
                            }
                        }
                    }
                    NmxcPartitionOperationType::Remove(nmx_c_partition_id) => {
                        let request = libnmxc::nmxc_model::DeletePartitionRequest {
                            context: None,
                            partition_id: Some(libnmxc::nmxc_model::PartitionId {
                                partition_id: *nmx_c_partition_id,
                            }),
                            gateway_id: NMX_C_GATEWAY_ID.into(),
                            name: String::new(),
                        };
                        match nmxc_client.delete_partition(request).await {
                            Ok(_) => true,
                            Err(e) => {
                                tracing::warn!(
                                    %logical_partition_id,
                                    %nmx_c_partition_id,
                                    "Failed to issue delete partition to NMX-C, continuing with other operations: {e}"
                                );
                                false
                            }
                        }
                    }
                    NmxcPartitionOperationType::RemoveUnknownPartition(nmx_c_partition_id) => {
                        let request = libnmxc::nmxc_model::DeletePartitionRequest {
                            context: None,
                            partition_id: Some(libnmxc::nmxc_model::PartitionId {
                                partition_id: *nmx_c_partition_id,
                            }),
                            gateway_id: NMX_C_GATEWAY_ID.into(),
                            name: String::new(),
                        };
                        match nmxc_client.delete_partition(request).await {
                            Ok(_) => true,
                            Err(e) => {
                                return Err(CarbideError::internal(format!(
                                    "Failed to delete default partition: {e}"
                                )));
                            }
                        }
                    }
                    NmxcPartitionOperationType::Update(nmx_c_partition_id) => {
                        let pid = libnmxc::nmxc_model::PartitionId {
                            partition_id: *nmx_c_partition_id,
                        };
                        let list_req = libnmxc::nmxc_model::GetPartitionInfoListRequest {
                            context: None,
                            partition_id_list: vec![pid],
                            partition_name_list: vec![],
                            gateway_id: NMX_C_GATEWAY_ID.into(),
                        };
                        match nmxc_client.get_partition_info_list(list_req).await {
                            Err(e) => {
                                tracing::warn!(
                                    %logical_partition_id,
                                    %nmx_c_partition_id,
                                    "Failed to get partition info from NMX-C before update: {e}"
                                );
                                false
                            }
                            Ok(resp) => {
                                let current_uids = resp
                                    .partition_info_list
                                    .into_iter()
                                    .find(|info| {
                                        info.partition_id
                                            .as_ref()
                                            .map(|id| id.partition_id == *nmx_c_partition_id)
                                            .unwrap_or(false)
                                    })
                                    .map(|info| info.gpu_uid_list)
                                    .unwrap_or_default();

                                let desired: HashSet<u64> =
                                    operation.gpu_uids.iter().copied().collect();
                                let current: HashSet<u64> = current_uids.iter().copied().collect();
                                let to_remove: Vec<u64> =
                                    current.difference(&desired).copied().collect();
                                let to_add: Vec<u64> =
                                    desired.difference(&current).copied().collect();

                                let mut ok = true;
                                if !to_remove.is_empty() {
                                    let req = libnmxc::nmxc_model::UpdatePartitionRequest {
                                        context: None,
                                        partition_id: Some(pid),
                                        location_list: vec![],
                                        gpu_uid: to_remove,
                                        gateway_id: NMX_C_GATEWAY_ID.into(),
                                        name: String::new(),
                                        reroute: true,
                                    };
                                    if let Err(e) =
                                        nmxc_client.remove_gpus_from_partition(req).await
                                    {
                                        tracing::warn!(
                                            %logical_partition_id,
                                            %nmx_c_partition_id,
                                            "Failed to remove GPUs from partition on NMX-C: {e}"
                                        );
                                        ok = false;
                                    }
                                }
                                if ok && !to_add.is_empty() {
                                    let req = libnmxc::nmxc_model::UpdatePartitionRequest {
                                        context: None,
                                        partition_id: Some(pid),
                                        location_list: vec![],
                                        gpu_uid: to_add,
                                        gateway_id: NMX_C_GATEWAY_ID.into(),
                                        name: String::new(),
                                        reroute: true,
                                    };
                                    if let Err(e) = nmxc_client.add_gpus_to_partition(req).await {
                                        tracing::warn!(
                                            %logical_partition_id,
                                            %nmx_c_partition_id,
                                            "Failed to add GPUs to partition on NMX-C: {e}"
                                        );
                                        ok = false;
                                    }
                                }
                                ok
                            }
                        }
                    }
                };
                let applied_change = AppliedChange {
                    operation: operation.operation_type.clone().into(),
                    status: if success {
                        NmxmPartitionOperationStatus::Completed
                    } else {
                        NmxmPartitionOperationStatus::Failed
                    },
                };
                *metrics
                    .applied_changes
                    .entry(applied_change.clone())
                    .or_default() += 1;
                metrics
                    .operation_latencies
                    .entry(applied_change)
                    .or_default()
                    .push(start_time.elapsed());
                if success {
                    completed_operations
                        .entry(logical_partition_id)
                        .or_default()
                        .push(operation);
                }
            }
        }
        Ok(completed_operations)
    }

    async fn update_db_with_nmx_c_operations(
        &self,
        txn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        completed_nmx_c_operations: HashMap<NvLinkLogicalPartitionId, Vec<NmxcPartitionOperation>>,
        db_nvl_logical_partitions: &[LogicalPartition],
        nmx_c_partitions: &HashMap<String, PartitionInfo>,
    ) -> CarbideResult<()> {
        for (logical_partition_id, operations) in completed_nmx_c_operations {
            for operation in operations {
                match operation.operation_type {
                    NmxcPartitionOperationType::Create => {
                        // Create the nvl partition in the database
                        let new_partition = model::nvl_partition::NewNvlPartition {
                            id: NvLinkPartitionId::new(),
                            logical_partition_id,
                            name: NvlPartitionName::try_from(operation.name.clone())?,
                            domain_uuid: operation.domain_uuid.unwrap_or_default(),
                            nmx_m_id: match nmx_c_partitions.values().find(|p| {
                                let p_uids: HashSet<u64> = p.gpu_uid_list.iter().copied().collect();
                                let op_uids: HashSet<u64> =
                                    operation.gpu_uids.iter().copied().collect();
                                p_uids == op_uids
                            }) {
                                Some(p) => nmx_c_partition_id_string(p),
                                None => {
                                    tracing::error!(
                                        "NMX-C partition not found for name {}",
                                        operation.name
                                    );
                                    continue;
                                }
                            },
                        };
                        let _partition = db::nvl_partition::create(&new_partition, txn).await?;
                    }
                    NmxcPartitionOperationType::Remove(_) => {
                        db::nvl_partition::final_delete(
                            operation.db_partition_id.unwrap_or_default(),
                            txn,
                        )
                        .await?;
                    }
                    NmxcPartitionOperationType::Update(_) => {
                        // No-op, since partition membership is not tracked in the partitions table. The status observation of the
                        // added/removed GPUs will be updated.
                    }
                    NmxcPartitionOperationType::RemoveUnknownPartition(_) => {
                        // No-op, since default partition membership is not tracked in the partitions table. The status observation of the
                        // added/removed GPUs will be updated.
                    }
                }
            }
        }

        // walk the logical partition list and check if any logical partitions need to be cleaned up
        for lp in db_nvl_logical_partitions {
            if model::nvl_logical_partition::is_marked_as_deleted(lp) {
                tracing::info!(logical_partition_id = %lp.id, "Deleting logical partition");
                db::nvl_logical_partition::final_delete(lp.id, txn).await?;
            }
        }

        Ok(())
    }

    async fn load_mnnvl_managed_host_snapshots(
        &self,
        txn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> CarbideResult<HashMap<MachineId, ManagedHostStateSnapshot>> {
        let mnvvl_machine_ids = find_machine_ids(
            txn.as_mut(),
            MachineSearchConfig {
                mnnvl_only: true,
                include_predicted_host: true,
                ..Default::default()
            },
        )
        .await?;
        load_by_machine_ids(
            txn.as_mut(),
            mnvvl_machine_ids.as_slice(),
            LoadSnapshotOptions {
                include_history: false,
                include_instance_data: true,
                host_health_config: self.host_health,
            },
        )
        .await
        .map_err(CarbideError::from)
    }
}

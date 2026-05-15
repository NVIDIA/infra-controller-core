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

use ::rpc::errors::RpcDataConversionError;
use carbide_uuid::machine::MachineId;
use config_version::Versioned;

use crate::instance::config::InstanceConfig;
use crate::instance::config::extension_services::InstanceExtensionServicesConfig;
use crate::instance::config::infiniband::InstanceInfinibandConfig;
use crate::instance::config::network::InstanceNetworkConfig;
use crate::instance::config::nvlink::InstanceNvLinkConfig;
use crate::instance::status::{InstanceStatus, InstanceStatusObservations, SyncState};
use crate::machine::infiniband::MachineInfinibandStatusObservation;
use crate::machine::nvlink::MachineNvLinkStatusObservation;
use crate::machine::{ManagedHostState, ReprovisionRequest};
use crate::rpc_conv::instance::status::tenant::instance_status_tenant_state;

pub mod extension_service;
pub mod infiniband;
pub mod network;
pub mod nvlink;
pub mod tenant;

impl TryFrom<InstanceStatus> for rpc::InstanceStatus {
    type Error = RpcDataConversionError;

    fn try_from(status: InstanceStatus) -> Result<Self, Self::Error> {
        Ok(rpc::InstanceStatus {
            tenant: status.tenant.map(|status| status.try_into()).transpose()?,
            network: Some(status.network.try_into()?),
            infiniband: Some(status.infiniband.try_into()?),
            dpu_extension_services: Some(status.extension_services.try_into()?),
            nvlink: Some(status.nvlink.try_into()?),
            configs_synced: rpc::SyncState::try_from(status.configs_synced)? as i32,
            update: status.reprovision_request.map(|request| request.into()),
        })
    }
}

/// Derives an Instances network status from the users desired config
/// and status that we observed from the networking subsystem.
///
/// This mechanism guarantees that the status we return to the user always
/// matches the latest `Config` set by the user. We can not directly
/// forwarding the last observed status without taking `Config` into account,
/// because the observation might have been related to a different config,
/// and the interfaces therefore won't match.
#[allow(clippy::too_many_arguments)]
pub fn instance_status_from_config_and_observation(
    dpu_id_to_device_map: HashMap<String, Vec<MachineId>>,
    instance_config: Versioned<&InstanceConfig>,
    network_config: Versioned<&InstanceNetworkConfig>,
    ib_config: Versioned<&InstanceInfinibandConfig>,
    extension_services_config: Versioned<&InstanceExtensionServicesConfig>,
    nvlink_config: Versioned<&InstanceNvLinkConfig>,
    observations: &InstanceStatusObservations,
    machine_state: ManagedHostState,
    delete_requested: bool,
    reprovision_request: Option<ReprovisionRequest>,
    ib_status: Option<&MachineInfinibandStatusObservation>,
    nvlink_status: Option<&MachineNvLinkStatusObservation>,
    is_network_config_request_pending: bool,
) -> Result<InstanceStatus, RpcDataConversionError> {
    let mut instance_config_synced = SyncState::Synced;

    for network_obs in observations.network.values() {
        if let Some(version_obs) = network_obs.instance_config_version
            && instance_config.version != version_obs
        {
            instance_config_synced = SyncState::Pending;
            break;
        }
        // TODO(bcavanagh): Switch to SyncState::Pending or
        //                  return Err(RpcDataConversionError::InvalidConfigVersion)
        //                  after all dpu-agents have been updated to support/report the field.
        // If observations.network.instance_config_version was None, then "ignore"
    }

    let network =
        crate::instance::status::network::InstanceNetworkStatus::from_config_and_observations(
            dpu_id_to_device_map.clone(),
            network_config,
            &observations.network,
            is_network_config_request_pending,
        );

    let infiniband =
        crate::instance::status::infiniband::InstanceInfinibandStatus::from_config_and_observation(
            ib_config, ib_status,
        );

    let extension_services =
        crate::instance::status::extension_service::InstanceExtensionServicesStatus::from_config_and_observations(
            &dpu_id_to_device_map,
            extension_services_config,
            &observations.extension_services,
        );
    let extension_services_ready =
        crate::instance::status::extension_service::is_extension_services_ready(
            &extension_services,
        );
    let nvlink = crate::instance::status::nvlink::InstanceNvLinkStatus::from_config_and_observation(
        nvlink_config,
        nvlink_status,
    );

    let phone_home_last_contact = observations.phone_home_last_contact;

    // If additional configs are added, they need to be incorporated here
    let configs_synced = match (
        network.configs_synced,
        infiniband.configs_synced,
        extension_services.configs_synced,
        nvlink.configs_synced,
        instance_config_synced,
    ) {
        (
            SyncState::Synced,
            SyncState::Synced,
            SyncState::Synced,
            SyncState::Synced,
            SyncState::Synced,
        ) => SyncState::Synced,
        _ => SyncState::Pending,
    };

    let tenant = crate::instance::status::tenant::InstanceTenantStatus {
        state: match delete_requested {
            false => instance_status_tenant_state(
                machine_state,
                configs_synced,
                instance_config.os.phone_home_enabled,
                phone_home_last_contact,
                extension_services_ready,
            )?,
            true => {
                // If instance deletion was requested, we always confirm the
                // tenant that the instance is actually in progress of shutting down.
                // The instance might however still first need to run through
                // various provisioning steps to become "ready" before starting
                // to terminate
                crate::instance::status::tenant::TenantState::Terminating
            }
        },
        state_details: String::new(),
    };

    Ok(InstanceStatus {
        tenant: Some(tenant),
        network,
        infiniband,
        extension_services,
        nvlink,
        configs_synced,
        reprovision_request,
    })
}

impl TryFrom<SyncState> for rpc::SyncState {
    type Error = RpcDataConversionError;

    fn try_from(state: SyncState) -> Result<Self, Self::Error> {
        Ok(match state {
            SyncState::Synced => rpc::SyncState::Synced,
            SyncState::Pending => rpc::SyncState::Pending,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::instance::status::tenant::TenantState;
    use crate::machine::{DpuReprovisionStates, ManagedHostState, ReprovisionState};

    #[test]
    fn test_tenant_state() {
        let machine_id: MachineId =
            MachineId::from_str("fm100htjtiaehv1n5vh67tbmqq4eabcjdng40f7jupsadbedhruh6rag1l0")
                .unwrap();

        assert_eq!(
            instance_status_tenant_state(
                ManagedHostState::DPUReprovision {
                    dpu_states: DpuReprovisionStates {
                        states: HashMap::from([(
                            machine_id,
                            ReprovisionState::WaitingForNetworkConfig,
                        )]),
                    },
                },
                SyncState::Synced,
                false,
                None,
                false,
            )
            .unwrap(),
            TenantState::Invalid
        );
    }
}

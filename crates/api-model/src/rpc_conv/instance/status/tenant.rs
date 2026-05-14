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

use ::rpc::errors::RpcDataConversionError;

use crate::instance::status::SyncState;
use crate::instance::status::tenant::{InstanceTenantStatus, TenantState};
use crate::machine::{InstanceState, ManagedHostState};

/// Tries to convert Machine state to tenant state.
pub fn instance_status_tenant_state(
    machine_state: ManagedHostState,
    configs_synced: SyncState,
    phone_home_enrolled: bool,
    phone_home_last_contact: Option<chrono::DateTime<chrono::Utc>>,
    extension_services_ready: bool,
) -> Result<TenantState, RpcDataConversionError> {
    // At this point, we are sure that instance is created.
    // If machine state is still ready, means state machine has not processed this instance
    // yet.

    let tenant_state = match machine_state {
        ManagedHostState::Ready => TenantState::Provisioning,
        ManagedHostState::Assigned { instance_state } => match instance_state {
            InstanceState::Init
            | InstanceState::WaitingForNetworkSegmentToBeReady
            | InstanceState::WaitingForNetworkConfig
            | InstanceState::WaitingForStorageConfig
            | InstanceState::WaitingForExtensionServicesConfig
            | InstanceState::WaitingForRebootToReady => TenantState::Provisioning,
            InstanceState::NetworkConfigUpdate { .. } => TenantState::Configuring,

            InstanceState::Ready => {
                let phone_home_pending = phone_home_enrolled && phone_home_last_contact.is_none();

                // TODO phone_home_last_contact window? e.g. must have been received in last 10 minutes
                match (phone_home_pending, configs_synced, extension_services_ready) {
                    // If there is no pending phone-home, but configs are
                    // not synced, configs must have changed after provisioning finished
                    // since we entered Ready state.
                    (false, SyncState::Pending, _) => TenantState::Configuring,

                    // If there is no pending phone-home, but extension services are not ready,
                    // then extension services must have changed after provisioning finished
                    // since we entered Ready state.
                    (false, _, false) => TenantState::Configuring,

                    // If there is no pending phone-home and extension services are ready,
                    // return Ready (this was the default before phone_home)
                    (false, SyncState::Synced, true) => TenantState::Ready,

                    // If there is a pending phone-home, we're still
                    // provisioning.
                    (true, _, _) => TenantState::Provisioning,
                }
            }
            // If termination had been requested (i.e., if the `deleted` column
            // of the instance record in the DB is non-null), then things would
            // have short-circuited to Terminating before ever even getting to
            // this tenant_state function.
            InstanceState::SwitchToAdminNetwork | InstanceState::WaitingForNetworkReconfig => {
                TenantState::Terminating
            }
            // When tenants request a custom pxe reboot, the managed hosts
            // will go through HostPlatformConfiguration and WaitingForDpusToUp
            // before going back to Ready
            InstanceState::WaitingForDpusToUp | InstanceState::HostPlatformConfiguration { .. } => {
                TenantState::Configuring
            }
            InstanceState::BootingWithDiscoveryImage { .. }
            | InstanceState::DPUReprovision { .. }
            | InstanceState::HostReprovision { .. } => TenantState::Updating,
            InstanceState::DpaProvisioning => TenantState::Updating,
            InstanceState::WaitingForDpaToBeReady => TenantState::Updating,
            InstanceState::Failed { .. } => TenantState::Failed,
        },
        ManagedHostState::ForceDeletion => TenantState::Terminating,
        _ => {
            tracing::error!(%machine_state, "Invalid state during state handling");
            TenantState::Invalid
        }
    };

    Ok(tenant_state)
}

impl TryFrom<InstanceTenantStatus> for rpc::InstanceTenantStatus {
    type Error = RpcDataConversionError;

    fn try_from(state: InstanceTenantStatus) -> Result<Self, Self::Error> {
        Ok(rpc::InstanceTenantStatus {
            state: rpc::TenantState::try_from(state.state)? as i32,
            state_details: state.state_details,
        })
    }
}

impl TryFrom<TenantState> for rpc::TenantState {
    type Error = RpcDataConversionError;

    fn try_from(state: TenantState) -> Result<Self, Self::Error> {
        Ok(match state {
            TenantState::Provisioning => rpc::TenantState::Provisioning,
            TenantState::DpuReprovisioning => rpc::TenantState::DpuReprovisioning,
            TenantState::Ready => rpc::TenantState::Ready,
            TenantState::Configuring => rpc::TenantState::Configuring,
            TenantState::Terminating => rpc::TenantState::Terminating,
            TenantState::Terminated => rpc::TenantState::Terminated,
            TenantState::Failed => rpc::TenantState::Failed,
            TenantState::HostReprovisioning => rpc::TenantState::HostReprovisioning,
            TenantState::Updating => rpc::TenantState::Updating,
            TenantState::Invalid => rpc::TenantState::Invalid,
        })
    }
}

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
use std::net::IpAddr;

use ::rpc::common::SystemPowerControl;
use ::rpc::forge::{self as rpc};
use carbide_uuid::power_shelf::PowerShelfId;
use carbide_uuid::switch::SwitchId;
use component_manager::component_manager::ComponentManager;
use component_manager::compute_tray_manager::{ComputeTrayEndpoint, ComputeTrayVendor};
use component_manager::error::ComponentManagerError;
use component_manager::nv_switch_manager::SwitchEndpoint;
use component_manager::power_shelf_manager::{PowerShelfEndpoint, PowerShelfVendor};
use db::{self, WithTransaction};
use forge_secrets::credentials::{
    BmcCredentialType, CredentialKey, CredentialManager, Credentials,
};
use futures_util::FutureExt;
use mac_address::MacAddress;
use model::component_manager::{
    ComputeTrayComponent as ModelComputeTrayComponent, NvSwitchComponent, PowerAction,
    PowerShelfComponent,
};
use model::firmware::FirmwareComponentType;
use model::machine::machine_search_config::MachineSearchConfig;
use model::rack::{FirmwareUpgradeJob, MaintenanceActivity};
use tonic::{Code, Request, Response, Status};

use crate::api::{Api, log_request_data};

const MACHINE_POWER_OVERRIDE_SOURCE: &str = "component_power_control";
const MACHINE_POWER_OVERRIDE_MESSAGE: &str = "Compute-Tray component power control in progress";

fn require_component_manager(api: &Api) -> Result<&ComponentManager, Status> {
    api.component_manager
        .as_ref()
        .ok_or_else(|| Status::unimplemented("component manager is not configured"))
}

fn component_manager_error_to_status(err: ComponentManagerError) -> Status {
    match err {
        ComponentManagerError::Unavailable(msg) => Status::unavailable(msg),
        ComponentManagerError::NotFound(msg) => Status::not_found(msg),
        ComponentManagerError::InvalidArgument(msg) => Status::invalid_argument(msg),
        ComponentManagerError::Internal(msg) => Status::internal(msg),
        ComponentManagerError::Transport(e) => Status::unavailable(format!("transport error: {e}")),
        ComponentManagerError::Status(s) => s,
        ComponentManagerError::Rms(msg) => Status::internal(format!("RMS error: {msg}")),
    }
}

fn make_result(
    id: &str,
    status: rpc::ComponentManagerStatusCode,
    error: Option<String>,
) -> rpc::ComponentResult {
    rpc::ComponentResult {
        component_id: id.to_owned(),
        status: status as i32,
        error: error.unwrap_or_default(),
    }
}

fn success_result(id: &str) -> rpc::ComponentResult {
    make_result(id, rpc::ComponentManagerStatusCode::Success, None)
}

fn not_found_result(id: &str) -> rpc::ComponentResult {
    make_result(
        id,
        rpc::ComponentManagerStatusCode::NotFound,
        Some(format!("no explored endpoint found for {id}")),
    )
}

fn error_result(id: &str, error: String) -> rpc::ComponentResult {
    make_result(
        id,
        rpc::ComponentManagerStatusCode::InternalError,
        Some(error),
    )
}

fn status_result(id: &str, status: Status) -> rpc::ComponentResult {
    let component_status = match status.code() {
        Code::InvalidArgument | Code::FailedPrecondition | Code::OutOfRange => {
            rpc::ComponentManagerStatusCode::InvalidArgument
        }
        Code::NotFound => rpc::ComponentManagerStatusCode::NotFound,
        Code::AlreadyExists => rpc::ComponentManagerStatusCode::AlreadyExists,
        Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted => {
            rpc::ComponentManagerStatusCode::Unavailable
        }
        _ => rpc::ComponentManagerStatusCode::InternalError,
    };
    make_result(id, component_status, Some(status.message().to_string()))
}

fn not_found_component_result(id: &str, message: impl Into<String>) -> rpc::ComponentResult {
    make_result(
        id,
        rpc::ComponentManagerStatusCode::NotFound,
        Some(message.into()),
    )
}

fn invalid_argument_component_result(id: &str, message: impl Into<String>) -> rpc::ComponentResult {
    make_result(
        id,
        rpc::ComponentManagerStatusCode::InvalidArgument,
        Some(message.into()),
    )
}

fn rack_requested_firmware_version(rack: &model::rack::Rack) -> Option<String> {
    rack.config
        .maintenance_requested
        .as_ref()?
        .activities
        .iter()
        .find_map(|activity| match activity {
            MaintenanceActivity::FirmwareUpgrade {
                firmware_version: Some(firmware_version),
                ..
            } if !firmware_version.is_empty() => Some(firmware_version.clone()),
            _ => None,
        })
}

fn rack_firmware_upgrade_requested(rack: &model::rack::Rack) -> bool {
    rack.config
        .maintenance_requested
        .as_ref()
        .is_some_and(|scope| {
            scope.activities.is_empty()
                || scope
                    .activities
                    .iter()
                    .any(|activity| matches!(activity, MaintenanceActivity::FirmwareUpgrade { .. }))
        })
}

fn firmware_job_state(job: &FirmwareUpgradeJob) -> i32 {
    if let Some(status) = job.status.as_deref() {
        match status.to_ascii_lowercase().as_str() {
            "queued" | "pending" => return rpc::FirmwareUpdateState::FwStateQueued as i32,
            "running" | "in_progress" | "active" => {
                return rpc::FirmwareUpdateState::FwStateInProgress as i32;
            }
            "verifying" => return rpc::FirmwareUpdateState::FwStateVerifying as i32,
            "completed" | "success" | "done" => {
                return rpc::FirmwareUpdateState::FwStateCompleted as i32;
            }
            "failed" | "error" => return rpc::FirmwareUpdateState::FwStateFailed as i32,
            "cancelled" | "canceled" => return rpc::FirmwareUpdateState::FwStateCancelled as i32,
            _ => {}
        }
    }

    let devices: Vec<_> = job.all_devices().collect();
    let total = devices.len();

    if total == 0 {
        return rpc::FirmwareUpdateState::FwStateUnknown as i32;
    }

    let completed = devices
        .iter()
        .filter(|device| device.status == "completed")
        .count();
    let failed = devices
        .iter()
        .filter(|device| device.status == "failed")
        .count();
    let terminal = completed + failed;
    let has_in_progress = devices
        .iter()
        .any(|device| matches!(device.status.as_str(), "in_progress" | "running" | "active"));
    let all_queued = devices
        .iter()
        .all(|device| matches!(device.status.as_str(), "pending" | "queued" | "started"));

    if failed > 0 && terminal == total {
        rpc::FirmwareUpdateState::FwStateFailed as i32
    } else if completed == total {
        rpc::FirmwareUpdateState::FwStateCompleted as i32
    } else if terminal > 0 || has_in_progress || job.started_at.is_some() {
        rpc::FirmwareUpdateState::FwStateInProgress as i32
    } else if all_queued {
        rpc::FirmwareUpdateState::FwStateQueued as i32
    } else {
        rpc::FirmwareUpdateState::FwStateUnknown as i32
    }
}

fn rack_firmware_status(rack: &model::rack::Rack) -> rpc::FirmwareUpdateStatus {
    let requested_version = rack_requested_firmware_version(rack);
    let firmware_upgrade_requested = rack_firmware_upgrade_requested(rack);
    let job = rack.firmware_upgrade_job.as_ref();
    let state = if let Some(job) = job {
        firmware_job_state(job)
    } else if firmware_upgrade_requested {
        rpc::FirmwareUpdateState::FwStateQueued as i32
    } else {
        rpc::FirmwareUpdateState::FwStateUnknown as i32
    };
    let target_version = requested_version
        .or_else(|| job.and_then(|job| job.firmware_id.clone()))
        .unwrap_or_default();
    let updated_at = job
        .and_then(|job| job.completed_at.or(job.started_at))
        .or_else(|| firmware_upgrade_requested.then_some(rack.updated))
        .map(Into::into);

    rpc::FirmwareUpdateStatus {
        result: Some(success_result(rack.id.as_ref())),
        state,
        target_version,
        updated_at,
    }
}

fn build_inventory_entries(
    id_strings: &[String],
    report_by_id: &HashMap<String, model::site_explorer::EndpointExplorationReport>,
) -> Vec<rpc::ComponentInventoryEntry> {
    id_strings
        .iter()
        .map(|id| match report_by_id.get(id) {
            Some(report) => rpc::ComponentInventoryEntry {
                result: Some(success_result(id)),
                report: Some(report.clone().into()),
            },
            None => rpc::ComponentInventoryEntry {
                result: Some(not_found_result(id)),
                report: None,
            },
        })
        .collect()
}

fn map_power_action(raw: i32) -> Result<PowerAction, Status> {
    match SystemPowerControl::try_from(raw) {
        Ok(SystemPowerControl::On) => Ok(PowerAction::On),
        Ok(SystemPowerControl::GracefulShutdown) => Ok(PowerAction::GracefulShutdown),
        Ok(SystemPowerControl::ForceOff) => Ok(PowerAction::ForceOff),
        Ok(SystemPowerControl::GracefulRestart) => Ok(PowerAction::GracefulRestart),
        Ok(SystemPowerControl::ForceRestart) => Ok(PowerAction::ForceRestart),
        Ok(SystemPowerControl::AcPowercycle) => Ok(PowerAction::AcPowercycle),
        Ok(SystemPowerControl::Unknown) | Err(_) => Err(Status::invalid_argument(format!(
            "unknown power action: {raw}"
        ))),
    }
}

/// Maps raw proto `ComputeTrayComponent` values to display-name strings.
///
/// Keep in sync with [`firmware_component_type_to_proto`] (same file) and
/// `format_compute_tray_component` in `admin-cli/src/component_manager/versions/cmd.rs`.
fn map_compute_tray_component_names(raw: &[i32]) -> Result<Vec<String>, Status> {
    raw.iter()
        .map(|&v| match rpc::ComputeTrayComponent::try_from(v) {
            Ok(rpc::ComputeTrayComponent::Bmc) => Ok("BMC".to_string()),
            Ok(rpc::ComputeTrayComponent::Bios) => Ok("BIOS".to_string()),
            Ok(rpc::ComputeTrayComponent::Cec) => Ok("CEC".to_string()),
            Ok(rpc::ComputeTrayComponent::Nic) => Ok("NIC".to_string()),
            Ok(rpc::ComputeTrayComponent::CpldMb) => Ok("CPLD_MB".to_string()),
            Ok(rpc::ComputeTrayComponent::CpldPdb) => Ok("CPLD_PDB".to_string()),
            Ok(rpc::ComputeTrayComponent::HgxBmc) => Ok("HGX_BMC".to_string()),
            Ok(rpc::ComputeTrayComponent::CombinedBmcUefi) => Ok("COMBINED_BMC_UEFI".to_string()),
            Ok(rpc::ComputeTrayComponent::Gpu) => Ok("GPU".to_string()),
            Ok(rpc::ComputeTrayComponent::Cx7) => Ok("CX7".to_string()),
            Ok(rpc::ComputeTrayComponent::Unknown) => Err(Status::invalid_argument(
                "compute tray component must not be Unknown",
            )),
            Err(e) => Err(Status::invalid_argument(format!(
                "unrecognized compute tray component value {v}: {e}"
            ))),
        })
        .collect()
}

/// Converts a [`FirmwareComponentType`] to its proto equivalent.
///
/// Keep in sync with [`map_compute_tray_component_names`] (same file) and
/// `format_compute_tray_component` in `admin-cli/src/component_manager/versions/cmd.rs`.
fn firmware_component_type_to_proto(fct: &FirmwareComponentType) -> rpc::ComputeTrayComponent {
    match fct {
        FirmwareComponentType::Bmc => rpc::ComputeTrayComponent::Bmc,
        FirmwareComponentType::Uefi => rpc::ComputeTrayComponent::Bios,
        FirmwareComponentType::Cec => rpc::ComputeTrayComponent::Cec,
        FirmwareComponentType::Nic => rpc::ComputeTrayComponent::Nic,
        FirmwareComponentType::Cx7 => rpc::ComputeTrayComponent::Cx7,
        FirmwareComponentType::CpldMb => rpc::ComputeTrayComponent::CpldMb,
        FirmwareComponentType::CpldPdb => rpc::ComputeTrayComponent::CpldPdb,
        FirmwareComponentType::HGXBmc => rpc::ComputeTrayComponent::HgxBmc,
        FirmwareComponentType::CombinedBmcUefi => rpc::ComputeTrayComponent::CombinedBmcUefi,
        FirmwareComponentType::Gpu => rpc::ComputeTrayComponent::Gpu,
        FirmwareComponentType::Unknown => rpc::ComputeTrayComponent::Unknown,
    }
}

fn get_compute_tray_firmware_version(
    compute_machine_id: &carbide_uuid::machine::MachineId,
    bmc_info: &model::bmc_info::BmcInfo,
    endpoint_by_ip: &HashMap<IpAddr, model::site_explorer::ExploredEndpoint>,
    fw_snapshot: &carbide_firmware::FirmwareConfigSnapshot,
) -> rpc::DeviceFirmwareVersions {
    let id_str = compute_machine_id.to_string();

    let Some(ip_str) = bmc_info.ip.as_ref() else {
        return rpc::DeviceFirmwareVersions {
            result: Some(invalid_argument_component_result(
                &id_str,
                format!("machine {compute_machine_id} has no BMC IP configured"),
            )),
            ..Default::default()
        };
    };

    let Ok(ip) = ip_str.parse::<IpAddr>() else {
        tracing::warn!(
            machine_id = %compute_machine_id,
            bmc_ip = %ip_str,
            "BMC IP failed to parse as a valid address"
        );
        return rpc::DeviceFirmwareVersions {
            result: Some(error_result(
                &id_str,
                format!("machine {compute_machine_id} has unparseable BMC IP: {ip_str}"),
            )),
            ..Default::default()
        };
    };

    let Some(endpoint) = endpoint_by_ip.get(&ip) else {
        return rpc::DeviceFirmwareVersions {
            result: Some(not_found_component_result(
                &id_str,
                format!(
                    "no explored endpoint found for machine {compute_machine_id} (BMC IP {ip})"
                ),
            )),
            ..Default::default()
        };
    };

    let Some(fw) = fw_snapshot.find_fw_info_for_host(endpoint) else {
        return rpc::DeviceFirmwareVersions {
            result: Some(not_found_component_result(
                &id_str,
                format!("no firmware config matches endpoint for machine {compute_machine_id}"),
            )),
            ..Default::default()
        };
    };

    let compute_fw_versions: Vec<rpc::ComputeTrayFirmwareVersions> = fw
        .components
        .iter()
        .map(|(component_type, component)| {
            let versions = component
                .known_firmware
                .iter()
                .map(|entry| entry.version.clone())
                .collect();
            rpc::ComputeTrayFirmwareVersions {
                component: firmware_component_type_to_proto(component_type).into(),
                versions,
            }
        })
        .collect();

    if compute_fw_versions.is_empty() {
        return rpc::DeviceFirmwareVersions {
            result: Some(not_found_component_result(
                &id_str,
                format!(
                    "firmware config for machine {compute_machine_id} has no component entries"
                ),
            )),
            ..Default::default()
        };
    }

    rpc::DeviceFirmwareVersions {
        result: Some(success_result(&id_str)),
        compute_fw_versions,
        ..Default::default()
    }
}

fn map_nv_switch_component_names(raw: &[i32]) -> Result<Vec<String>, Status> {
    map_nv_switch_components(raw).map(|cs| cs.into_iter().map(|c| c.to_string()).collect())
}

fn map_nv_switch_components(raw: &[i32]) -> Result<Vec<NvSwitchComponent>, Status> {
    raw.iter()
        .filter(|&&v| v != rpc::NvSwitchComponent::Unknown as i32)
        .map(|&v| match rpc::NvSwitchComponent::try_from(v) {
            Ok(rpc::NvSwitchComponent::Bmc) => Ok(NvSwitchComponent::Bmc),
            Ok(rpc::NvSwitchComponent::Cpld) => Ok(NvSwitchComponent::Cpld),
            Ok(rpc::NvSwitchComponent::Bios) => Ok(NvSwitchComponent::Bios),
            Ok(rpc::NvSwitchComponent::Nvos) => Ok(NvSwitchComponent::Nvos),
            _ => Err(Status::invalid_argument(format!(
                "unknown NV-Switch component: {v}"
            ))),
        })
        .collect()
}

fn map_compute_tray_components(raw: &[i32]) -> Result<Vec<ModelComputeTrayComponent>, Status> {
    raw.iter()
        .map(|&v| match rpc::ComputeTrayComponent::try_from(v) {
            Ok(rpc::ComputeTrayComponent::Bmc) => Ok(ModelComputeTrayComponent::Bmc),
            Ok(rpc::ComputeTrayComponent::Bios) => Ok(ModelComputeTrayComponent::Bios),
            Ok(rpc::ComputeTrayComponent::CpldMb) => Ok(ModelComputeTrayComponent::Cpld),
            Ok(rpc::ComputeTrayComponent::Cx7) => Ok(ModelComputeTrayComponent::Cx7),
            Ok(rpc::ComputeTrayComponent::Unknown) => Err(Status::invalid_argument(
                "compute tray component must not be Unknown",
            )),
            Ok(other) => Err(Status::invalid_argument(format!(
                "compute tray component {other:?} is not supported for direct dispatch"
            ))),
            Err(e) => Err(Status::invalid_argument(format!(
                "unrecognized compute tray component value {v}: {e}"
            ))),
        })
        .collect()
}

fn map_power_shelf_components(raw: &[i32]) -> Result<Vec<PowerShelfComponent>, Status> {
    raw.iter()
        .filter(|&&v| v != rpc::PowerShelfComponent::Unknown as i32)
        .map(|&v| match rpc::PowerShelfComponent::try_from(v) {
            Ok(rpc::PowerShelfComponent::Pmc) => Ok(PowerShelfComponent::Pmc),
            Ok(rpc::PowerShelfComponent::Psu) => Ok(PowerShelfComponent::Psu),
            _ => Err(Status::invalid_argument(format!(
                "unknown power shelf component: {v}"
            ))),
        })
        .collect()
}

// ---- Endpoint resolution helpers ----

struct UnresolvedDevice<Id> {
    id: Id,
    reason: String,
}

impl<Id: std::fmt::Display> std::fmt::Display for UnresolvedDevice<Id> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.id, self.reason)
    }
}

struct ResolvedSwitchEndpoints {
    endpoints: Vec<SwitchEndpoint>,
    mac_to_id: HashMap<MacAddress, SwitchId>,
}

struct SwitchEndpoints {
    resolved: ResolvedSwitchEndpoints,
    unresolved: Vec<UnresolvedDevice<SwitchId>>,
}

async fn fetch_credentials(
    credential_manager: &dyn CredentialManager,
    key: CredentialKey,
) -> Result<Credentials, ComponentManagerError> {
    match credential_manager.get_credentials(&key).await {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err(ComponentManagerError::NotFound(format!(
            "no credentials found for {key:?}"
        ))),
        Err(e) => Err(ComponentManagerError::Internal(format!(
            "failed to fetch credentials for {key:?}: {e}"
        ))),
    }
}

async fn fetch_switch_bmc_credentials(
    credential_manager: &dyn CredentialManager,
    bmc_mac: MacAddress,
) -> Result<Credentials, ComponentManagerError> {
    let key = CredentialKey::BmcCredentials {
        credential_type: BmcCredentialType::BmcRoot {
            bmc_mac_address: bmc_mac,
        },
    };
    fetch_credentials(credential_manager, key).await
}

async fn fetch_compute_tray_bmc_credentials(
    credential_manager: &dyn CredentialManager,
    bmc_mac: MacAddress,
) -> Result<Credentials, ComponentManagerError> {
    let key = CredentialKey::BmcCredentials {
        credential_type: BmcCredentialType::BmcRoot {
            bmc_mac_address: bmc_mac,
        },
    };
    fetch_credentials(credential_manager, key).await
}

async fn fetch_switch_nvos_credentials(
    credential_manager: &dyn CredentialManager,
    bmc_mac: MacAddress,
) -> Result<Credentials, ComponentManagerError> {
    let key = CredentialKey::SwitchNvosAdmin {
        bmc_mac_address: bmc_mac,
    };
    fetch_credentials(credential_manager, key).await
}

async fn fetch_powershelf_pmc_credentials(
    credential_manager: &dyn CredentialManager,
    pmc_mac: MacAddress,
) -> Result<Credentials, ComponentManagerError> {
    let key = CredentialKey::BmcCredentials {
        credential_type: BmcCredentialType::BmcRoot {
            bmc_mac_address: pmc_mac,
        },
    };
    fetch_credentials(credential_manager, key).await
}

async fn resolve_switch_endpoints(
    api: &Api,
    switch_ids: &[SwitchId],
) -> Result<SwitchEndpoints, Status> {
    let rows = db::switch::find_switch_endpoints_by_ids(&mut api.db_reader(), switch_ids)
        .await
        .map_err(|e| Status::internal(format!("db error resolving switch endpoints: {e}")))?;

    let mut endpoints = Vec::with_capacity(rows.len());
    let mut mac_to_id = HashMap::with_capacity(rows.len());
    let mut unresolved = Vec::new();
    let mut resolved_ids = HashSet::with_capacity(rows.len());

    for row in rows {
        let (Some(nvos_mac), Some(nvos_ip)) = (row.nvos_mac, row.nvos_ip) else {
            let u = UnresolvedDevice {
                id: row.switch_id,
                reason: "NVOS MAC or IP not available".into(),
            };
            tracing::warn!(%u, "skipping switch");
            unresolved.push(u);
            resolved_ids.insert(row.switch_id);
            continue;
        };
        resolved_ids.insert(row.switch_id);

        let bmc_credentials = match fetch_switch_bmc_credentials(
            api.credential_manager.as_ref(),
            row.bmc_mac,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                let u = UnresolvedDevice {
                    id: row.switch_id,
                    reason: format!("BMC credentials unavailable: {e}"),
                };
                tracing::warn!(%u, "skipping switch");
                unresolved.push(u);
                continue;
            }
        };

        let nvos_credentials =
            match fetch_switch_nvos_credentials(api.credential_manager.as_ref(), row.bmc_mac).await
            {
                Ok(c) => c,
                Err(e) => {
                    let u = UnresolvedDevice {
                        id: row.switch_id,
                        reason: format!("NVOS credentials unavailable: {e}"),
                    };
                    tracing::warn!(%u, "skipping switch");
                    unresolved.push(u);
                    continue;
                }
            };

        mac_to_id.insert(row.bmc_mac, row.switch_id);
        endpoints.push(SwitchEndpoint {
            bmc_ip: row.bmc_ip,
            bmc_mac: row.bmc_mac,
            nvos_ip,
            nvos_mac,
            bmc_credentials,
            nvos_credentials,
        });
    }

    for id in switch_ids {
        if !resolved_ids.contains(id) {
            let u = UnresolvedDevice {
                id: *id,
                reason: "switch not found in database".into(),
            };
            tracing::warn!(%u, "skipping switch");
            unresolved.push(u);
        }
    }

    if !unresolved.is_empty() {
        tracing::warn!(
            count = unresolved.len(),
            "some switches could not be resolved to endpoints"
        );
    }

    Ok(SwitchEndpoints {
        resolved: ResolvedSwitchEndpoints {
            endpoints,
            mac_to_id,
        },
        unresolved,
    })
}

struct ResolvedPowerShelfEndpoints {
    endpoints: Vec<PowerShelfEndpoint>,
    mac_to_id: HashMap<MacAddress, PowerShelfId>,
}

struct PowerShelfEndpoints {
    resolved: ResolvedPowerShelfEndpoints,
    unresolved: Vec<UnresolvedDevice<PowerShelfId>>,
}

async fn resolve_power_shelf_endpoints(
    api: &Api,
    power_shelf_ids: &[PowerShelfId],
) -> Result<PowerShelfEndpoints, Status> {
    let rows =
        db::power_shelf::find_power_shelf_endpoints_by_ids(&mut api.db_reader(), power_shelf_ids)
            .await
            .map_err(|e| {
                Status::internal(format!("db error resolving power shelf endpoints: {e}"))
            })?;

    let mut endpoints = Vec::with_capacity(rows.len());
    let mut mac_to_id = HashMap::with_capacity(rows.len());
    let mut unresolved = Vec::new();
    let mut resolved_ids = HashSet::with_capacity(rows.len());

    for row in rows {
        resolved_ids.insert(row.power_shelf_id);

        let pmc_credentials =
            match fetch_powershelf_pmc_credentials(api.credential_manager.as_ref(), row.pmc_mac)
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    let u = UnresolvedDevice {
                        id: row.power_shelf_id,
                        reason: format!("PMC credentials unavailable: {e}"),
                    };
                    tracing::warn!(%u, "skipping power shelf");
                    unresolved.push(u);
                    continue;
                }
            };

        mac_to_id.insert(row.pmc_mac, row.power_shelf_id);
        endpoints.push(PowerShelfEndpoint {
            pmc_ip: row.pmc_ip,
            pmc_mac: row.pmc_mac,
            // TODO: retrieve vendor from DB instead of using a hardcoded default
            pmc_vendor: PowerShelfVendor::DEFAULT,
            pmc_credentials,
        });
    }

    for id in power_shelf_ids {
        if !resolved_ids.contains(id) {
            let u = UnresolvedDevice {
                id: *id,
                reason: "power shelf not found in database".into(),
            };
            tracing::warn!(%u, "skipping power shelf");
            unresolved.push(u);
        }
    }

    if !unresolved.is_empty() {
        tracing::warn!(
            count = unresolved.len(),
            "some power shelves could not be resolved to endpoints"
        );
    }

    Ok(PowerShelfEndpoints {
        resolved: ResolvedPowerShelfEndpoints {
            endpoints,
            mac_to_id,
        },
        unresolved,
    })
}

fn map_bmc_vendor_to_compute_tray(vendor: bmc_vendor::BMCVendor) -> ComputeTrayVendor {
    match vendor {
        bmc_vendor::BMCVendor::Dell => ComputeTrayVendor::Dell,
        bmc_vendor::BMCVendor::Hpe => ComputeTrayVendor::Hpe,
        bmc_vendor::BMCVendor::Lenovo => ComputeTrayVendor::Lenovo,
        bmc_vendor::BMCVendor::Supermicro => ComputeTrayVendor::Supermicro,
        bmc_vendor::BMCVendor::Nvidia => ComputeTrayVendor::Nvidia,
        _ => ComputeTrayVendor::Unknown,
    }
}

struct ResolvedComputeTrayEndpoints {
    endpoints: Vec<ComputeTrayEndpoint>,
    ip_to_machine_id: HashMap<IpAddr, carbide_uuid::machine::MachineId>,
}

struct ComputeTrayEndpoints {
    resolved: ResolvedComputeTrayEndpoints,
    unresolved: Vec<UnresolvedDevice<carbide_uuid::machine::MachineId>>,
}

async fn resolve_compute_tray_endpoints(
    api: &Api,
    machine_ids: &[carbide_uuid::machine::MachineId],
) -> Result<ComputeTrayEndpoints, Status> {
    let machines = db::machine::find(
        api.db_reader().as_mut(),
        db::ObjectFilter::List(machine_ids),
        MachineSearchConfig::default(),
    )
    .await
    .map_err(|e| Status::internal(format!("failed to look up machines: {e}")))?;

    let machine_by_id: HashMap<_, _> = machines.into_iter().map(|m| (m.id, m)).collect();

    let mut endpoints = Vec::with_capacity(machine_ids.len());
    let mut ip_to_machine_id = HashMap::with_capacity(machine_ids.len());
    let mut unresolved = Vec::new();

    for &machine_id in machine_ids {
        let Some(machine) = machine_by_id.get(&machine_id) else {
            unresolved.push(UnresolvedDevice {
                id: machine_id,
                reason: "machine not found in database".into(),
            });
            continue;
        };

        let Some(bmc_mac) = machine.bmc_info.mac else {
            unresolved.push(UnresolvedDevice {
                id: machine_id,
                reason: "BMC MAC not available".into(),
            });
            continue;
        };

        let Some(ip_str) = machine.bmc_info.ip.as_ref() else {
            unresolved.push(UnresolvedDevice {
                id: machine_id,
                reason: "BMC IP not configured".into(),
            });
            continue;
        };

        let Ok(bmc_ip) = ip_str.parse::<IpAddr>() else {
            unresolved.push(UnresolvedDevice {
                id: machine_id,
                reason: format!("unparseable BMC IP: {ip_str}"),
            });
            continue;
        };

        let bmc_credentials = match fetch_compute_tray_bmc_credentials(
            api.credential_manager.as_ref(),
            bmc_mac,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                unresolved.push(UnresolvedDevice {
                    id: machine_id,
                    reason: format!("BMC credentials unavailable: {e}"),
                });
                continue;
            }
        };

        let vendor = map_bmc_vendor_to_compute_tray(machine.bmc_vendor());

        ip_to_machine_id.insert(bmc_ip, machine_id);
        endpoints.push(ComputeTrayEndpoint {
            vendor,
            bmc_ip,
            bmc_credentials,
        });
    }

    if !unresolved.is_empty() {
        tracing::warn!(
            count = unresolved.len(),
            "some compute trays could not be resolved to endpoints"
        );
    }

    Ok(ComputeTrayEndpoints {
        resolved: ResolvedComputeTrayEndpoints {
            endpoints,
            ip_to_machine_id,
        },
        unresolved,
    })
}

fn switch_mac_to_id_str(mac: &MacAddress, mac_to_id: &HashMap<MacAddress, SwitchId>) -> String {
    mac_to_id
        .get(mac)
        .map(|id| id.to_string())
        .unwrap_or_else(|| mac.to_string())
}

fn ps_mac_to_id_str(mac: &MacAddress, mac_to_id: &HashMap<MacAddress, PowerShelfId>) -> String {
    mac_to_id
        .get(mac)
        .map(|id| id.to_string())
        .unwrap_or_else(|| mac.to_string())
}

fn map_fw_state(state: model::component_manager::FirmwareState) -> i32 {
    use model::component_manager::FirmwareState;
    match state {
        FirmwareState::Unknown => rpc::FirmwareUpdateState::FwStateUnknown as i32,
        FirmwareState::Queued => rpc::FirmwareUpdateState::FwStateQueued as i32,
        FirmwareState::InProgress => rpc::FirmwareUpdateState::FwStateInProgress as i32,
        FirmwareState::Verifying => rpc::FirmwareUpdateState::FwStateVerifying as i32,
        FirmwareState::Completed => rpc::FirmwareUpdateState::FwStateCompleted as i32,
        FirmwareState::Failed => rpc::FirmwareUpdateState::FwStateFailed as i32,
        FirmwareState::Cancelled => rpc::FirmwareUpdateState::FwStateCancelled as i32,
    }
}

// ---- Power Control ----

pub(crate) async fn component_power_control(
    api: &Api,
    request: Request<rpc::ComponentPowerControlRequest>,
) -> Result<Response<rpc::ComponentPowerControlResponse>, Status> {
    log_request_data(&request);
    let cm = require_component_manager(api)?;
    let req = request.into_inner();

    let action = map_power_action(req.action)?;

    let target = req
        .target
        .ok_or_else(|| Status::invalid_argument("target is required"))?;

    let (results, exploration_ips) = match target {
        rpc::component_power_control_request::Target::SwitchIds(list) => {
            let endpoints = resolve_switch_endpoints(api, &list.ids).await?;

            let mut results: Vec<_> = endpoints
                .unresolved
                .iter()
                .map(|u| error_result(&u.id.to_string(), u.reason.clone()))
                .collect();

            tracing::info!(
                backend = cm.nv_switch.name(),
                count = endpoints.resolved.endpoints.len(),
                ?action,
                "power control for switches"
            );
            let backend_results = cm
                .nv_switch
                .power_control(&endpoints.resolved.endpoints, action)
                .await
                .map_err(component_manager_error_to_status)?;
            results.extend(backend_results.into_iter().map(|r| {
                let id = switch_mac_to_id_str(&r.bmc_mac, &endpoints.resolved.mac_to_id);
                if r.success {
                    success_result(&id)
                } else {
                    error_result(&id, r.error.unwrap_or_default())
                }
            }));

            let ips: Vec<IpAddr> = endpoints
                .resolved
                .endpoints
                .iter()
                .map(|ep| ep.bmc_ip)
                .collect();

            (results, ips)
        }
        rpc::component_power_control_request::Target::PowerShelfIds(list) => {
            let endpoints = resolve_power_shelf_endpoints(api, &list.ids).await?;

            let mut results: Vec<_> = endpoints
                .unresolved
                .iter()
                .map(|u| error_result(&u.id.to_string(), u.reason.clone()))
                .collect();

            tracing::info!(
                backend = cm.power_shelf.name(),
                count = endpoints.resolved.endpoints.len(),
                ?action,
                "power control for power shelves"
            );
            let backend_results = cm
                .power_shelf
                .power_control(&endpoints.resolved.endpoints, action)
                .await
                .map_err(component_manager_error_to_status)?;
            results.extend(backend_results.into_iter().map(|r| {
                let id = ps_mac_to_id_str(&r.pmc_mac, &endpoints.resolved.mac_to_id);
                if r.success {
                    success_result(&id)
                } else {
                    error_result(&id, r.error.unwrap_or_default())
                }
            }));

            let ips: Vec<IpAddr> = endpoints
                .resolved
                .endpoints
                .iter()
                .map(|ep| ep.pmc_ip)
                .collect();

            (results, ips)
        }
        rpc::component_power_control_request::Target::MachineIds(list) => {
            if cm.compute_tray_use_state_controller {
                // TODO: implement state controller path for compute tray power control
                return Err(Status::unimplemented(
                    "compute tray power control through the state controller is not yet supported",
                ));
            } else {
                let resolved = resolve_compute_tray_endpoints(api, &list.machine_ids).await?;

                let mut results: Vec<_> = resolved
                    .unresolved
                    .iter()
                    .map(|u| error_result(&u.id.to_string(), u.reason.clone()))
                    .collect();

                let resolved_machine_ids: Vec<_> = resolved
                    .resolved
                    .endpoints
                    .iter()
                    .filter_map(|ep| resolved.resolved.ip_to_machine_id.get(&ep.bmc_ip).copied())
                    .collect();

                // Insert health overrides and update power-manager desired state
                // before issuing Redfish commands.
                let desired_state = desired_power_state(action) as i32;
                let mut overrides_inserted = Vec::new();
                for &machine_id in &resolved_machine_ids {
                    let inserted = power_control_health_override(api, machine_id, true).await;
                    if inserted {
                        overrides_inserted.push(machine_id);
                    }

                    let power_req = rpc::PowerOptionUpdateRequest {
                        machine_id: Some(machine_id),
                        power_state: desired_state,
                    };
                    match crate::handlers::power_options::update_power_option(
                        api,
                        Request::new(power_req),
                    )
                    .await
                    {
                        Ok(_) => {}
                        Err(e)
                            if e.code() == Code::InvalidArgument
                                && e.message().contains("already set as") =>
                        {
                            tracing::debug!(
                                %machine_id,
                                desired_state,
                                "power option already in desired state, skipping"
                            );
                        }
                        Err(e) => {
                            results.push(error_result(
                                &machine_id.to_string(),
                                format!("failed to update power option: {e}"),
                            ));
                        }
                    }
                }

                tracing::info!(
                    backend = cm.compute_tray.name(),
                    count = resolved.resolved.endpoints.len(),
                    ?action,
                    "power control for compute trays"
                );
                let backend_results = cm
                    .compute_tray
                    .power_control(&resolved.resolved.endpoints, action)
                    .await
                    .map_err(component_manager_error_to_status)?;

                // Clear health overrides after Redfish dispatch.
                for machine_id in &overrides_inserted {
                    power_control_health_override(api, *machine_id, false).await;
                }

                let ips: Vec<IpAddr> = resolved
                    .resolved
                    .endpoints
                    .iter()
                    .map(|ep| ep.bmc_ip)
                    .collect();

                results.extend(backend_results.into_iter().map(|r| {
                    let id = resolved
                        .resolved
                        .ip_to_machine_id
                        .get(&r.bmc_ip)
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| r.bmc_ip.to_string());
                    if r.success {
                        success_result(&id)
                    } else {
                        error_result(&id, r.error.unwrap_or_default())
                    }
                }));

                (results, ips)
            }
        }
    };

    // request re-exploration for the BMC/PMC endpoints that had power control initiated against them
    // so that site explorer refreshes its data for the device. NICo Flow will query the power state
    // shortly after initiating power control via this path. NICo Flow queries the power state of a
    // device via the site exploration report data.
    request_re_exploration(api, &exploration_ips).await;

    Ok(Response::new(rpc::ComponentPowerControlResponse {
        results,
    }))
}

/// Best-effort insert or removal of the health report override used to
/// suppress external alerting during compute power control.
/// Returns `true` when the operation succeeded.
async fn power_control_health_override(
    api: &Api,
    machine_id: carbide_uuid::machine::MachineId,
    insert: bool,
) -> bool {
    let result = if insert {
        let req = rpc::InsertMachineHealthReportRequest {
            machine_id: Some(machine_id),
            health_report_entry: Some(rpc::HealthReportEntry {
                report: Some(::rpc::health::HealthReport {
                    source: MACHINE_POWER_OVERRIDE_SOURCE.to_string(),
                    triggered_by: None,
                    observed_at: None,
                    successes: vec![],
                    alerts: vec![::rpc::health::HealthProbeAlert {
                        id: health_report::HealthProbeId::internal_maintenance().to_string(),
                        target: None,
                        in_alert_since: None,
                        message: MACHINE_POWER_OVERRIDE_MESSAGE.to_string(),
                        tenant_message: None,
                        classifications: vec![
                            health_report::HealthAlertClassification::suppress_external_alerting()
                                .to_string(),
                        ],
                    }],
                }),
                mode: rpc::HealthReportApplyMode::Replace as i32,
            }),
        };
        crate::handlers::health::insert_machine_health_report(api, Request::new(req))
            .await
            .map(|_| ())
    } else {
        let req = rpc::RemoveMachineHealthReportRequest {
            machine_id: Some(machine_id),
            source: MACHINE_POWER_OVERRIDE_SOURCE.to_string(),
        };
        crate::handlers::health::remove_machine_health_report(api, Request::new(req))
            .await
            .map(|_| ())
    };

    if let Err(e) = &result {
        let action = if insert { "insert" } else { "remove" };
        tracing::warn!(
            %machine_id,
            error = %e,
            "failed to {action} health report override for power control"
        );
    }

    result.is_ok()
}

fn desired_power_state(action: PowerAction) -> rpc::PowerState {
    match action {
        PowerAction::On
        | PowerAction::ForceRestart
        | PowerAction::GracefulRestart
        | PowerAction::AcPowercycle => rpc::PowerState::On,
        PowerAction::GracefulShutdown | PowerAction::ForceOff => rpc::PowerState::Off,
    }
}

/// Best-effort: flag BMC/PMC endpoints for re-exploration so the site
/// explorer refreshes its cache before `VerifyPowerStatus` polls.
async fn request_re_exploration(api: &Api, ips: &[IpAddr]) {
    if ips.is_empty() {
        return;
    }
    let result = api
        .with_txn(|txn| {
            db::explored_endpoints::request_exploration_for_addresses(ips, txn.as_mut()).boxed()
        })
        .await;
    if let Err(e) | Ok(Err(e)) = result {
        tracing::warn!(?e, "failed to request re-exploration after power control");
    }
}

// ---- Inventory ----

pub(crate) async fn get_component_inventory(
    api: &Api,
    request: Request<rpc::GetComponentInventoryRequest>,
) -> Result<Response<rpc::GetComponentInventoryResponse>, Status> {
    log_request_data(&request);
    let req = request.into_inner();

    let target = req
        .target
        .ok_or_else(|| Status::invalid_argument("target is required"))?;

    let entries = match target {
        rpc::get_component_inventory_request::Target::SwitchIds(list) => {
            let id_ip_pairs =
                db::switch::find_bmc_ips_by_switch_ids(&mut api.db_reader(), &list.ids)
                    .await
                    .map_err(|e| Status::internal(format!("db error: {e}")))?;

            let ip_to_id: HashMap<IpAddr, String> = id_ip_pairs
                .into_iter()
                .map(|(sid, ip)| (ip, sid.to_string()))
                .collect();

            let id_strings: Vec<String> = list.ids.iter().map(|id| id.to_string()).collect();
            let ips: Vec<IpAddr> = ip_to_id.keys().copied().collect();
            let endpoints = db::explored_endpoints::find_by_ips(&mut api.db_reader(), ips)
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?;

            let report_by_id: HashMap<String, _> = endpoints
                .into_iter()
                .filter_map(|ep| {
                    let id = ip_to_id.get(&ep.address)?;
                    Some((id.clone(), ep.report))
                })
                .collect();

            build_inventory_entries(&id_strings, &report_by_id)
        }
        rpc::get_component_inventory_request::Target::PowerShelfIds(list) => {
            let id_ip_pairs =
                db::power_shelf::find_bmc_ips_by_power_shelf_ids(&mut api.db_reader(), &list.ids)
                    .await
                    .map_err(|e| Status::internal(format!("db error: {e}")))?;

            let ip_to_id: HashMap<IpAddr, String> = id_ip_pairs
                .into_iter()
                .map(|(psid, ip)| (ip, psid.to_string()))
                .collect();

            let id_strings: Vec<String> = list.ids.iter().map(|id| id.to_string()).collect();
            let ips: Vec<IpAddr> = ip_to_id.keys().copied().collect();
            let endpoints = db::explored_endpoints::find_by_ips(&mut api.db_reader(), ips)
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?;

            let report_by_id: HashMap<String, _> = endpoints
                .into_iter()
                .filter_map(|ep| {
                    let id = ip_to_id.get(&ep.address)?;
                    Some((id.clone(), ep.report))
                })
                .collect();

            build_inventory_entries(&id_strings, &report_by_id)
        }
        rpc::get_component_inventory_request::Target::MachineIds(list) => {
            let id_strings: Vec<String> =
                list.machine_ids.iter().map(|id| id.to_string()).collect();

            let mut txn = api
                .txn_begin()
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?;

            let bmc_pairs = db::machine_topology::find_machine_bmc_pairs_by_machine_id(
                &mut txn,
                list.machine_ids.clone(),
            )
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

            txn.commit()
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?;

            let ip_to_id: HashMap<IpAddr, String> = bmc_pairs
                .into_iter()
                .filter_map(|(mid, ip_str)| {
                    let ip: IpAddr = ip_str?.parse().ok()?;
                    Some((ip, mid.to_string()))
                })
                .collect();

            let ips: Vec<IpAddr> = ip_to_id.keys().copied().collect();
            let endpoints = db::explored_endpoints::find_by_ips(&mut api.db_reader(), ips)
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?;

            let report_by_id: HashMap<String, _> = endpoints
                .into_iter()
                .filter_map(|ep| {
                    let id = ip_to_id.get(&ep.address)?;
                    Some((id.clone(), ep.report))
                })
                .collect();

            build_inventory_entries(&id_strings, &report_by_id)
        }
    };

    Ok(Response::new(rpc::GetComponentInventoryResponse {
        entries,
    }))
}

// ---- Firmware Update ----

pub(crate) async fn update_component_firmware(
    api: &Api,
    request: Request<rpc::UpdateComponentFirmwareRequest>,
) -> Result<Response<rpc::UpdateComponentFirmwareResponse>, Status> {
    log_request_data(&request);
    let req = request.into_inner();

    let target = req
        .target
        .ok_or_else(|| Status::invalid_argument("target is required"))?;

    let mut rack_machine_ids: Vec<String> = Vec::new();
    let mut rack_switch_ids: Vec<String> = Vec::new();
    let mut rack_id: Option<carbide_uuid::rack::RackId> = None;
    let mut power_shelf_results: Option<Vec<rpc::ComponentResult>> = None;
    let mut rack_results: Option<Vec<rpc::ComponentResult>> = None;
    let mut component_names: Vec<String> = Vec::new();

    match target {
        rpc::update_component_firmware_request::Target::Switches(t) => {
            let cm = require_component_manager(api)?;
            let list = t
                .switch_ids
                .ok_or_else(|| Status::invalid_argument("switch_ids is required"))?;
            if list.ids.is_empty() {
                return Err(Status::invalid_argument("switch_ids must not be empty"));
            }

            if cm.nv_switch_use_state_controller {
                component_names = map_nv_switch_component_names(&t.components)?;

                let mut txn =
                    api.database_connection.begin().await.map_err(|e| {
                        Status::internal(format!("failed to begin transaction: {e}"))
                    })?;
                let switch = db::switch::find_by_id(&mut txn, &list.ids[0])
                    .await
                    .map_err(|e| Status::internal(format!("failed to look up switch: {e}")))?
                    .ok_or_else(|| {
                        Status::not_found(format!("switch {} not found", list.ids[0]))
                    })?;
                drop(txn);

                rack_id = Some(switch.rack_id.ok_or_else(|| {
                    Status::failed_precondition(format!(
                        "switch {} is not associated with a rack",
                        list.ids[0]
                    ))
                })?);
                rack_switch_ids = list.ids.iter().map(|id| id.to_string()).collect();
            } else {
                // Directly dispatch to backend
                let components = map_nv_switch_components(&t.components)?;
                let endpoints = resolve_switch_endpoints(api, &list.ids).await?;

                let mut results: Vec<_> = endpoints
                    .unresolved
                    .iter()
                    .map(|u| error_result(&u.id.to_string(), u.reason.clone()))
                    .collect();

                let backend_results = cm
                    .nv_switch
                    .queue_firmware_updates(
                        &endpoints.resolved.endpoints,
                        &req.target_version,
                        &components,
                    )
                    .await
                    .map_err(component_manager_error_to_status)?;
                results.extend(backend_results.into_iter().map(|r| {
                    let id = switch_mac_to_id_str(&r.bmc_mac, &endpoints.resolved.mac_to_id);
                    if r.success {
                        success_result(&id)
                    } else {
                        error_result(&id, r.error.unwrap_or_default())
                    }
                }));

                return Ok(Response::new(rpc::UpdateComponentFirmwareResponse {
                    results,
                }));
            }
        }
        rpc::update_component_firmware_request::Target::ComputeTrays(t) => {
            let cm = require_component_manager(api)?;
            let list = t
                .machine_ids
                .ok_or_else(|| Status::invalid_argument("machine_ids is required"))?;
            if list.machine_ids.is_empty() {
                return Err(Status::invalid_argument("machine_ids must not be empty"));
            }

            if cm.compute_tray_use_state_controller {
                component_names = map_compute_tray_component_names(&t.components)?;

                let machine = db::machine::find_one(
                    api.db_reader().as_mut(),
                    &list.machine_ids[0],
                    Default::default(),
                )
                .await
                .map_err(|e| Status::internal(format!("failed to look up machine: {e}")))?
                .ok_or_else(|| {
                    Status::not_found(format!("machine {} not found", list.machine_ids[0]))
                })?;

                rack_id = Some(machine.rack_id.ok_or_else(|| {
                    Status::failed_precondition(format!(
                        "machine {} is not associated with a rack",
                        list.machine_ids[0]
                    ))
                })?);
                rack_machine_ids = list.machine_ids.iter().map(|id| id.to_string()).collect();
            } else {
                let components = map_compute_tray_components(&t.components)?;
                let resolved = resolve_compute_tray_endpoints(api, &list.machine_ids).await?;

                let mut results: Vec<_> = resolved
                    .unresolved
                    .iter()
                    .map(|u| error_result(&u.id.to_string(), u.reason.clone()))
                    .collect();

                let backend_results = cm
                    .compute_tray
                    .update_firmware(
                        &resolved.resolved.endpoints,
                        &req.target_version,
                        &components,
                    )
                    .await
                    .map_err(component_manager_error_to_status)?;
                results.extend(backend_results.into_iter().map(|r| {
                    if r.success {
                        success_result(&r.bmc_ip.to_string())
                    } else {
                        error_result(&r.bmc_ip.to_string(), r.error.unwrap_or_default())
                    }
                }));

                return Ok(Response::new(rpc::UpdateComponentFirmwareResponse {
                    results,
                }));
            }
        }
        rpc::update_component_firmware_request::Target::PowerShelves(t) => {
            let cm = require_component_manager(api)?;
            let list = t
                .power_shelf_ids
                .ok_or_else(|| Status::invalid_argument("power_shelf_ids is required"))?;
            let components = map_power_shelf_components(&t.components)?;
            let endpoints = resolve_power_shelf_endpoints(api, &list.ids).await?;

            let mut results: Vec<_> = endpoints
                .unresolved
                .iter()
                .map(|u| error_result(&u.id.to_string(), u.reason.clone()))
                .collect();

            let backend_results = cm
                .power_shelf
                .update_firmware(
                    &endpoints.resolved.endpoints,
                    &req.target_version,
                    &components,
                )
                .await
                .map_err(component_manager_error_to_status)?;
            results.extend(backend_results.into_iter().map(|r| {
                let id = ps_mac_to_id_str(&r.pmc_mac, &endpoints.resolved.mac_to_id);
                if r.success {
                    success_result(&id)
                } else {
                    error_result(&id, r.error.unwrap_or_default())
                }
            }));
            power_shelf_results = Some(results);
        }
        rpc::update_component_firmware_request::Target::Racks(t) => {
            let list = t
                .rack_ids
                .ok_or_else(|| Status::invalid_argument("rack_ids is required"))?;
            if list.rack_ids.is_empty() {
                return Err(Status::invalid_argument("rack_ids must not be empty"));
            }

            let mut results = Vec::new();
            for rack_id in list.rack_ids {
                let rack_id_string = rack_id.to_string();
                let maintenance_req = Request::new(rpc::RackMaintenanceOnDemandRequest {
                    rack_id: Some(rack_id),
                    scope: Some(rpc::RackMaintenanceScope {
                        machine_ids: vec![],
                        switch_ids: vec![],
                        power_shelf_ids: vec![],
                        activities: vec![rpc::MaintenanceActivityConfig {
                            activity: Some(
                                rpc::maintenance_activity_config::Activity::FirmwareUpgrade(
                                    rpc::FirmwareUpgradeActivity {
                                        firmware_version: req.target_version.clone(),
                                        components: vec![],
                                    },
                                ),
                            ),
                        }],
                    }),
                });

                match crate::handlers::rack::on_demand_rack_maintenance(api, maintenance_req).await
                {
                    Ok(_) => results.push(success_result(&rack_id_string)),
                    Err(status) => results.push(status_result(&rack_id_string, status)),
                }
            }
            rack_results = Some(results);
        }
    }

    if let Some(results) = power_shelf_results {
        return Ok(Response::new(rpc::UpdateComponentFirmwareResponse {
            results,
        }));
    }

    if let Some(results) = rack_results {
        return Ok(Response::new(rpc::UpdateComponentFirmwareResponse {
            results,
        }));
    }

    let rack_id = rack_id.ok_or_else(|| {
        Status::invalid_argument("no machines or switches specified for firmware upgrade")
    })?;

    let maintenance_req = Request::new(rpc::RackMaintenanceOnDemandRequest {
        rack_id: Some(rack_id),
        scope: Some(rpc::RackMaintenanceScope {
            machine_ids: rack_machine_ids.clone(),
            switch_ids: rack_switch_ids.clone(),
            power_shelf_ids: vec![],
            activities: vec![rpc::MaintenanceActivityConfig {
                activity: Some(rpc::maintenance_activity_config::Activity::FirmwareUpgrade(
                    rpc::FirmwareUpgradeActivity {
                        firmware_version: req.target_version,
                        components: component_names,
                    },
                )),
            }],
        }),
    });

    crate::handlers::rack::on_demand_rack_maintenance(api, maintenance_req).await?;

    let results: Vec<_> = rack_machine_ids
        .iter()
        .chain(rack_switch_ids.iter())
        .map(|id| success_result(id))
        .collect();

    Ok(Response::new(rpc::UpdateComponentFirmwareResponse {
        results,
    }))
}

// ---- Firmware Status ----

pub(crate) async fn get_component_firmware_status(
    api: &Api,
    request: Request<rpc::GetComponentFirmwareStatusRequest>,
) -> Result<Response<rpc::GetComponentFirmwareStatusResponse>, Status> {
    log_request_data(&request);
    let req = request.into_inner();

    let target = req
        .target
        .ok_or_else(|| Status::invalid_argument("target is required"))?;

    let statuses = match target {
        rpc::get_component_firmware_status_request::Target::SwitchIds(list) => {
            let cm = require_component_manager(api)?;
            let endpoints = resolve_switch_endpoints(api, &list.ids).await?;

            let mut statuses: Vec<_> = endpoints
                .unresolved
                .iter()
                .map(|u| rpc::FirmwareUpdateStatus {
                    result: Some(error_result(&u.id.to_string(), u.reason.clone())),
                    state: rpc::FirmwareUpdateState::FwStateUnknown as i32,
                    target_version: String::new(),
                    updated_at: None,
                })
                .collect();

            let backend_statuses = cm
                .nv_switch
                .get_firmware_status(&endpoints.resolved.endpoints)
                .await
                .map_err(component_manager_error_to_status)?;
            statuses.extend(backend_statuses.into_iter().map(|s| {
                let id = switch_mac_to_id_str(&s.bmc_mac, &endpoints.resolved.mac_to_id);
                rpc::FirmwareUpdateStatus {
                    result: Some(if s.error.is_none() {
                        success_result(&id)
                    } else {
                        error_result(&id, s.error.unwrap_or_default())
                    }),
                    state: map_fw_state(s.state),
                    target_version: s.target_version,
                    updated_at: None,
                }
            }));
            statuses
        }
        rpc::get_component_firmware_status_request::Target::PowerShelfIds(list) => {
            let cm = require_component_manager(api)?;
            let endpoints = resolve_power_shelf_endpoints(api, &list.ids).await?;

            let mut statuses: Vec<_> = endpoints
                .unresolved
                .iter()
                .map(|u| rpc::FirmwareUpdateStatus {
                    result: Some(error_result(&u.id.to_string(), u.reason.clone())),
                    state: rpc::FirmwareUpdateState::FwStateUnknown as i32,
                    target_version: String::new(),
                    updated_at: None,
                })
                .collect();

            let backend_statuses = cm
                .power_shelf
                .get_firmware_status(&endpoints.resolved.endpoints)
                .await
                .map_err(component_manager_error_to_status)?;
            statuses.extend(backend_statuses.into_iter().map(|s| {
                let id = ps_mac_to_id_str(&s.pmc_mac, &endpoints.resolved.mac_to_id);
                rpc::FirmwareUpdateStatus {
                    result: Some(if s.error.is_none() {
                        success_result(&id)
                    } else {
                        error_result(&id, s.error.unwrap_or_default())
                    }),
                    state: map_fw_state(s.state),
                    target_version: s.target_version,
                    updated_at: None,
                }
            }));
            statuses
        }
        rpc::get_component_firmware_status_request::Target::MachineIds(_) => {
            return Err(Status::unimplemented(
                "machine firmware status is not supported via this RPC",
            ));
        }
        rpc::get_component_firmware_status_request::Target::RackIds(list) => {
            if list.rack_ids.is_empty() {
                return Err(Status::invalid_argument("rack_ids must not be empty"));
            }

            let requested_rack_ids = list.rack_ids;
            let racks = db::rack::find_by(
                api.db_reader().as_mut(),
                db::ObjectColumnFilter::List(db::rack::IdColumn, &requested_rack_ids),
            )
            .await
            .map_err(|e| Status::internal(format!("failed to look up racks: {e}")))?;
            let rack_by_id: HashMap<_, _> = racks
                .into_iter()
                .map(|rack| (rack.id.clone(), rack))
                .collect();

            requested_rack_ids
                .iter()
                .map(|rack_id| {
                    rack_by_id.get(rack_id).map(rack_firmware_status).unwrap_or(
                        rpc::FirmwareUpdateStatus {
                            result: Some(not_found_component_result(
                                rack_id.as_ref(),
                                format!("rack {rack_id} not found"),
                            )),
                            state: rpc::FirmwareUpdateState::FwStateUnknown as i32,
                            target_version: String::new(),
                            updated_at: None,
                        },
                    )
                })
                .collect()
        }
    };

    Ok(Response::new(rpc::GetComponentFirmwareStatusResponse {
        statuses,
    }))
}

// ---- List Firmware Versions ----

pub(crate) async fn list_component_firmware_versions(
    api: &Api,
    request: Request<rpc::ListComponentFirmwareVersionsRequest>,
) -> Result<Response<rpc::ListComponentFirmwareVersionsResponse>, Status> {
    log_request_data(&request);
    let req = request.into_inner();

    let target = req
        .target
        .ok_or_else(|| Status::invalid_argument("target is required"))?;

    match target {
        rpc::list_component_firmware_versions_request::Target::SwitchIds(list) => {
            let cm = require_component_manager(api)?;
            let endpoints = resolve_switch_endpoints(api, &list.ids).await?;

            let mut devices: Vec<rpc::DeviceFirmwareVersions> = endpoints
                .unresolved
                .iter()
                .map(|u| rpc::DeviceFirmwareVersions {
                    result: Some(error_result(&u.id.to_string(), u.reason.clone())),
                    ..Default::default()
                })
                .collect();

            let versions = cm
                .nv_switch
                .list_firmware_bundles()
                .await
                .map_err(component_manager_error_to_status)?;

            for ep in &endpoints.resolved.endpoints {
                let id = endpoints
                    .resolved
                    .mac_to_id
                    .get(&ep.bmc_mac)
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                devices.push(rpc::DeviceFirmwareVersions {
                    result: Some(success_result(&id)),
                    versions: versions.clone(),
                    ..Default::default()
                });
            }

            Ok(Response::new(rpc::ListComponentFirmwareVersionsResponse {
                devices,
            }))
        }
        rpc::list_component_firmware_versions_request::Target::PowerShelfIds(list) => {
            let cm = require_component_manager(api)?;
            let endpoints = resolve_power_shelf_endpoints(api, &list.ids).await?;

            let mut devices: Vec<rpc::DeviceFirmwareVersions> = endpoints
                .unresolved
                .iter()
                .map(|u| rpc::DeviceFirmwareVersions {
                    result: Some(error_result(&u.id.to_string(), u.reason.clone())),
                    ..Default::default()
                })
                .collect();

            let fw_results = cm
                .power_shelf
                .list_firmware(&endpoints.resolved.endpoints)
                .await
                .map_err(component_manager_error_to_status)?;

            for fv in fw_results {
                let id = endpoints
                    .resolved
                    .mac_to_id
                    .get(&fv.pmc_mac)
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                let result = if let Some(err) = fv.error {
                    error_result(&id, err)
                } else {
                    success_result(&id)
                };
                devices.push(rpc::DeviceFirmwareVersions {
                    result: Some(result),
                    versions: fv.versions,
                    ..Default::default()
                });
            }

            Ok(Response::new(rpc::ListComponentFirmwareVersionsResponse {
                devices,
            }))
        }
        rpc::list_component_firmware_versions_request::Target::MachineIds(list) => {
            let cm = require_component_manager(api)?;
            if list.machine_ids.is_empty() {
                return Err(Status::invalid_argument("machine_ids must not be empty"));
            }

            if cm.compute_tray_use_state_controller {
                let fw_snapshot = api.runtime_config.get_firmware_config().create_snapshot();

                let machines = db::machine::find(
                    api.db_reader().as_mut(),
                    db::ObjectFilter::List(&list.machine_ids),
                    MachineSearchConfig::default(),
                )
                .await
                .map_err(|e| Status::internal(format!("failed to look up machines: {e}")))?;

                let bmc_ips: Vec<IpAddr> = machines
                    .iter()
                    .filter_map(|m| m.bmc_info.ip.as_ref()?.parse().ok())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();

                let endpoints =
                    db::explored_endpoints::find_by_ips(api.db_reader().as_mut(), bmc_ips)
                        .await
                        .map_err(|e| {
                            Status::internal(format!("failed to look up explored endpoints: {e}"))
                        })?;

                let endpoint_by_ip: HashMap<IpAddr, _> =
                    endpoints.into_iter().map(|ep| (ep.address, ep)).collect();

                let machine_by_id: HashMap<_, _> =
                    machines.into_iter().map(|m| (m.id, m)).collect();

                let devices = list
                    .machine_ids
                    .iter()
                    .map(|machine_id| {
                        let Some(machine) = machine_by_id.get(machine_id) else {
                            return rpc::DeviceFirmwareVersions {
                                result: Some(not_found_component_result(
                                    &machine_id.to_string(),
                                    format!("machine {machine_id} not found"),
                                )),
                                ..Default::default()
                            };
                        };
                        get_compute_tray_firmware_version(
                            machine_id,
                            &machine.bmc_info,
                            &endpoint_by_ip,
                            &fw_snapshot,
                        )
                    })
                    .collect();

                Ok(Response::new(rpc::ListComponentFirmwareVersionsResponse {
                    devices,
                }))
            } else {
                let resolved = resolve_compute_tray_endpoints(api, &list.machine_ids).await?;

                let mut devices: Vec<rpc::DeviceFirmwareVersions> = resolved
                    .unresolved
                    .iter()
                    .map(|u| rpc::DeviceFirmwareVersions {
                        result: Some(error_result(&u.id.to_string(), u.reason.clone())),
                        ..Default::default()
                    })
                    .collect();

                let versions = cm
                    .compute_tray
                    .list_firmware_bundles()
                    .await
                    .map_err(component_manager_error_to_status)?;

                for ep in &resolved.resolved.endpoints {
                    let id = resolved
                        .resolved
                        .ip_to_machine_id
                        .get(&ep.bmc_ip)
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| ep.bmc_ip.to_string());
                    devices.push(rpc::DeviceFirmwareVersions {
                        result: Some(success_result(&id)),
                        versions: versions.clone(),
                        ..Default::default()
                    });
                }

                Ok(Response::new(rpc::ListComponentFirmwareVersionsResponse {
                    devices,
                }))
            }
        }
        rpc::list_component_firmware_versions_request::Target::RackIds(list) => {
            if list.rack_ids.is_empty() {
                return Err(Status::invalid_argument("rack_ids must not be empty"));
            }

            let requested_rack_ids = list.rack_ids;
            let racks = db::rack::find_by(
                api.db_reader().as_mut(),
                db::ObjectColumnFilter::List(db::rack::IdColumn, &requested_rack_ids),
            )
            .await
            .map_err(|e| Status::internal(format!("failed to look up racks: {e}")))?;
            let rack_by_id: HashMap<_, _> = racks
                .into_iter()
                .map(|rack| (rack.id.clone(), rack))
                .collect();

            let mut txn = api
                .database_connection
                .begin()
                .await
                .map_err(|e| Status::internal(format!("failed to begin transaction: {e}")))?;
            let firmwares = db::rack_firmware::list_all(
                &mut txn,
                model::rack_firmware::RackFirmwareSearchFilter {
                    only_available: true,
                    rack_hardware_type: None,
                },
            )
            .await
            .map_err(|e| Status::internal(format!("failed to list rack firmware: {e}")))?;
            txn.commit()
                .await
                .map_err(|e| Status::internal(format!("failed to commit transaction: {e}")))?;

            let devices = requested_rack_ids
                .iter()
                .map(|rack_id| {
                    let Some(rack) = rack_by_id.get(rack_id) else {
                        return rpc::DeviceFirmwareVersions {
                            result: Some(not_found_component_result(
                                rack_id.as_ref(),
                                format!("rack {rack_id} not found"),
                            )),
                            ..Default::default()
                        };
                    };

                    let Some(profile_id) = rack.rack_profile_id.as_ref() else {
                        return rpc::DeviceFirmwareVersions {
                            result: Some(invalid_argument_component_result(
                                rack_id.as_ref(),
                                format!("rack {rack_id} has no rack_profile_id"),
                            )),
                            ..Default::default()
                        };
                    };

                    let Some(profile) = api.runtime_config.rack_profiles.get(profile_id.as_str())
                    else {
                        return rpc::DeviceFirmwareVersions {
                            result: Some(not_found_component_result(
                                rack_id.as_ref(),
                                format!("rack profile {profile_id} not found"),
                            )),
                            ..Default::default()
                        };
                    };

                    let Some(rack_hardware_type) = profile.rack_hardware_type.as_ref() else {
                        return rpc::DeviceFirmwareVersions {
                            result: Some(invalid_argument_component_result(
                                rack_id.as_ref(),
                                format!(
                                    "rack profile {profile_id} does not define rack_hardware_type"
                                ),
                            )),
                            ..Default::default()
                        };
                    };

                    let versions = firmwares
                        .iter()
                        .filter(|firmware| {
                            firmware.rack_hardware_type.is_any()
                                || firmware.rack_hardware_type == *rack_hardware_type
                        })
                        .map(|firmware| firmware.id.clone())
                        .collect();

                    rpc::DeviceFirmwareVersions {
                        result: Some(success_result(rack_id.as_ref())),
                        versions,
                        ..Default::default()
                    }
                })
                .collect();

            Ok(Response::new(rpc::ListComponentFirmwareVersionsResponse {
                devices,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use config_version::{ConfigVersion, Versioned};
    use model::component_manager::FirmwareState;
    use model::metadata::Metadata;
    use model::rack::{Rack, RackConfig, RackState};
    use tonic::Code;

    use super::*;

    fn firmware_device(status: &str) -> model::rack::FirmwareUpgradeDeviceStatus {
        model::rack::FirmwareUpgradeDeviceStatus {
            node_id: String::new(),
            mac: "00:00:00:00:00:00".to_string(),
            bmc_ip: String::new(),
            status: status.to_string(),
            job_id: None,
            parent_job_id: None,
            error_message: None,
        }
    }

    fn test_rack_with_job(job: Option<FirmwareUpgradeJob>) -> Rack {
        Rack {
            id: Default::default(),
            rack_profile_id: None,
            config: RackConfig::default(),
            controller_state: Versioned::new(RackState::Ready, ConfigVersion::initial()),
            controller_state_outcome: None,
            firmware_upgrade_job: job,
            nvos_update_job: None,
            health_reports: Default::default(),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            deleted: None,
            metadata: Metadata::default(),
            version: ConfigVersion::initial(),
        }
    }

    #[test]
    fn error_to_status_unavailable() {
        let st =
            component_manager_error_to_status(ComponentManagerError::Unavailable("gone".into()));
        assert_eq!(st.code(), Code::Unavailable);
        assert!(st.message().contains("gone"));
    }

    #[test]
    fn error_to_status_not_found() {
        let st =
            component_manager_error_to_status(ComponentManagerError::NotFound("missing".into()));
        assert_eq!(st.code(), Code::NotFound);
    }

    #[test]
    fn error_to_status_invalid_argument() {
        let st =
            component_manager_error_to_status(ComponentManagerError::InvalidArgument("bad".into()));
        assert_eq!(st.code(), Code::InvalidArgument);
    }

    #[test]
    fn error_to_status_internal() {
        let st = component_manager_error_to_status(ComponentManagerError::Internal("oops".into()));
        assert_eq!(st.code(), Code::Internal);
    }

    #[test]
    fn error_to_status_passthrough() {
        let original = Status::permission_denied("nope");
        let st = component_manager_error_to_status(ComponentManagerError::Status(original));
        assert_eq!(st.code(), Code::PermissionDenied);
    }

    #[test]
    fn power_action_on() {
        let action = map_power_action(SystemPowerControl::On as i32).unwrap();
        assert!(matches!(action, PowerAction::On));
    }

    #[test]
    fn power_action_graceful_shutdown() {
        let action = map_power_action(SystemPowerControl::GracefulShutdown as i32).unwrap();
        assert!(matches!(action, PowerAction::GracefulShutdown));
    }

    #[test]
    fn power_action_force_off() {
        let action = map_power_action(SystemPowerControl::ForceOff as i32).unwrap();
        assert!(matches!(action, PowerAction::ForceOff));
    }

    #[test]
    fn power_action_graceful_restart() {
        let action = map_power_action(SystemPowerControl::GracefulRestart as i32).unwrap();
        assert!(matches!(action, PowerAction::GracefulRestart));
    }

    #[test]
    fn power_action_force_restart() {
        let action = map_power_action(SystemPowerControl::ForceRestart as i32).unwrap();
        assert!(matches!(action, PowerAction::ForceRestart));
    }

    #[test]
    fn power_action_ac_powercycle() {
        let action = map_power_action(SystemPowerControl::AcPowercycle as i32).unwrap();
        assert!(matches!(action, PowerAction::AcPowercycle));
    }

    #[test]
    fn power_action_unknown_rejected() {
        let err = map_power_action(SystemPowerControl::Unknown as i32).unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    #[test]
    fn power_action_unset_defaults_to_zero_and_is_rejected() {
        let req = rpc::ComponentPowerControlRequest::default();
        assert_eq!(req.action, 0);
        let err = map_power_action(req.action).unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    #[test]
    fn power_action_invalid_value() {
        let err = map_power_action(9999).unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    #[test]
    fn firmware_job_state_explicit_status_wins_for_empty_job() {
        let job = FirmwareUpgradeJob {
            status: Some("queued".to_string()),
            ..Default::default()
        };

        assert_eq!(
            firmware_job_state(&job),
            rpc::FirmwareUpdateState::FwStateQueued as i32
        );
    }

    #[test]
    fn firmware_job_state_empty_job_without_status_is_unknown() {
        assert_eq!(
            firmware_job_state(&FirmwareUpgradeJob::default()),
            rpc::FirmwareUpdateState::FwStateUnknown as i32
        );
    }

    #[test]
    fn firmware_job_state_all_completed_is_completed() {
        let job = FirmwareUpgradeJob {
            machines: vec![firmware_device("completed")],
            switches: vec![firmware_device("completed")],
            ..Default::default()
        };

        assert_eq!(
            firmware_job_state(&job),
            rpc::FirmwareUpdateState::FwStateCompleted as i32
        );
    }

    #[test]
    fn firmware_job_state_mixed_terminal_with_failure_is_failed() {
        let job = FirmwareUpgradeJob {
            machines: vec![firmware_device("completed")],
            switches: vec![firmware_device("failed")],
            ..Default::default()
        };

        assert_eq!(
            firmware_job_state(&job),
            rpc::FirmwareUpdateState::FwStateFailed as i32
        );
    }

    #[test]
    fn firmware_job_state_partial_terminal_is_in_progress() {
        let job = FirmwareUpgradeJob {
            machines: vec![firmware_device("completed")],
            switches: vec![firmware_device("pending")],
            ..Default::default()
        };

        assert_eq!(
            firmware_job_state(&job),
            rpc::FirmwareUpdateState::FwStateInProgress as i32
        );
    }

    #[test]
    fn firmware_job_state_all_pending_without_start_is_queued() {
        let job = FirmwareUpgradeJob {
            machines: vec![firmware_device("pending")],
            switches: vec![firmware_device("queued")],
            ..Default::default()
        };

        assert_eq!(
            firmware_job_state(&job),
            rpc::FirmwareUpdateState::FwStateQueued as i32
        );
    }

    #[test]
    fn firmware_job_state_unknown_device_status_is_unknown() {
        let job = FirmwareUpgradeJob {
            machines: vec![firmware_device("mystery")],
            ..Default::default()
        };

        assert_eq!(
            firmware_job_state(&job),
            rpc::FirmwareUpdateState::FwStateUnknown as i32
        );
    }

    #[test]
    fn rack_firmware_status_reports_retained_completed_job() {
        let job = FirmwareUpgradeJob {
            firmware_id: Some("fw-1".to_string()),
            status: Some("completed".to_string()),
            started_at: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
            completed_at: Some(chrono::Utc::now()),
            ..Default::default()
        };
        let rack = test_rack_with_job(Some(job));

        let status = rack_firmware_status(&rack);

        assert_eq!(
            status.state,
            rpc::FirmwareUpdateState::FwStateCompleted as i32
        );
        assert_eq!(status.target_version, "fw-1");
        assert!(status.updated_at.is_some());
    }

    #[test]
    fn rack_firmware_status_default_request_uses_job_firmware_id() {
        let job = FirmwareUpgradeJob {
            firmware_id: Some("fw-default".to_string()),
            status: Some("in_progress".to_string()),
            started_at: Some(chrono::Utc::now()),
            ..Default::default()
        };
        let mut rack = test_rack_with_job(Some(job));
        rack.config.maintenance_requested = Some(model::rack::MaintenanceScope {
            activities: vec![MaintenanceActivity::FirmwareUpgrade {
                firmware_version: None,
                components: vec![],
            }],
            ..Default::default()
        });

        let status = rack_firmware_status(&rack);

        assert_eq!(
            status.state,
            rpc::FirmwareUpdateState::FwStateInProgress as i32
        );
        assert_eq!(status.target_version, "fw-default");
        assert!(status.updated_at.is_some());
    }

    #[test]
    fn rack_firmware_status_default_request_without_job_is_queued() {
        let mut rack = test_rack_with_job(None);
        rack.config.maintenance_requested = Some(model::rack::MaintenanceScope {
            activities: vec![MaintenanceActivity::FirmwareUpgrade {
                firmware_version: None,
                components: vec![],
            }],
            ..Default::default()
        });

        let status = rack_firmware_status(&rack);

        assert_eq!(status.state, rpc::FirmwareUpdateState::FwStateQueued as i32);
        assert!(status.target_version.is_empty());
        assert!(status.updated_at.is_some());
    }

    #[test]
    fn fw_state_round_trip_all_variants() {
        let cases = [
            (
                FirmwareState::Unknown,
                rpc::FirmwareUpdateState::FwStateUnknown as i32,
            ),
            (
                FirmwareState::Queued,
                rpc::FirmwareUpdateState::FwStateQueued as i32,
            ),
            (
                FirmwareState::InProgress,
                rpc::FirmwareUpdateState::FwStateInProgress as i32,
            ),
            (
                FirmwareState::Verifying,
                rpc::FirmwareUpdateState::FwStateVerifying as i32,
            ),
            (
                FirmwareState::Completed,
                rpc::FirmwareUpdateState::FwStateCompleted as i32,
            ),
            (
                FirmwareState::Failed,
                rpc::FirmwareUpdateState::FwStateFailed as i32,
            ),
            (
                FirmwareState::Cancelled,
                rpc::FirmwareUpdateState::FwStateCancelled as i32,
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(map_fw_state(input), expected, "mismatch for {input:?}");
        }
    }

    #[test]
    fn firmware_component_type_to_proto_round_trip() {
        use model::firmware::FirmwareComponentType;

        let cases = [
            (FirmwareComponentType::Bmc, rpc::ComputeTrayComponent::Bmc),
            (FirmwareComponentType::Uefi, rpc::ComputeTrayComponent::Bios),
            (FirmwareComponentType::Cec, rpc::ComputeTrayComponent::Cec),
            (FirmwareComponentType::Nic, rpc::ComputeTrayComponent::Nic),
            (
                FirmwareComponentType::CpldMb,
                rpc::ComputeTrayComponent::CpldMb,
            ),
            (
                FirmwareComponentType::CpldPdb,
                rpc::ComputeTrayComponent::CpldPdb,
            ),
            (
                FirmwareComponentType::HGXBmc,
                rpc::ComputeTrayComponent::HgxBmc,
            ),
            (
                FirmwareComponentType::CombinedBmcUefi,
                rpc::ComputeTrayComponent::CombinedBmcUefi,
            ),
            (FirmwareComponentType::Gpu, rpc::ComputeTrayComponent::Gpu),
            (FirmwareComponentType::Cx7, rpc::ComputeTrayComponent::Cx7),
            (
                FirmwareComponentType::Unknown,
                rpc::ComputeTrayComponent::Unknown,
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(
                firmware_component_type_to_proto(&input),
                expected,
                "mismatch for {input:?}"
            );
        }
    }

    #[test]
    fn make_result_fields() {
        let r = make_result(
            "sw-1",
            rpc::ComponentManagerStatusCode::Success,
            Some("info".into()),
        );
        assert_eq!(r.component_id, "sw-1");
        assert_eq!(r.status, rpc::ComponentManagerStatusCode::Success as i32);
        assert_eq!(r.error, "info");
    }

    #[test]
    fn success_result_has_no_error() {
        let r = success_result("sw-2");
        assert_eq!(r.status, rpc::ComponentManagerStatusCode::Success as i32);
        assert!(r.error.is_empty());
    }

    #[test]
    fn not_found_result_has_error_message() {
        let r = not_found_result("sw-3");
        assert_eq!(r.status, rpc::ComponentManagerStatusCode::NotFound as i32);
        assert!(r.error.contains("sw-3"));
    }

    #[test]
    fn error_result_has_internal_error_status() {
        let r = error_result("sw-4", "boom".into());
        assert_eq!(
            r.status,
            rpc::ComponentManagerStatusCode::InternalError as i32,
        );
        assert_eq!(r.error, "boom");
    }

    fn test_switch_id() -> SwitchId {
        use carbide_uuid::switch::{SwitchIdSource, SwitchType};
        SwitchId::new(SwitchIdSource::Tpm, [0u8; 32], SwitchType::NvLink)
    }

    fn test_power_shelf_id() -> PowerShelfId {
        use carbide_uuid::power_shelf::{PowerShelfIdSource, PowerShelfType};
        PowerShelfId::new(PowerShelfIdSource::Tpm, [0u8; 32], PowerShelfType::Rack)
    }

    #[test]
    fn switch_mac_to_id_str_found() {
        let mac: MacAddress = "AA:BB:CC:DD:EE:01".parse().unwrap();
        let id = test_switch_id();
        let map = HashMap::from([(mac, id)]);
        assert_eq!(switch_mac_to_id_str(&mac, &map), id.to_string());
    }

    #[test]
    fn switch_mac_to_id_str_not_found_falls_back_to_mac() {
        let mac: MacAddress = "AA:BB:CC:DD:EE:01".parse().unwrap();
        let map = HashMap::new();
        assert_eq!(switch_mac_to_id_str(&mac, &map), mac.to_string());
    }

    #[test]
    fn ps_mac_to_id_str_found() {
        let mac: MacAddress = "AA:BB:CC:DD:EE:02".parse().unwrap();
        let id = test_power_shelf_id();
        let map = HashMap::from([(mac, id)]);
        assert_eq!(ps_mac_to_id_str(&mac, &map), id.to_string());
    }

    #[test]
    fn ps_mac_to_id_str_not_found_falls_back_to_mac() {
        let mac: MacAddress = "AA:BB:CC:DD:EE:02".parse().unwrap();
        let map = HashMap::new();
        assert_eq!(ps_mac_to_id_str(&mac, &map), mac.to_string());
    }

    #[test]
    fn unresolved_switch_produces_error_result_with_reason() {
        let id = test_switch_id();
        let u = UnresolvedDevice {
            id,
            reason: "BMC credentials unavailable: no BMC credentials found".into(),
        };
        let r = error_result(&u.id.to_string(), u.reason);
        assert_eq!(r.component_id, id.to_string());
        assert_eq!(
            r.status,
            rpc::ComponentManagerStatusCode::InternalError as i32,
        );
        assert!(r.error.contains("BMC credentials unavailable"));
    }

    #[test]
    fn unresolved_power_shelf_produces_error_result_with_reason() {
        let id = test_power_shelf_id();
        let u = UnresolvedDevice {
            id,
            reason: "PMC credentials unavailable: no PMC credentials found".into(),
        };
        let r = error_result(&u.id.to_string(), u.reason);
        assert_eq!(r.component_id, id.to_string());
        assert_eq!(
            r.status,
            rpc::ComponentManagerStatusCode::InternalError as i32,
        );
        assert!(r.error.contains("PMC credentials unavailable"));
    }

    #[test]
    fn unresolved_device_display() {
        let id = test_switch_id();
        let u = UnresolvedDevice {
            id,
            reason: "NVOS MAC or IP not available".into(),
        };
        let display = u.to_string();
        assert!(display.contains(&id.to_string()));
        assert!(display.contains("NVOS MAC or IP not available"));
    }

    #[test]
    fn desired_power_state_on_variants() {
        use super::desired_power_state;
        assert_eq!(
            desired_power_state(PowerAction::On),
            self::rpc::PowerState::On
        );
        assert_eq!(
            desired_power_state(PowerAction::ForceRestart),
            self::rpc::PowerState::On
        );
        assert_eq!(
            desired_power_state(PowerAction::GracefulRestart),
            self::rpc::PowerState::On
        );
        assert_eq!(
            desired_power_state(PowerAction::AcPowercycle),
            self::rpc::PowerState::On
        );
    }

    #[test]
    fn desired_power_state_off_variants() {
        use super::desired_power_state;
        assert_eq!(
            desired_power_state(PowerAction::GracefulShutdown),
            self::rpc::PowerState::Off
        );
        assert_eq!(
            desired_power_state(PowerAction::ForceOff),
            self::rpc::PowerState::Off
        );
    }

    // ---- get_compute_tray_firmware_version tests ----

    use carbide_uuid::machine::{MachineIdSource, MachineType};
    use model::bmc_info::BmcInfo;
    use model::site_explorer::ExploredEndpoint;

    fn test_machine_id() -> carbide_uuid::machine::MachineId {
        carbide_uuid::machine::MachineId::new(MachineIdSource::Tpm, [0u8; 32], MachineType::Host)
    }

    fn stub_endpoint(ip: IpAddr) -> ExploredEndpoint {
        ExploredEndpoint {
            address: ip,
            report: Default::default(),
            report_version: ConfigVersion::initial(),
            preingestion_state: model::site_explorer::PreingestionState::Initial,
            waiting_for_explorer_refresh: false,
            exploration_requested: false,
            last_redfish_bmc_reset: None,
            last_ipmitool_bmc_reset: None,
            last_redfish_reboot: None,
            last_redfish_powercycle: None,
            pause_ingestion_and_poweron: false,
            pause_remediation: false,
            boot_interface_mac: None,
        }
    }

    fn fw_snapshot_from(
        models: HashMap<String, model::firmware::Firmware>,
    ) -> carbide_firmware::FirmwareConfigSnapshot {
        carbide_firmware::FirmwareConfig::new(std::path::PathBuf::new(), &models, &HashMap::new())
            .create_snapshot()
    }

    fn empty_fw_snapshot() -> carbide_firmware::FirmwareConfigSnapshot {
        fw_snapshot_from(HashMap::new())
    }

    #[test]
    fn compute_fw_versions_no_bmc_ip() {
        let id = test_machine_id();
        let bmc = BmcInfo::default();
        let result =
            get_compute_tray_firmware_version(&id, &bmc, &HashMap::new(), &empty_fw_snapshot());
        let r = result.result.unwrap();
        assert_eq!(
            r.status,
            rpc::ComponentManagerStatusCode::InvalidArgument as i32,
        );
        assert!(r.error.contains("no BMC IP configured"));
    }

    #[test]
    fn compute_fw_versions_unparseable_ip() {
        let id = test_machine_id();
        let bmc = BmcInfo {
            ip: Some("not-an-ip".into()),
            ..Default::default()
        };
        let result =
            get_compute_tray_firmware_version(&id, &bmc, &HashMap::new(), &empty_fw_snapshot());
        let r = result.result.unwrap();
        assert_eq!(
            r.status,
            rpc::ComponentManagerStatusCode::InternalError as i32,
        );
        assert!(r.error.contains("unparseable BMC IP"));
    }

    #[test]
    fn compute_fw_versions_no_explored_endpoint() {
        let id = test_machine_id();
        let bmc = BmcInfo {
            ip: Some("10.0.0.1".into()),
            ..Default::default()
        };
        let result =
            get_compute_tray_firmware_version(&id, &bmc, &HashMap::new(), &empty_fw_snapshot());
        let r = result.result.unwrap();
        assert_eq!(r.status, rpc::ComponentManagerStatusCode::NotFound as i32);
        assert!(r.error.contains("no explored endpoint found"));
    }

    #[test]
    fn compute_fw_versions_no_firmware_config() {
        let id = test_machine_id();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let bmc = BmcInfo {
            ip: Some("10.0.0.1".into()),
            ..Default::default()
        };
        let endpoints = HashMap::from([(ip, stub_endpoint(ip))]);
        let result = get_compute_tray_firmware_version(&id, &bmc, &endpoints, &empty_fw_snapshot());
        let r = result.result.unwrap();
        assert_eq!(r.status, rpc::ComponentManagerStatusCode::NotFound as i32);
        assert!(r.error.contains("no firmware config matches"));
    }

    #[test]
    fn compute_fw_versions_empty_components() {
        use model::firmware::Firmware;

        let id = test_machine_id();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let bmc = BmcInfo {
            ip: Some("10.0.0.1".into()),
            ..Default::default()
        };

        let mut endpoint = stub_endpoint(ip);
        endpoint.report.vendor = Some(bmc_vendor::BMCVendor::Nvidia);
        endpoint.report.model = Some("TestModel".into());
        let endpoints = HashMap::from([(ip, endpoint)]);

        let fw = Firmware {
            vendor: bmc_vendor::BMCVendor::Nvidia,
            model: "TestModel".into(),
            components: HashMap::new(),
            ..Default::default()
        };
        let models = HashMap::from([("TestModel".into(), fw)]);
        let fw_snapshot = fw_snapshot_from(models);

        let result = get_compute_tray_firmware_version(&id, &bmc, &endpoints, &fw_snapshot);
        let r = result.result.unwrap();
        assert_eq!(r.status, rpc::ComponentManagerStatusCode::NotFound as i32);
        assert!(r.error.contains("no component entries"));
    }

    #[test]
    fn compute_fw_versions_success() {
        use model::firmware::{Firmware, FirmwareComponent, FirmwareEntry};

        let id = test_machine_id();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let bmc = BmcInfo {
            ip: Some("10.0.0.1".into()),
            ..Default::default()
        };

        let mut endpoint = stub_endpoint(ip);
        endpoint.report.vendor = Some(bmc_vendor::BMCVendor::Nvidia);
        endpoint.report.model = Some("TestModel".into());
        let endpoints = HashMap::from([(ip, endpoint)]);

        let mut components = HashMap::new();
        components.insert(
            FirmwareComponentType::Bmc,
            FirmwareComponent {
                known_firmware: vec![FirmwareEntry {
                    version: "1.2.3".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        let fw = Firmware {
            vendor: bmc_vendor::BMCVendor::Nvidia,
            model: "TestModel".into(),
            components,
            ..Default::default()
        };
        let models = HashMap::from([("TestModel".into(), fw)]);
        let fw_snapshot = fw_snapshot_from(models);

        let result = get_compute_tray_firmware_version(&id, &bmc, &endpoints, &fw_snapshot);
        let r = result.result.unwrap();
        assert_eq!(r.status, rpc::ComponentManagerStatusCode::Success as i32);
        assert_eq!(result.compute_fw_versions.len(), 1);
        assert_eq!(
            result.compute_fw_versions[0].component,
            rpc::ComputeTrayComponent::Bmc as i32,
        );
        assert_eq!(result.compute_fw_versions[0].versions, vec!["1.2.3"]);
    }
}

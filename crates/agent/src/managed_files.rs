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
use core::str::FromStr;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use backon::{ExponentialBuilder, Retryable};

use std::time::Duration;

use carbide_uuid::machine::{MachineId, MachineInterfaceId};
use forge_dpu_agent_utils::utils::create_forge_client;
use rpc::forge::InterfaceList;
use rpc::forge_tls_client::{ForgeClientConfig, ForgeClientT};

use crate::duppet::{self, FileEnsure, PendingSync, SyncOptions, SyncStatus};
use crate::periodic_config_fetcher::PeriodicConfigFetcher;

async fn get_interface(
    client: &mut ForgeClientT,
    interface: MachineInterfaceId,
) -> Result<InterfaceList, eyre::Error> {
    let request = tonic::Request::new(rpc::forge::InterfaceSearchQuery {
        id: Some(interface),
        ip: None,
    });

    let resp = match client.find_interfaces(request).await {
        Ok(response) => response,
        Err(err) => {
            return Err(eyre::eyre!(
                "Error while executing the FindInterfaces gRPC call: {}",
                err.to_string()
            ));
        }
    };

    Ok(resp.into_inner())
}

fn take_single<T>(mut items: Vec<T>) -> Result<T, eyre::Error> {
    let len = items.len();
    if len != 1 {
        return Err(eyre::eyre!("expected exactly 1 element, found {len}"));
    }
    Ok(items.remove(0))
}

async fn get_host_machine_id(
    fetcher: Arc<PeriodicConfigFetcher>,
    forge_client_config: Arc<ForgeClientConfig>,
    forge_api: &str,
) -> Result<Option<String>, eyre::Error> {
    if let Some(interface_id) = fetcher.get_host_machine_interface_id() {
        let mut client = create_forge_client(forge_api, &forge_client_config).await?;
        let interface_list = get_interface(&mut client, MachineInterfaceId::from_str(&interface_id)?)
            .await
            .map_err(|e| {
                tracing::error!("get_interface({}) failed: {:?}", interface_id, e);
                e
            })?;
        let interface = take_single(interface_list.interfaces)?;
        if let Some(id) = interface.machine_id {
            return Ok(Some(id.to_string()));
        }
    }

    Ok(None)
}

async fn get_host_machine_id_retry(
    fetcher: Arc<PeriodicConfigFetcher>,
    forge_client_config: Arc<ForgeClientConfig>,
    forge_api: &str,
) -> Result<String, eyre::Report> {
    let retry_policy = ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(100))
        .with_max_delay(Duration::from_secs(5 * 60))
        .with_factor(2.0)
        .without_max_times();

    (|| async {
        get_host_machine_id(
            fetcher.clone(),
            forge_client_config.clone(),
            forge_api,
        )
        .await
        .map_err(|e| {
            tracing::warn!("get_host_machine_id() attempt failed: {:?}", e);
            e
        })?
        .ok_or(eyre::eyre!("host_machine_id unavailable, will retry"))
    })
    .retry(retry_policy)
    .await
}

pub fn main_sync(
    sync_options: SyncOptions,
    machine_id: &MachineId,
    periodic_config_fetcher: Arc<PeriodicConfigFetcher>,
    forge_client_config: Arc<ForgeClientConfig>,
    forge_api: String,
) -> io::Result<(HashMap<PathBuf, SyncStatus>, Vec<PendingSync>)> {
    // Sync out all duppet-managed config files. This can be called as part of
    // main_loop running if we want (and can also be called willy nilly with
    // ad-hoc sets of files, including whenever the nvue config changes if we
    // wanted to pull it in), but for now we just do this one duppet sync
    // during setup_and_run. Current files being managed are:
    //
    // - /etc/cron.daily/apt-clean
    // - /etc/dhcp/dhclient-exit-hooks.d/ntpsec
    // - /run/otelcol-contrib/machine-id
    // - /run/otelcol-contrib/host-machine-id
    let duppet_files: HashMap<PathBuf, duppet::FileSpec> = HashMap::from([
        (
            "/etc/cron.daily/apt-clean".into(),
            duppet::FileSpec::new()
                .with_content(include_str!("../templates/apt-clean"))
                .with_perms(0o755),
        ),
        (
            "/etc/dhcp/dhclient-exit-hooks.d/ntpsec".into(),
            duppet::FileSpec::new()
                .with_content(include_str!("../templates/ntpsec"))
                .with_perms(0o644),
        ),
        (
            "/lib/systemd/system/update-ovs-pipe-size.service".into(),
            duppet::FileSpec::new()
                .with_content(include_str!("../templates/update-ovs-pipe-size.service"))
                .with_perms(0o644),
        ),
        (
            "/opt/forge/update-ovs-pipe-size.sh".into(),
            duppet::FileSpec::new()
                .with_content(include_str!("../templates/update-ovs-pipe-size"))
                .with_perms(0o755)
                .with_exec_on_change(),
        ),
        (
            "/run/otelcol-contrib/machine-id".into(),
            duppet::FileSpec::new().with_content(build_otel_machine_id_file_content(machine_id)),
        ),
        (
            "/run/otelcol-contrib/host-machine-id".into(),
            duppet::FileSpec::new().with_content(|| async move {
                let id = get_host_machine_id_retry(
                    periodic_config_fetcher.clone(),
                    forge_client_config.clone(),
                    &forge_api,
                )
                .await;
                build_otel_host_machine_id_file_content(id.ok().as_deref().unwrap_or(""))
            }),
        ),
        // September 30, 2025.
        //
        // /etc/rc.local was added as a workaround for a bug pre-HBN 1.5,
        // which was fixed a couple of years ago. Having this hanging around
        // wasn't a problem until now: as of DOCA 2.9.3, the DPU now uses
        // networkd to manage DHCP leases, meaning we need to stop running
        // dhclient -- both are managing leases at the same time. Kind of a
        // creative way to have redundancy, but not quite what we want!
        //
        // This itself can go in some number of weeks, once the build
        // this is a part of gets deployed everywhere, and this file is
        // cleaned up.
        //
        // https://jirasw.nvidia.com/browse/FORGE-7062
        (
            "/etc/rc.local".into(),
            duppet::FileSpec::new().with_ensure(FileEnsure::Absent),
        ),
    ]);

    duppet::sync(duppet_files, sync_options)
}

// Write "machine.id=<value>" to a file so the OpenTelemetry collector can apply it as a resource
// attribute.
pub fn build_otel_machine_id_file_content(machine_id: &MachineId) -> String {
    format!("machine.id={machine_id}\n")
}

// Write "host.machine.id=<value>" to a file so the OpenTelemetry collector can apply it as a
// resource attribute.
pub fn build_otel_host_machine_id_file_content(host_machine_id: &str) -> String {
    format!("host.machine.id={host_machine_id}\n")
}

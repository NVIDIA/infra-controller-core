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

use std::net::SocketAddr;
use std::path::PathBuf;
use std::{env, fs};

use tempfile::{NamedTempFile, TempDir};

use crate::Options;

// TODO: Add settings to config file and switch this to true
// Then assert that it works
const AGENT_CONFIG: &str = r#"
[forge-system]
api-server = "https://$API_SERVER"
pxe-server = "http://127.0.0.1:8080"
root-ca = "$ROOT_DIR/dev/certs/forge_root.pem"

[machine]
is-fake-dpu = true
interface-id = "f377ed72-d912-4879-958a-8d1f82a50d62"
mac-address = "11:22:33:44:55:66"
hostname = "abc.forge.example.com"

[hbn]
root-dir = "$HBN_ROOT"
skip-reload = true

[period]
main-loop-active-secs = 1
network-config-fetch-secs = 1
main-loop-idle-secs = 30
version-check-secs = 600
inventory-update-secs = 3600
discovery-retry-secs = 1
discovery-retries-max = 1000
"#;

pub fn setup_agent_run_env(
    addr: &SocketAddr,
    td: &TempDir,
    acf: &NamedTempFile,
    test_metadata_service: bool,
) -> eyre::Result<Option<Options>> {
    let Ok(repo_root) = env::var("REPO_ROOT").or_else(|_| env::var("CONTAINER_REPO_ROOT")) else {
        tracing::warn!(
            "Either REPO_ROOT or CONTAINER_REPO_ROOT need to be set to run this test. Skipping."
        );
        return Ok(None);
    };
    let root_dir = PathBuf::from(repo_root);

    unsafe {
        env::set_var("DISABLE_TLS_ENFORCEMENT", "true");
        env::set_var("IGNORE_MGMT_VRF", "true");
        env::set_var("NO_DPU_CONTAINERS", "true");

        // Put our fake `crictl` on front of path so that HBN health checks succeed
        let dev_bin = root_dir.join("dev/bin");
        if let Some(path) = env::var_os("PATH") {
            let mut paths = env::split_paths(&path).collect::<Vec<_>>();
            paths.insert(0, dev_bin);
            let new_path = env::join_paths(paths)?;
            env::set_var("PATH", new_path);
        }
    }

    let hbn_root = td.path();
    tracing::info!("Using hbn_root: {:?}", hbn_root);
    fs::create_dir_all(hbn_root.join("etc/frr"))?;
    fs::create_dir_all(hbn_root.join("etc/network"))?;
    fs::create_dir_all(hbn_root.join("etc/supervisor/conf.d"))?;
    fs::create_dir_all(hbn_root.join("etc/cumulus/acl/policy.d"))?;
    fs::create_dir_all(hbn_root.join("var/support"))?;

    let cfg = AGENT_CONFIG
        .replace("$ROOT_DIR", &root_dir.display().to_string())
        .replace("$HBN_ROOT", &hbn_root.display().to_string())
        .replace("$API_SERVER", &addr.to_string());

    fs::write(acf.path(), cfg)?;
    let opts = crate::Options {
        version: false,
        config_path: Some(acf.path().to_path_buf()),
        cmd: Some(crate::AgentCommand::Run(Box::new(crate::RunOptions {
            enable_metadata_service: test_metadata_service,
            override_machine_id: None,
            override_network_virtualization_type: None,
            skip_upgrade_check: false,
            dhcp_grpc_server: None,
            fmds_grpc_server: None,
            hbn_config_mode: crate::command_line::HbnConfigMode::ContainerExec,
            agent_platform_type: crate::command_line::AgentPlatformType::DpuOs,
            dhcp_server_interface_prepend: None,
        }))),
    };

    Ok(Some(opts))
}

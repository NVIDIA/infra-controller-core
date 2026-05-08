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

use std::path::PathBuf;
use std::{env, fs};

use ::rpc::forge_tls_client::ForgeClientConfig;
use tokio::task::JoinSet;

const ROOT_CERT_PATH: &str = "dev/certs/forge_developer_local_only_root_cert_pem";

#[tokio::test]
async fn test_upgrade_check() -> eyre::Result<()> {
    carbide_host_support::init_logging()?;

    unsafe {
        env::set_var("DISABLE_TLS_ENFORCEMENT", "true");
        env::set_var("IGNORE_MGMT_VRF", "true");
    }

    let root_dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));

    let marker = tempfile::NamedTempFile::new()?;

    let mut join_set = JoinSet::new();
    let mock_server = carbide_agent_mock_api_server::MockApiServer::new();
    let mock_server_handle = mock_server.spawn(&mut join_set).await?;
    let addr = mock_server_handle.addr;

    let client_config =
        ForgeClientConfig::new(root_dir.join(ROOT_CERT_PATH).display().to_string(), None)
            .use_mgmt_vrf()?;

    let upgrade_cmd = format!(
        "echo apt-get install --yes --only-upgrade --reinstall forge-dpu-agent > {}",
        marker.path().display()
    );
    let machine_id = "fm100ht6n80e7do39u8gmt7cvhm89pb32st9ngevgdolu542l1nfa4an0rg".parse()?;
    crate::upgrade::upgrade(
        &format!("https://{addr}"),
        &client_config,
        &machine_id,
        Some(upgrade_cmd).as_deref(),
    )
    .await?;

    assert!(
        fs::read_to_string(marker.path())?.contains("apt-get install"),
        "Upgrade command should have run"
    );

    std::mem::drop(mock_server_handle);
    join_set.join_all().await;

    Ok(())
}

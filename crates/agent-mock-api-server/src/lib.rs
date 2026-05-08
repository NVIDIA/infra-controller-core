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
mod api;
#[allow(dead_code)]
mod generated;

use std::fs;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use api_test_helper::utils::LOCALHOST_CERTS;
use carbide_network::virtualization::VpcVirtualizationType;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tonic::transport::{Identity, Server, ServerTlsConfig};

use crate::generated::forge::forge_server::ForgeServer;

#[derive(Debug)]
pub struct MockApiServer {
    config: MockApiServerConfig,
    state: Arc<MockApiServerState>,
}

#[derive(Clone, Debug)]
pub struct MockApiServerConfig {
    pub virtualization_type: VpcVirtualizationType,
    pub upgrade_response: MockUpgradeCheckResponse,
    pub sitename: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MockUpgradeCheckResponse {
    pub should_upgrade: bool,
    pub package_version: String,
    pub server_version: String,
}

#[derive(Default, Debug)]
struct MockApiServerState {
    has_discovered: AtomicBool,
    has_checked_for_upgrade: AtomicBool,
    num_netconf_fetches: AtomicUsize,
    num_health_reports: AtomicUsize,
    num_get_dpu_ips: AtomicUsize,
}

pub struct MockApiServerHandle {
    pub addr: SocketAddr,
    _shutdown_tx: oneshot::Sender<()>,
    state: Arc<MockApiServerState>,
}

impl Default for MockApiServer {
    fn default() -> Self {
        Self {
            config: Default::default(),
            state: Default::default(),
        }
    }
}

impl Default for MockApiServerConfig {
    fn default() -> Self {
        Self {
            virtualization_type: VpcVirtualizationType::default(),
            upgrade_response: MockUpgradeCheckResponse::upgrade_available(),
            sitename: Some("testsite".to_string()),
        }
    }
}

impl MockUpgradeCheckResponse {
    pub fn upgrade_available() -> Self {
        Self {
            should_upgrade: true,
            package_version: "2024.05-rc3-0".to_string(),
            server_version: "v2024.05-rc3-0".to_string(),
        }
    }

    pub fn no_upgrade_current_build() -> Self {
        let server_version = carbide_version::v!(build_version).to_string();
        Self {
            should_upgrade: false,
            package_version: server_version
                .strip_prefix('v')
                .unwrap_or(&server_version)
                .to_string(),
            server_version,
        }
    }
}

impl MockApiServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_virtualization_type(mut self, virtualization_type: VpcVirtualizationType) -> Self {
        self.config.virtualization_type = virtualization_type;
        self
    }

    pub fn with_upgrade_response(mut self, upgrade_response: MockUpgradeCheckResponse) -> Self {
        self.config.upgrade_response = upgrade_response;
        self
    }

    pub fn with_sitename(mut self, sitename: Option<String>) -> Self {
        self.config.sitename = sitename;
        self
    }

    pub async fn spawn(self, join_set: &mut JoinSet<()>) -> eyre::Result<MockApiServerHandle> {
        let cert = fs::read(&LOCALHOST_CERTS.server_cert)?;
        let key = fs::read(&LOCALHOST_CERTS.server_key)?;
        let identity = Identity::from_pem(cert, key);
        let tls = ServerTlsConfig::new().identity(identity);
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .inspect_err(|crypto_provider| {
                tracing::warn!("Crypto provider already configured: {crypto_provider:?}")
            })
            .ok(); // if something else is already default, ignore.

        let addr = {
            // Pick an open port
            let l = TcpListener::bind("127.0.0.1:0").await?;
            l.local_addr()?
                .to_socket_addrs()?
                .next()
                .expect("No socket available")
        };

        println!("Mock gRPC server listening on {addr}");

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let state = Arc::clone(&self.state);

        join_set.spawn(async move {
            Server::builder()
                .tls_config(tls)
                .expect("TLS config error")
                .add_service(ForgeServer::new(self))
                .serve_with_shutdown(addr, async move {
                    shutdown_rx.await.ok();
                })
                .await
                .expect("Error running mock API server");
        });

        Ok(MockApiServerHandle {
            addr,
            _shutdown_tx: shutdown_tx,
            state,
        })
    }
}

impl MockApiServerHandle {
    pub fn has_discovered(&self) -> bool {
        self.state.has_discovered.load(Ordering::SeqCst)
    }

    pub fn has_checked_for_upgrade(&self) -> bool {
        self.state.has_checked_for_upgrade.load(Ordering::SeqCst)
    }

    pub fn num_netconf_fetches(&self) -> usize {
        self.state.num_netconf_fetches.load(Ordering::SeqCst)
    }

    pub fn num_health_reports(&self) -> usize {
        self.state.num_health_reports.load(Ordering::SeqCst)
    }

    pub fn num_get_dpu_ips(&self) -> usize {
        self.state.num_get_dpu_ips.load(Ordering::SeqCst)
    }
}

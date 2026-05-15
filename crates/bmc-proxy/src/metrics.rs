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

use std::io;
use std::net::SocketAddr;

use metrics_endpoint::{MetricsEndpointConfig, MetricsSetup};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub async fn start(
    address: SocketAddr,
    metrics_setup: MetricsSetup,
    cancellation_token: CancellationToken,
    join_set: &mut JoinSet<()>,
) -> io::Result<()> {
    let listener = TcpListener::bind(&address).await?;
    tracing::info!(%address, "Starting metrics listener");

    join_set
        .build_task()
        .name("bmc-proxy metrics service")
        .spawn(async move {
            metrics_endpoint::run_metrics_endpoint_with_listener(
                &MetricsEndpointConfig {
                    address,
                    registry: metrics_setup.registry,
                    health_controller: Some(metrics_setup.health_controller),
                },
                cancellation_token,
                listener,
            )
            .await
        })
        // Safety: Should only fail if not in a tokio runtime
        .expect("Error spawning metrics endpoint");

    Ok(())
}

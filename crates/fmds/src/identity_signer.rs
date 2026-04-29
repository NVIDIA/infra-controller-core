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

use std::sync::Arc;

use async_trait::async_trait;
use forge_dpu_agent_utils::utils::create_forge_client;
use forge_dpu_fmds_shared::machine_identity::MetaDataIdentitySigner;
use rpc::forge::{MachineIdentityRequest, MachineIdentityResponse};

use crate::state::FmdsState;

#[async_trait]
impl MetaDataIdentitySigner for FmdsState {
    async fn wait_identity_permit(&self) -> Result<(), tonic::Status> {
        let snap = self.machine_identity.load();
        let lim = Arc::clone(&snap.governor);
        let wait = snap.wait_timeout;
        tokio::time::timeout(wait, lim.until_ready())
            .await
            .map_err(|_| {
                tonic::Status::resource_exhausted(
                    "timed out waiting for machine-identity rate limit capacity (machine-identity.wait-timeout-secs)",
                )
            })?;
        Ok(())
    }

    fn sign_proxy_base(&self) -> Option<String> {
        self.machine_identity.load().sign_proxy_base.clone()
    }

    fn sign_proxy_http_client(&self) -> Option<reqwest::Client> {
        self.machine_identity.load().sign_proxy_http_client.clone()
    }

    async fn sign_machine_identity(
        &self,
        audiences: Vec<String>,
    ) -> Result<MachineIdentityResponse, tonic::Status> {
        let forge_client_config = self.forge_client_config.as_ref().ok_or_else(|| {
            tonic::Status::failed_precondition(
                "Forge client TLS is not configured; cannot sign machine identity",
            )
        })?;
        let snap = self.machine_identity.load();
        let timeout = snap.forge_call_timeout;
        tokio::time::timeout(timeout, async {
            let mut client = create_forge_client(&self.forge_api, forge_client_config)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
            client
                .sign_machine_identity(MachineIdentityRequest {
                    audience: audiences,
                })
                .await
                .map(|r| r.into_inner())
        })
        .await
        .map_err(|_| {
            tonic::Status::deadline_exceeded(
                "timed out calling Forge for machine identity (machine-identity.sign-timeout-secs)",
            )
        })?
    }
}

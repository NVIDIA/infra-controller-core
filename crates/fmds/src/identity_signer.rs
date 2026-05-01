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

use async_trait::async_trait;
use axum::http::{HeaderMap, Uri};
use forge_dpu_fmds_shared::machine_identity::{
    MetaDataIdentitySigner, forward_sign_proxy_if_ready, sign_machine_identity_with_forge,
    wait_identity_rate_limit_permit,
};

use crate::state::FmdsState;

#[async_trait]
impl MetaDataIdentitySigner for FmdsState {
    async fn wait_identity_permit(&self) -> Result<(), tonic::Status> {
        let snap = self.machine_identity.load();
        wait_identity_rate_limit_permit(&snap.governor, snap.wait_timeout).await
    }

    async fn forward_sign_proxy_if_configured(
        &self,
        uri: &Uri,
        headers: &HeaderMap,
    ) -> Option<axum::response::Response> {
        let serving = self.machine_identity.load_full();
        forward_sign_proxy_if_ready(
            serving.sign_proxy_base.as_deref(),
            serving.sign_proxy_http_client.as_ref(),
            uri,
            headers,
        )
        .await
    }

    async fn sign_machine_identity(
        &self,
        audiences: Vec<String>,
    ) -> Result<rpc::forge::MachineIdentityResponse, tonic::Status> {
        let forge_client_config = self.forge_client_config.as_ref().ok_or_else(|| {
            tonic::Status::failed_precondition(
                "Forge client TLS is not configured; cannot sign machine identity",
            )
        })?;
        let snap = self.machine_identity.load();
        sign_machine_identity_with_forge(
            &self.forge_api,
            forge_client_config.as_ref(),
            snap.forge_call_timeout,
            audiences,
        )
        .await
    }
}

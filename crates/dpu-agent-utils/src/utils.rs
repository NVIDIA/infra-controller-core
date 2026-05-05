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
use rpc::nico_tls_client::{ApiConfig, NicoClientConfig, NicoClientT, NicoTlsClient};

// Nico Communication
pub async fn create_nico_client(
    nico_api: &str,
    client_config: &NicoClientConfig,
) -> Result<NicoClientT, eyre::Error> {
    match NicoTlsClient::retry_build(&ApiConfig::new(nico_api, client_config)).await {
        Ok(client) => Ok(client),
        Err(err) => Err(eyre::eyre!(
            "Could not connect to Nico API server at {}: {err}",
            nico_api
        )),
    }
}

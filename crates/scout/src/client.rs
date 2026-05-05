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

use ::rpc::nico_tls_client::{self, ApiConfig, NicoClientConfig};
use nico_tls::client_config::ClientCert;
pub use scout::{NicoClientError, NicoClientResult};

use crate::Options;

pub(crate) async fn create_nico_client(
    config: &Options,
) -> NicoClientResult<nico_tls_client::NicoClientT> {
    let client_config = NicoClientConfig::new(
        config.root_ca.clone(),
        Some(ClientCert {
            cert_path: config.client_cert.clone(),
            key_path: config.client_key.clone(),
        }),
    );
    let api_config = ApiConfig::new(&config.api, &client_config);

    let client = nico_tls_client::NicoTlsClient::retry_build(&api_config)
        .await
        .map_err(|err| NicoClientError::TransportError(err.to_string()))?;
    Ok(client)
}

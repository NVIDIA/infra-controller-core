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
use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path, Query};
use axum::http::request::Parts;
use axum_client_ip::ClientIp;
use carbide_uuid::machine::MachineInterfaceId;
use serde::{Deserialize, Serialize};

use crate::common::{MachineInterface, MachineLookup};
use crate::extractors::machine_architecture::MachineArchitecture;
use crate::rpc_error::PxeRequestError;

#[derive(Clone, Serialize, Deserialize, Debug)]
struct MaybeMachineInterface {
    #[serde(rename(deserialize = "buildarch"))]
    build_architecture: String,
    #[serde(default)]
    uuid: Option<MachineInterfaceId>,
    #[serde(default)]
    uuid_as_param: Option<String>,
    #[serde(default)]
    platform: Option<String>,
    #[serde(default)]
    manufacturer: Option<String>,
    #[serde(default)]
    product: Option<String>,
    #[serde(default)]
    serial: Option<String>,
    #[serde(default)]
    asset: Option<String>,
}

impl<S> FromRequestParts<S> for MachineInterface
where
    S: Send + Sync,
{
    type Rejection = PxeRequestError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Ok(maybe) = Query::<MaybeMachineInterface>::from_request_parts(parts, state).await
        else {
            // Query parsing only fails on the required build_architecture
            // field; everything else is optional.
            return Err(PxeRequestError::InvalidBuildArch);
        };
        let mut maybe = maybe.0;
        maybe.uuid_as_param = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .ok()
            .and_then(|params| params.0.get("uuid").cloned());

        let build_architecture = MachineArchitecture::try_from(maybe.build_architecture.as_str())?;

        // Prefer interface_id (DHCP option 43.70 path) when present.
        // Otherwise fall back to the source IP -- ClientIp uses
        // X-Forwarded-For when a proxy injects it, and the TCP socket
        // peer otherwise.
        let lookup = match (maybe.uuid, maybe.uuid_as_param) {
            (Some(uuid), _) => MachineLookup::InterfaceId(uuid),
            (None, Some(uuid)) => MachineLookup::InterfaceId(uuid.parse().map_err(
                |e: carbide_uuid::typed_uuids::UuidError| PxeRequestError::UuidConversion(e.into()),
            )?),
            (None, None) => {
                let client_ip = ClientIp::from_request_parts(parts, state)
                    .await
                    .map_err(PxeRequestError::MissingIp)?
                    .0;
                MachineLookup::SourceIp(client_ip)
            }
        };

        Ok(MachineInterface {
            architecture: Some(build_architecture),
            lookup,
            platform: maybe.platform,
            manufacturer: maybe.manufacturer,
            product: maybe.product,
            serial: maybe.serial,
            asset: maybe.asset,
        })
    }
}

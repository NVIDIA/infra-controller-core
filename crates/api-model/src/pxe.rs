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

use std::net::IpAddr;

use carbide_uuid::machine::MachineInterfaceId;
use rpc::forge::MachineArchitecture;

pub struct PxeInstructionRequest {
    pub arch: MachineArchitecture,
    pub product: Option<String>,
    pub client_ip: IpAddr,
}

impl TryFrom<rpc::forge::PxeInstructionRequest> for PxeInstructionRequest {
    type Error = rpc::errors::RpcDataConversionError;

    fn try_from(value: rpc::forge::PxeInstructionRequest) -> Result<Self, Self::Error> {
        let arch = rpc::forge::MachineArchitecture::try_from(value.arch).map_err(|_| {
            rpc::errors::RpcDataConversionError::InvalidArgument(
                "Unknown arch received.".to_string(),
            )
        })?;

        let client_ip_str =
            value
                .client_ip
                .ok_or(rpc::errors::RpcDataConversionError::MissingArgument(
                    "client_ip",
                ))?;
        let client_ip: IpAddr = client_ip_str.parse().map_err(|e| {
            rpc::errors::RpcDataConversionError::InvalidArgument(format!(
                "Failed parsing client_ip '{client_ip_str}': {e}"
            ))
        })?;

        let product = value.product;

        Ok(PxeInstructionRequest {
            arch,
            product,
            client_ip,
        })
    }
}

/// Input provided to `PxeInstructions::get_pxe_instructions`.
/// The PxeInstructionsRequest model contains the client_ip
/// as determined by carbide-pxe, whereas PxeInstructionsInput
/// contains the resolved machine_interface_id.
pub struct PxeInstructionsInput {
    pub interface_id: MachineInterfaceId,
    pub arch: MachineArchitecture,
    pub product: Option<String>,
}

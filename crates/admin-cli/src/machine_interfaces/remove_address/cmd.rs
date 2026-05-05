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

use ::rpc::admin_cli::NicoCliResult;
use ::rpc::nico as nicorpc;

use super::args::Args;
use crate::rpc::ApiClient;

pub async fn handle_remove_address(args: Args, api_client: &ApiClient) -> NicoCliResult<()> {
    let resp = api_client
        .0
        .remove_static_address(nicorpc::RemoveStaticAddressRequest {
            interface_id: Some(args.interface_id),
            ip_address: args.ip_address.to_string(),
        })
        .await?;

    match resp.status() {
        nicorpc::RemoveStaticAddressStatus::Removed => {
            println!(
                "Removed static address {} from interface {}",
                resp.ip_address, args.interface_id
            );
        }
        nicorpc::RemoveStaticAddressStatus::NotFound => {
            println!(
                "No static address {} found on interface {}",
                resp.ip_address, args.interface_id
            );
        }
    }

    Ok(())
}

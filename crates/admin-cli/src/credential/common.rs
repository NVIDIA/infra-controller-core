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

use ::rpc::admin_cli::NicoCliError;
use clap::{Parser, ValueEnum};

pub const DEFAULT_IB_FABRIC_NAME: &str = "default";

#[derive(ValueEnum, Parser, Debug, Clone)]
pub enum BmcCredentialType {
    // Site Wide BMC Root Account Credentials
    SiteWideRoot,
    // BMC Specific Root Credentials
    BmcRoot,
    // BMC Specific Nico-Admin Credentials
    BmcNicoAdmin,
}

impl From<BmcCredentialType> for rpc::nico::CredentialType {
    fn from(c_type: BmcCredentialType) -> Self {
        use rpc::nico::CredentialType::*;
        match c_type {
            BmcCredentialType::SiteWideRoot => SiteWideBmcRoot,
            BmcCredentialType::BmcRoot => RootBmcByMacAddress,
            BmcCredentialType::BmcNicoAdmin => BmcNicoAdminByMacAddress,
        }
    }
}

#[derive(ValueEnum, Parser, Debug, Clone)]
pub enum UefiCredentialType {
    Dpu,
    Host,
}

impl From<UefiCredentialType> for rpc::nico::CredentialType {
    fn from(c_type: UefiCredentialType) -> Self {
        use rpc::nico::CredentialType::*;
        match c_type {
            UefiCredentialType::Dpu => DpuUefi,
            UefiCredentialType::Host => HostUefi,
        }
    }
}

pub fn url_validator(url: String) -> Result<String, NicoCliError> {
    let addr = tonic::transport::Uri::try_from(&url)
        .map_err(|_| NicoCliError::GenericError("invalid url".to_string()))?;
    Ok(addr.to_string())
}

pub fn password_validator(s: String) -> Result<String, NicoCliError> {
    // TODO: check password according BMC pwd rule.
    if s.is_empty() {
        return Err(NicoCliError::GenericError("invalid input".to_string()));
    }
    Ok(s)
}

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

use std::str::FromStr;
use std::sync::Arc;

use carbide_uuid::nvlink::NvLinkDomainId;
use carbide_uuid::rack::RackId;
use mac_address::MacAddress;

use crate::HealthError;
use crate::config::StaticBmcEndpoint;
use crate::endpoint::{
    BmcAddr, BmcCredentials, BmcEndpoint, BoxFuture, EndpointMetadata, EndpointSource, MachineData,
    PowerShelfData, SwitchData,
};

pub struct StaticEndpointSource {
    endpoints: Vec<Arc<BmcEndpoint>>,
}

impl StaticEndpointSource {
    pub fn new(endpoints: Vec<BmcEndpoint>) -> Self {
        Self {
            endpoints: endpoints.into_iter().map(Arc::new).collect(),
        }
    }

    pub fn from_config(configs: &[StaticBmcEndpoint]) -> Self {
        let endpoints = configs
            .iter()
            .filter_map(|cfg| {
                let ip = match cfg.ip.parse() {
                    Ok(ip) => ip,
                    Err(error) => {
                        tracing::warn!(?error, ip = ?cfg.ip, "Invalid IP in static endpoint config");
                        return None;
                    }
                };

                let mac = MacAddress::from_str(&cfg.mac).ok()?;

                let metadata = if let Some(power_shelf) = &cfg.power_shelf {
                    let id = power_shelf.id.as_ref().and_then(|id| match id.parse() {
                        Ok(id) => Some(id),
                        Err(error) => {
                            tracing::warn!(
                                ?error,
                                power_shelf_id = ?id,
                                "Invalid power_shelf.id in static endpoint config"
                            );
                            None
                        }
                    });
                    let serial = power_shelf
                        .serial
                        .clone()
                        .or_else(|| power_shelf.id.clone())
                        .unwrap_or_else(|| cfg.mac.clone());

                    Some(EndpointMetadata::PowerShelf(PowerShelfData {
                        id,
                        serial,
                    }))
                } else if let Some(switch) = &cfg.switch {
                    let id = switch.id.as_ref().and_then(|id| match id.parse() {
                        Ok(id) => Some(id),
                        Err(error) => {
                            tracing::warn!(
                                ?error,
                                switch_id = ?id,
                                "Invalid switch.id in static endpoint config"
                            );
                            None
                        }
                    });
                    let serial = switch
                        .serial
                        .clone()
                        .or_else(|| switch.id.clone())
                        .unwrap_or_else(|| cfg.mac.clone());

                    Some(EndpointMetadata::Switch(SwitchData {
                        id,
                        serial,
                        slot_number: switch.slot_number,
                        tray_index: switch.tray_index,
                    }))
                } else if let Some(machine) = &cfg.machine {
                    let machine_id = &machine.id;
                    let nvlink_domain_uuid = machine.nvlink_domain_uuid.as_ref().and_then(|id| {
                        match NvLinkDomainId::from_str(id) {
                            Ok(id) => Some(id),
                            Err(error) => {
                                tracing::warn!(
                                    ?error,
                                    nvlink_domain_uuid = ?id,
                                    "Invalid machine.nvlink_domain_uuid in static endpoint config"
                                );
                                None
                            }
                        }
                    });

                    match machine_id.parse() {
                        Ok(machine_id) => Some(EndpointMetadata::Machine(MachineData {
                            machine_id,
                            machine_serial: machine.serial.clone(),
                            slot_number: machine.slot_number,
                            tray_index: machine.tray_index,
                            nvlink_domain_uuid,
                        })),
                        Err(error) => {
                            tracing::warn!(
                                ?error,
                                ?machine_id,
                                "Invalid machine.id in static endpoint config"
                            );
                            None
                        }
                    }
                } else {
                    None
                };

                let endpoint = BmcEndpoint::with_fixed_credentials(
                    BmcAddr {
                        ip,
                        port: cfg.port,
                        mac,
                    },
                    BmcCredentials::UsernamePassword {
                        username: cfg.username.clone(),
                        password: cfg.password.clone(),
                    },
                    metadata,
                    cfg.rack_id.as_ref().map(|id| RackId::new(id.as_str())),
                );

                Some(Arc::new(endpoint))
            })
            .collect();

        Self { endpoints }
    }
}

impl EndpointSource for StaticEndpointSource {
    fn fetch_bmc_hosts<'a>(&'a self) -> BoxFuture<'a, Result<Vec<Arc<BmcEndpoint>>, HealthError>> {
        Box::pin(async move { Ok(self.endpoints.clone()) })
    }
}

pub struct CompositeEndpointSource {
    sources: Vec<Arc<dyn EndpointSource>>,
}

impl CompositeEndpointSource {
    pub fn new(sources: Vec<Arc<dyn EndpointSource>>) -> Self {
        Self { sources }
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

impl EndpointSource for CompositeEndpointSource {
    fn fetch_bmc_hosts<'a>(&'a self) -> BoxFuture<'a, Result<Vec<Arc<BmcEndpoint>>, HealthError>> {
        Box::pin(async move {
            let mut all = Vec::new();

            for src in &self.sources {
                let mut endpoints = src.fetch_bmc_hosts().await?;
                all.append(&mut endpoints);
            }

            Ok(all)
        })
    }
}

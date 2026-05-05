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

use std::borrow::Cow;

use libmlx::firmware::config::FirmwareFlasherProfile;
use libmlx::profile::error::MlxProfileError;
use libmlx::profile::serialization::SerializableProfile;
use rpc::forge_agent_control_response as fac;
use rpc::forge_agent_control_response::mlx_device_action::{Command as DpaCommandPb, Command};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum OpCode<'a> {
    Noop,
    Unlock {
        key: String,
    },
    ApplyProfile {
        serialized_profile: Option<SerializableProfile>,
    },
    Lock {
        key: String,
    },
    ApplyFirmware {
        profile: Option<Box<Cow<'a, FirmwareFlasherProfile>>>,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DpaCommand<'a> {
    pub op: OpCode<'a>,
}

impl TryFrom<DpaCommandPb> for DpaCommand<'static> {
    type Error = String;

    fn try_from(value: DpaCommandPb) -> Result<Self, Self::Error> {
        Ok(Self {
            op: match value {
                Command::Noop(_) => OpCode::Noop,
                Command::Lock(lock) => OpCode::Lock { key: lock.key },
                Command::Unlock(unlock) => OpCode::Unlock { key: unlock.key },
                Command::ApplyProfile(apply_profile) => OpCode::ApplyProfile {
                    serialized_profile: apply_profile
                        .serialized_profile
                        .map(TryInto::try_into)
                        .transpose()
                        .map_err(|e: MlxProfileError| e.to_string())?,
                },
                Command::ApplyFirmware(apply_firmware) => OpCode::ApplyFirmware {
                    profile: apply_firmware
                        .profile
                        .map(|p| Ok::<_, String>(Box::new(Cow::Owned(p.try_into()?))))
                        .transpose()?,
                },
            },
        })
    }
}

pub struct DpaDeviceCommand<'a> {
    pub pci_name: String,
    pub command: DpaCommand<'a>,
}

impl TryFrom<DpaDeviceCommand<'_>> for fac::MlxDeviceAction {
    type Error = String;

    fn try_from(device_command: DpaDeviceCommand<'_>) -> Result<Self, Self::Error> {
        let command = match device_command.command.op {
            OpCode::Noop => fac::mlx_device_action::Command::Noop(fac::MlxDeviceNoop {}),
            OpCode::Unlock { key } => {
                fac::mlx_device_action::Command::Unlock(fac::MlxDeviceUnlock { key })
            }
            OpCode::ApplyProfile { serialized_profile } => {
                let serialized_profile = serialized_profile
                    .map(|profile| (&profile).try_into())
                    .transpose()
                    .map_err(|e: libmlx::profile::error::MlxProfileError| e.to_string())?;
                fac::mlx_device_action::Command::ApplyProfile(fac::MlxDeviceApplyProfile {
                    serialized_profile,
                })
            }
            OpCode::Lock { key } => {
                fac::mlx_device_action::Command::Lock(fac::MlxDeviceLock { key })
            }
            OpCode::ApplyFirmware { profile } => {
                let profile = profile.map(|profile| (*profile).into_owned().into());
                fac::mlx_device_action::Command::ApplyFirmware(fac::MlxDeviceApplyFirmware {
                    profile,
                })
            }
        };

        Ok(fac::MlxDeviceAction {
            pci_name: device_command.pci_name,
            command: Some(command),
        })
    }
}

impl TryFrom<&fac::MlxDeviceAction> for DpaCommand<'static> {
    type Error = String;

    fn try_from(device_action: &fac::MlxDeviceAction) -> Result<Self, Self::Error> {
        let op = match device_action.command.as_ref() {
            Some(fac::mlx_device_action::Command::Noop(_)) | None => OpCode::Noop,
            Some(fac::mlx_device_action::Command::Lock(lock)) => OpCode::Lock {
                key: lock.key.clone(),
            },
            Some(fac::mlx_device_action::Command::Unlock(unlock)) => OpCode::Unlock {
                key: unlock.key.clone(),
            },
            Some(fac::mlx_device_action::Command::ApplyProfile(apply_profile)) => {
                let serialized_profile = apply_profile
                    .serialized_profile
                    .clone()
                    .map(TryInto::try_into)
                    .transpose()
                    .map_err(|e: libmlx::profile::error::MlxProfileError| e.to_string())?;
                OpCode::ApplyProfile { serialized_profile }
            }
            Some(fac::mlx_device_action::Command::ApplyFirmware(apply_firmware)) => {
                let profile = apply_firmware
                    .profile
                    .clone()
                    .map(TryInto::try_into)
                    .transpose()?
                    .map(|profile| Box::new(Cow::Owned(profile)));
                OpCode::ApplyFirmware { profile }
            }
        };

        Ok(DpaCommand { op })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpa_device_command_converts_to_rpc_lock_action() {
        let action: fac::MlxDeviceAction = DpaDeviceCommand {
            pci_name: "04:00.0".to_string(),
            command: DpaCommand {
                op: OpCode::Lock {
                    key: "secret".to_string(),
                },
            },
        }
        .try_into()
        .unwrap();

        assert_eq!(action.pci_name, "04:00.0");
        assert!(matches!(
            action.command,
            Some(fac::mlx_device_action::Command::Lock(fac::MlxDeviceLock { key }))
                if key == "secret"
        ));
    }

    #[test]
    fn rpc_unlock_action_converts_to_dpa_command() {
        let action = fac::MlxDeviceAction {
            pci_name: "04:00.0".to_string(),
            command: Some(fac::mlx_device_action::Command::Unlock(
                fac::MlxDeviceUnlock {
                    key: "secret".to_string(),
                },
            )),
        };

        let command = DpaCommand::try_from(&action).unwrap();
        assert!(matches!(command.op, OpCode::Unlock { key } if key == "secret"));
    }
}

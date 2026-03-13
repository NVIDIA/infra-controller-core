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

//! CLI subcommands for IPMI channel management operations.

use eyre::Context;

use crate::cmd::channel::{
    self, ChannelAccessType,
};
use crate::transport::IpmiTransport;
use crate::types::PrivilegeLevel;

/// Channel subcommands.
#[derive(clap::Subcommand)]
pub enum ChannelCommand {
    /// Show channel authentication capabilities.
    Authcap {
        /// Channel number to query.
        channel: u8,
        /// Requested privilege level (1=callback, 2=user, 3=operator, 4=admin).
        privilege: u8,
    },
    /// Show channel info.
    Info {
        /// Channel number to query (default: 1, the primary LAN channel).
        #[arg(default_value = "1")]
        channel: u8,
    },
    /// Get channel access settings.
    Getaccess {
        /// Channel number to query.
        channel: u8,
        /// Access type: "non-volatile" or "volatile".
        #[arg(short, long, default_value = "non-volatile")]
        access_type: String,
    },
    /// List cipher suites supported on a channel.
    GetCipherSuites {
        /// Channel number to query (default: 1, the primary LAN channel).
        #[arg(default_value = "1")]
        channel: u8,
    },
}

/// Dispatch a channel subcommand to the appropriate IPMI request.
///
/// # Errors
///
/// Returns an error if the IPMI transport fails or the BMC returns an error
/// completion code.
pub async fn run(
    transport: &mut impl IpmiTransport,
    cmd: ChannelCommand,
) -> eyre::Result<()> {
    match cmd {
        ChannelCommand::Authcap {
            channel: ch,
            privilege,
        } => {
            let priv_level = PrivilegeLevel::try_from(privilege).map_err(|v| {
                eyre::eyre!("invalid privilege level: 0x{v:02X} (use 1-5)")
            })?;

            let caps =
                channel::get_channel_auth_capabilities(transport, ch, priv_level)
                    .await
                    .context("get channel auth capabilities")?;

            println!("Channel number             : {}", caps.channel);
            println!(
                "IPMI v1.5  auth type       : {}",
                if caps.ipmi_v1_5 { "yes" } else { "no" }
            );
            println!(
                "IPMI v2.0  auth type       : {}",
                if caps.ipmi_v2_0 { "yes" } else { "no" }
            );
            println!("Auth type support          : 0x{:02X}", caps.auth_types);
            println!(
                "KG status                  : {}",
                if caps.kg_status {
                    "non-zero"
                } else {
                    "default (all zeroes)"
                }
            );
            println!(
                "Per message authentication : {}",
                if caps.per_message_auth {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!(
                "User level authentication  : {}",
                if caps.user_level_auth {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!(
                "Non-null usernames         : {}",
                if caps.non_null_usernames {
                    "yes"
                } else {
                    "no"
                }
            );
            println!(
                "Null usernames             : {}",
                if caps.null_usernames { "yes" } else { "no" }
            );
            println!(
                "Anonymous login            : {}",
                if caps.anonymous_login {
                    "yes"
                } else {
                    "no"
                }
            );
            println!(
                "OEM ID                     : {:02X}{:02X}{:02X}",
                caps.oem_id[0], caps.oem_id[1], caps.oem_id[2]
            );
            println!("OEM aux data               : 0x{:02X}", caps.oem_aux);
            Ok(())
        }
        ChannelCommand::Info { channel: ch } => {
            let info = channel::get_channel_info(transport, ch)
                .await
                .context("get channel info")?;

            let medium_str = match info.medium_type {
                0x01 => "IPMB (I2C)",
                0x02 => "ICMB v1.0",
                0x03 => "ICMB v0.9",
                0x04 => "802.3 LAN",
                0x05 => "Asynch. Serial/Modem (RS-232)",
                0x06 => "Other LAN",
                0x07 => "PCI SMBus",
                0x08 => "SMBus v1.0/1.1",
                0x09 => "SMBus v2.0",
                0x0C => "System Interface (KCS, SMIC, BT)",
                _ => "unknown",
            };

            let session_str = match info.session_support {
                0 => "session-less",
                1 => "single-session",
                2 => "multi-session",
                3 => "session-based",
                _ => "unknown",
            };

            println!("Channel              : {}", info.channel);
            println!("Medium Type          : {} (0x{:02X})", medium_str, info.medium_type);
            println!(
                "Protocol Type        : 0x{:02X}",
                info.protocol_type
            );
            println!("Session Support      : {session_str}");
            println!("Active Sessions      : {}", info.active_sessions);
            println!(
                "Vendor ID            : {:02X}{:02X}{:02X}",
                info.vendor_id[0], info.vendor_id[1], info.vendor_id[2]
            );
            println!(
                "Aux Channel Info     : {:02X}{:02X}",
                info.aux_info[0], info.aux_info[1]
            );
            Ok(())
        }
        ChannelCommand::Getaccess {
            channel: ch,
            access_type,
        } => {
            let at = match access_type.as_str() {
                "non-volatile" | "nv" => ChannelAccessType::NonVolatile,
                "volatile" | "active" => ChannelAccessType::Volatile,
                other => eyre::bail!(
                    "unknown access type: {other} (use 'non-volatile' or 'volatile')"
                ),
            };

            let access = channel::get_channel_access(transport, ch, at)
                .await
                .context("get channel access")?;

            let mode_str = match access.access_mode {
                0 => "disabled",
                1 => "pre-boot only",
                2 => "always available",
                3 => "shared",
                _ => "unknown",
            };

            println!("Channel {ch} access ({access_type}):");
            println!(
                "  Alerting           : {}",
                if access.alerting_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!(
                "  Per-message auth   : {}",
                if access.per_message_auth {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!(
                "  User level auth    : {}",
                if access.user_level_auth {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!("  Access mode        : {mode_str}");
            println!("  Privilege limit    : {}", access.privilege_limit);
            Ok(())
        }
        ChannelCommand::GetCipherSuites { channel: ch } => {
            let data =
                channel::get_channel_cipher_suites(transport, ch)
                    .await
                    .context("get channel cipher suites")?;

            if data.is_empty() {
                println!("No cipher suites reported for channel {ch}");
                return Ok(());
            }

            // Parse the raw cipher suite record data. Each record starts with
            // a tag byte: 0xC0 = standard cipher suite ID follows.
            // We extract just the cipher suite IDs for display.
            let mut ids: Vec<u8> = Vec::new();
            let mut i = 0;
            while i < data.len() {
                if data[i] == 0xC0 && i + 1 < data.len() {
                    ids.push(data[i + 1]);
                    // Skip the tag + ID, then any algorithm tags (0x00-0x3F range
                    // are algorithm tags that follow the suite ID).
                    i += 2;
                } else {
                    i += 1;
                }
            }

            println!("Cipher suites for channel {ch}:");
            for id in &ids {
                println!("  ID: {id}");
            }
            Ok(())
        }
    }
}

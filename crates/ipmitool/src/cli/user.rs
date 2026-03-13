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

//! CLI subcommands for IPMI user management.

use eyre::Context;

use crate::cmd::user;
use crate::transport::IpmiTransport;
use crate::types::PrivilegeLevel;

/// User management subcommands.
#[derive(clap::Subcommand)]
pub enum UserCommand {
    /// List all users on a channel.
    List {
        #[arg(short, long, default_value = "1")]
        channel: u8,
    },
    /// Get a user name by ID.
    Get { user_id: u8 },
    /// Set a user name.
    Set { user_id: u8, name: String },
    /// Set a user password.
    Password {
        user_id: u8,
        #[arg(long, default_value = "false")]
        twenty_byte: bool,
    },
    /// Enable a user.
    Enable { user_id: u8 },
    /// Disable a user.
    Disable { user_id: u8 },
    /// Set user privilege level on a channel.
    Priv {
        user_id: u8,
        privilege: String,
        #[arg(short, long, default_value = "1")]
        channel: u8,
    },
}

/// Dispatch a user subcommand to the appropriate IPMI request.
///
/// # Errors
///
/// Returns an error if the IPMI transport fails or the BMC returns an error
/// completion code.
pub async fn run(
    transport: &mut impl IpmiTransport,
    cmd: UserCommand,
) -> eyre::Result<()> {
    match cmd {
        UserCommand::List { channel } => {
            let summary = user::get_user_summary(transport, channel)
                .await
                .context("get user summary")?;

            println!(
                "Maximum User IDs     : {}",
                summary.max_user_ids
            );
            println!(
                "Enabled User Count   : {}",
                summary.enabled_user_count
            );
            println!(
                "Fixed Name Count     : {}",
                summary.fixed_name_count
            );
            println!();

            // Print a table header.
            println!(
                "{:<4} {:<17} {:<6} {:<6} {:<15}",
                "ID", "Name", "Enable", "IPMI", "Privilege"
            );

            for uid in 1..=summary.max_user_ids {
                // Attempt to get the user name. Some slots may return an
                // error (e.g., if the user ID is not configured); skip those.
                let name = match user::get_user_name(transport, uid).await {
                    Ok(n) => n,
                    Err(_) => continue,
                };

                let access =
                    match user::get_user_access(transport, channel, uid).await {
                        Ok(a) => a,
                        Err(_) => continue,
                    };

                let enabled = if access.ipmi_msg_enabled {
                    "true"
                } else {
                    "false"
                };
                let ipmi = if access.ipmi_msg_enabled {
                    "true"
                } else {
                    "false"
                };

                println!(
                    "{:<4} {:<17} {:<6} {:<6} {:<15}",
                    uid, name, enabled, ipmi, access.privilege_limit
                );
            }

            Ok(())
        }
        UserCommand::Get { user_id } => {
            let name = user::get_user_name(transport, user_id)
                .await
                .context("get user name")?;
            println!("User ID {user_id}: {name}");
            Ok(())
        }
        UserCommand::Set { user_id, name } => {
            user::set_user_name(transport, user_id, &name)
                .await
                .context("set user name")?;
            println!("Set user {user_id} name to: {name}");
            Ok(())
        }
        UserCommand::Password {
            user_id,
            twenty_byte,
        } => {
            // TODO: Read password interactively from a TTY or via a
            // secure input mechanism. For now, print a placeholder message
            // since we cannot do interactive input in all contexts.
            eprintln!(
                "Password input not yet implemented. \
                 Use set_user_password programmatically."
            );

            // Placeholder: set an empty password to demonstrate the flow.
            user::set_user_password(transport, user_id, "", twenty_byte)
                .await
                .context("set user password")?;
            println!("Set password for user {user_id}");
            Ok(())
        }
        UserCommand::Enable { user_id } => {
            user::enable_user(transport, user_id)
                .await
                .context("enable user")?;
            println!("Enabled user {user_id}");
            Ok(())
        }
        UserCommand::Disable { user_id } => {
            user::disable_user(transport, user_id)
                .await
                .context("disable user")?;
            println!("Disabled user {user_id}");
            Ok(())
        }
        UserCommand::Priv {
            user_id,
            privilege,
            channel,
        } => {
            let priv_level = parse_privilege(&privilege)?;
            user::set_user_access(transport, channel, user_id, priv_level, true)
                .await
                .context("set user access")?;
            println!(
                "Set user {user_id} privilege to {priv_level} on channel {channel}"
            );
            Ok(())
        }
    }
}

/// Parse a privilege level string into a [`PrivilegeLevel`].
fn parse_privilege(s: &str) -> eyre::Result<PrivilegeLevel> {
    match s.to_lowercase().as_str() {
        "callback" | "1" => Ok(PrivilegeLevel::Callback),
        "user" | "2" => Ok(PrivilegeLevel::User),
        "operator" | "3" => Ok(PrivilegeLevel::Operator),
        "administrator" | "admin" | "4" => Ok(PrivilegeLevel::Administrator),
        "oem" | "5" => Ok(PrivilegeLevel::Oem),
        other => eyre::bail!("unknown privilege level: {other}"),
    }
}

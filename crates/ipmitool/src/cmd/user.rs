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

//! User management commands: get/set user name, password, access control.
//!
//! All commands in this module use [`NetFn::App`] (0x06).

use crate::error::{IpmitoolError, Result};
use crate::transport::IpmiTransport;
use crate::types::{IpmiRequest, NetFn, PrivilegeLevel};

// ==============================================================================
// Types
// ==============================================================================

/// Parsed response from Get User Access (cmd 0x44).
///
/// Contains both the per-user access settings and the channel-wide user
/// summary information (max users, enabled count, fixed name count).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAccess {
    /// Maximum number of user IDs supported on this channel.
    pub max_user_ids: u8,
    /// Number of currently enabled user IDs.
    pub enabled_user_count: u8,
    /// Number of user IDs with fixed names (cannot be changed).
    pub fixed_name_count: u8,
    /// Raw channel access byte for the requested user.
    pub channel_access: u8,
    /// Maximum privilege level the user is allowed on this channel.
    pub privilege_limit: PrivilegeLevel,
    /// Whether link authentication is enabled for this user.
    pub link_auth_enabled: bool,
    /// Whether IPMI messaging is enabled for this user.
    pub ipmi_msg_enabled: bool,
}

/// Summary of user slot usage on a channel, extracted from Get User Access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSummary {
    /// Maximum number of user IDs supported on this channel.
    pub max_user_ids: u8,
    /// Number of currently enabled user IDs.
    pub enabled_user_count: u8,
    /// Number of user IDs with fixed names (cannot be changed).
    pub fixed_name_count: u8,
}

// ==============================================================================
// Commands
// ==============================================================================

/// Get User Name (NetFn=App, Cmd=0x46).
///
/// Returns the user name for the given user ID slot. The BMC returns a
/// 16-byte fixed-length field; trailing null bytes are stripped.
///
/// # Errors
///
/// Returns an error if the transport fails, the BMC returns a non-success
/// completion code, or the response is too short.
pub async fn get_user_name(
    transport: &mut impl IpmiTransport,
    user_id: u8,
) -> Result<String> {
    let req = IpmiRequest::with_data(NetFn::App, 0x46, vec![user_id]);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()?;

    if resp.data.len() < 16 {
        return Err(IpmitoolError::InvalidResponse(format!(
            "Get User Name response too short: expected 16 bytes, got {}",
            resp.data.len()
        )));
    }

    // The name is a 16-byte field, null-padded. Find the first null byte
    // (or take the whole 16 bytes if there are no nulls).
    let name_bytes = &resp.data[..16];
    let end = name_bytes
        .iter()
        .position(|&b| b == 0x00)
        .unwrap_or(16);

    let name = String::from_utf8_lossy(&name_bytes[..end]).into_owned();
    Ok(name)
}

/// Set User Name (NetFn=App, Cmd=0x45).
///
/// Sets the user name for the given user ID slot. The name is truncated
/// or null-padded to exactly 16 bytes.
///
/// # Errors
///
/// Returns an error if the transport fails or the BMC returns a non-success
/// completion code.
pub async fn set_user_name(
    transport: &mut impl IpmiTransport,
    user_id: u8,
    name: &str,
) -> Result<()> {
    // Build a 17-byte request: 1 byte user ID + 16 bytes name (null-padded).
    let mut data = vec![user_id];
    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(16);
    data.extend_from_slice(&name_bytes[..copy_len]);
    // Pad with zeros to fill the 16-byte name field.
    data.resize(17, 0x00);

    let req = IpmiRequest::with_data(NetFn::App, 0x45, data);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()
}

/// Set User Password (NetFn=App, Cmd=0x47).
///
/// Sets, enables, or disables a user's password. This function specifically
/// performs the "set password" operation (operation byte = 0x02).
///
/// If `is_20_byte` is true, the password field is 20 bytes (IPMI v2.0);
/// otherwise it is 16 bytes (IPMI v1.5 compatible).
///
/// # Errors
///
/// Returns an error if the transport fails or the BMC returns a non-success
/// completion code.
pub async fn set_user_password(
    transport: &mut impl IpmiTransport,
    user_id: u8,
    password: &str,
    is_20_byte: bool,
) -> Result<()> {
    let password_len = if is_20_byte { 20 } else { 16 };

    // Byte 0: user ID, with bit 7 indicating 20-byte password format.
    let id_byte = if is_20_byte {
        user_id | 0x80
    } else {
        user_id
    };

    // Byte 1: operation = 0x02 (set password).
    let mut data = vec![id_byte, 0x02];

    // Bytes 2..(2+password_len): password, null-padded.
    let pw_bytes = password.as_bytes();
    let copy_len = pw_bytes.len().min(password_len);
    data.extend_from_slice(&pw_bytes[..copy_len]);
    data.resize(2 + password_len, 0x00);

    let req = IpmiRequest::with_data(NetFn::App, 0x47, data);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()
}

/// Enable User (NetFn=App, Cmd=0x47).
///
/// Enables the user by sending a Set User Password command with
/// operation byte = 0x01.
///
/// # Errors
///
/// Returns an error if the transport fails or the BMC returns a non-success
/// completion code.
pub async fn enable_user(
    transport: &mut impl IpmiTransport,
    user_id: u8,
) -> Result<()> {
    // Operation 0x01 = enable user. No password data needed.
    let data = vec![user_id, 0x01];
    let req = IpmiRequest::with_data(NetFn::App, 0x47, data);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()
}

/// Disable User (NetFn=App, Cmd=0x47).
///
/// Disables the user by sending a Set User Password command with
/// operation byte = 0x00.
///
/// # Errors
///
/// Returns an error if the transport fails or the BMC returns a non-success
/// completion code.
pub async fn disable_user(
    transport: &mut impl IpmiTransport,
    user_id: u8,
) -> Result<()> {
    // Operation 0x00 = disable user. No password data needed.
    let data = vec![user_id, 0x00];
    let req = IpmiRequest::with_data(NetFn::App, 0x47, data);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()
}

/// Get User Access (NetFn=App, Cmd=0x44).
///
/// Returns the access settings for a specific user on a specific channel,
/// along with channel-wide user summary info.
///
/// # Errors
///
/// Returns an error if the transport fails, the BMC returns a non-success
/// completion code, or the response is too short.
pub async fn get_user_access(
    transport: &mut impl IpmiTransport,
    channel: u8,
    user_id: u8,
) -> Result<UserAccess> {
    let data = vec![channel & 0x0F, user_id & 0x3F];
    let req = IpmiRequest::with_data(NetFn::App, 0x44, data);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()?;

    if resp.data.len() < 4 {
        return Err(IpmitoolError::InvalidResponse(format!(
            "Get User Access response too short: expected >= 4 bytes, got {}",
            resp.data.len()
        )));
    }

    let d = &resp.data;

    // Byte 0: [5:0] = max user IDs (6 bits).
    let max_user_ids = d[0] & 0x3F;

    // Byte 1: [5:0] = enabled user count.
    let enabled_user_count = d[1] & 0x3F;

    // Byte 2: [5:0] = fixed name user count.
    let fixed_name_count = d[2] & 0x3F;

    // Byte 3: user access flags.
    // [3:0] = privilege limit, bit 4 = IPMI msg enabled, bit 5 = link auth enabled.
    let channel_access = d[3];
    let priv_nibble = d[3] & 0x0F;
    let privilege_limit = PrivilegeLevel::try_from(priv_nibble).unwrap_or(
        // If the nibble is 0x00 or out of range, default to User (0x0F means
        // "no access" in some BMCs, but we represent it as the lowest level).
        PrivilegeLevel::Callback,
    );
    let ipmi_msg_enabled = d[3] & 0x10 != 0;
    let link_auth_enabled = d[3] & 0x20 != 0;

    Ok(UserAccess {
        max_user_ids,
        enabled_user_count,
        fixed_name_count,
        channel_access,
        privilege_limit,
        link_auth_enabled,
        ipmi_msg_enabled,
    })
}

/// Set User Access (NetFn=App, Cmd=0x43).
///
/// Configures a user's privilege level and messaging capabilities on a channel.
///
/// # Errors
///
/// Returns an error if the transport fails or the BMC returns a non-success
/// completion code.
pub async fn set_user_access(
    transport: &mut impl IpmiTransport,
    channel: u8,
    user_id: u8,
    privilege: PrivilegeLevel,
    enable_ipmi: bool,
) -> Result<()> {
    // Byte 0: [3:0] = channel, bit 4 = enable IPMI messaging,
    //         bit 5 = enable user link auth, bit 7 = enable changing bits.
    let mut byte0 = (channel & 0x0F) | 0x80; // bit 7 = enable changes
    if enable_ipmi {
        byte0 |= 0x10; // bit 4 = IPMI messaging
    }

    // Byte 1: [5:0] = user ID.
    let byte1 = user_id & 0x3F;

    // Byte 2: [3:0] = privilege level limit.
    let byte2: u8 = privilege.into();

    // Byte 3: session limit (0 = no limit, per spec).
    let byte3 = 0x00;

    let data = vec![byte0, byte1, byte2, byte3];
    let req = IpmiRequest::with_data(NetFn::App, 0x43, data);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()
}

/// Get User Summary (via Get User Access, NetFn=App, Cmd=0x44).
///
/// Queries the channel for the maximum number of user slots and how many
/// are currently enabled. This issues the same command as [`get_user_access`]
/// but only returns the summary fields, discarding the per-user bits.
///
/// # Errors
///
/// Returns an error if the transport fails, the BMC returns a non-success
/// completion code, or the response is too short.
pub async fn get_user_summary(
    transport: &mut impl IpmiTransport,
    channel: u8,
) -> Result<UserSummary> {
    // Query user ID 1 to get the channel summary; the per-user fields
    // are ignored.
    let access = get_user_access(transport, channel, 1).await?;
    Ok(UserSummary {
        max_user_ids: access.max_user_ids,
        enabled_user_count: access.enabled_user_count,
        fixed_name_count: access.fixed_name_count,
    })
}

// ==============================================================================
// Tests
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::mock::MockTransport;

    #[tokio::test]
    async fn get_user_name_parses_null_terminated() {
        let mut transport = MockTransport::new();

        // Build response: completion code + 16-byte name field.
        // Name = "admin" followed by 11 null bytes.
        let mut resp = vec![0x00]; // completion code
        resp.extend_from_slice(b"admin");
        resp.resize(1 + 16, 0x00); // pad to 16 bytes of name
        transport.add_response(0x06, 0x46, resp);

        let name = get_user_name(&mut transport, 2)
            .await
            .expect("should parse user name");

        assert_eq!(name, "admin");
    }

    #[tokio::test]
    async fn get_user_name_parses_full_16_bytes() {
        let mut transport = MockTransport::new();

        // A name that fills all 16 bytes with no null terminator.
        let mut resp = vec![0x00]; // completion code
        resp.extend_from_slice(b"exactlysixteench"); // 16 bytes
        transport.add_response(0x06, 0x46, resp);

        let name = get_user_name(&mut transport, 3)
            .await
            .expect("should parse full-length name");

        assert_eq!(name, "exactlysixteench");
    }

    #[tokio::test]
    async fn get_user_name_short_response_errors() {
        let mut transport = MockTransport::new();

        // Only 8 bytes of name data — too short.
        let resp = vec![0x00, 0x61, 0x64, 0x6D, 0x69, 0x6E, 0x00, 0x00, 0x00];
        transport.add_response(0x06, 0x46, resp);

        let result = get_user_name(&mut transport, 2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn set_user_name_sends_padded_request() {
        let mut transport = MockTransport::new();
        transport.add_response(0x06, 0x45, vec![0x00]);

        set_user_name(&mut transport, 4, "test")
            .await
            .expect("set user name should succeed");
    }

    #[tokio::test]
    async fn set_user_name_truncates_long_names() {
        let mut transport = MockTransport::new();
        transport.add_response(0x06, 0x45, vec![0x00]);

        // A name longer than 16 bytes should not cause an error; it gets truncated.
        set_user_name(&mut transport, 4, "this_name_is_way_too_long_for_ipmi")
            .await
            .expect("set user name should succeed with truncation");
    }

    #[tokio::test]
    async fn get_user_access_parses_all_fields() {
        let mut transport = MockTransport::new();

        // Response: completion code + 4 bytes.
        // Byte 0: max_user_ids = 15 (0x0F).
        // Byte 1: enabled_user_count = 3 (0x03).
        // Byte 2: fixed_name_count = 2 (0x02).
        // Byte 3: privilege = ADMINISTRATOR (0x04), IPMI msg enabled (0x10),
        //         link auth enabled (0x20) => 0x34.
        let resp = vec![0x00, 0x0F, 0x03, 0x02, 0x34];
        transport.add_response(0x06, 0x44, resp);

        let access = get_user_access(&mut transport, 1, 2)
            .await
            .expect("should parse user access");

        assert_eq!(access.max_user_ids, 15);
        assert_eq!(access.enabled_user_count, 3);
        assert_eq!(access.fixed_name_count, 2);
        assert_eq!(access.privilege_limit, PrivilegeLevel::Administrator);
        assert!(access.ipmi_msg_enabled);
        assert!(access.link_auth_enabled);
    }

    #[tokio::test]
    async fn get_user_access_parses_minimal_privilege() {
        let mut transport = MockTransport::new();

        // Byte 3: privilege = USER (0x02), no IPMI msg, no link auth => 0x02.
        let resp = vec![0x00, 0x0A, 0x01, 0x01, 0x02];
        transport.add_response(0x06, 0x44, resp);

        let access = get_user_access(&mut transport, 1, 1)
            .await
            .expect("should parse user access");

        assert_eq!(access.privilege_limit, PrivilegeLevel::User);
        assert!(!access.ipmi_msg_enabled);
        assert!(!access.link_auth_enabled);
    }

    #[tokio::test]
    async fn get_user_access_short_response_errors() {
        let mut transport = MockTransport::new();

        // Only 2 data bytes after completion code — too short.
        let resp = vec![0x00, 0x0F, 0x03];
        transport.add_response(0x06, 0x44, resp);

        let result = get_user_access(&mut transport, 1, 2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn enable_user_succeeds() {
        let mut transport = MockTransport::new();
        transport.add_response(0x06, 0x47, vec![0x00]);

        enable_user(&mut transport, 3)
            .await
            .expect("enable user should succeed");
    }

    #[tokio::test]
    async fn disable_user_succeeds() {
        let mut transport = MockTransport::new();
        transport.add_response(0x06, 0x47, vec![0x00]);

        disable_user(&mut transport, 3)
            .await
            .expect("disable user should succeed");
    }

    #[tokio::test]
    async fn set_user_password_16_byte() {
        let mut transport = MockTransport::new();
        transport.add_response(0x06, 0x47, vec![0x00]);

        set_user_password(&mut transport, 2, "secret", false)
            .await
            .expect("set password (16-byte) should succeed");
    }

    #[tokio::test]
    async fn set_user_password_20_byte() {
        let mut transport = MockTransport::new();
        transport.add_response(0x06, 0x47, vec![0x00]);

        set_user_password(&mut transport, 2, "longer_secret_here", true)
            .await
            .expect("set password (20-byte) should succeed");
    }

    #[tokio::test]
    async fn set_user_access_succeeds() {
        let mut transport = MockTransport::new();
        transport.add_response(0x06, 0x43, vec![0x00]);

        set_user_access(&mut transport, 1, 3, PrivilegeLevel::Administrator, true)
            .await
            .expect("set user access should succeed");
    }

    #[tokio::test]
    async fn get_user_summary_extracts_counts() {
        let mut transport = MockTransport::new();

        // Same response format as get_user_access.
        let resp = vec![0x00, 0x0F, 0x05, 0x02, 0x34];
        transport.add_response(0x06, 0x44, resp);

        let summary = get_user_summary(&mut transport, 1)
            .await
            .expect("should parse user summary");

        assert_eq!(summary.max_user_ids, 15);
        assert_eq!(summary.enabled_user_count, 5);
        assert_eq!(summary.fixed_name_count, 2);
    }

    #[tokio::test]
    async fn enable_user_non_success_completion_code() {
        let mut transport = MockTransport::new();
        // Insufficient privilege (0xD4).
        transport.add_response(0x06, 0x47, vec![0xD4]);

        let result = enable_user(&mut transport, 3).await;
        assert!(result.is_err());
    }
}

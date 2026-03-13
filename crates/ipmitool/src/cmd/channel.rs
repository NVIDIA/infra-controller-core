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

//! Channel management commands: auth capabilities, access control, info,
//! cipher suites.
//!
//! All commands in this module use [`NetFn::App`] (0x06).

use crate::error::{IpmitoolError, Result};
use crate::transport::IpmiTransport;
use crate::types::{IpmiRequest, NetFn, PrivilegeLevel};

// ==============================================================================
// Types
// ==============================================================================

/// Whether to query non-volatile (persistent) or volatile (active) settings
/// when reading channel access configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelAccessType {
    /// Get non-volatile (persistent) settings.
    NonVolatile,
    /// Get current volatile (active) settings.
    Volatile,
}

impl ChannelAccessType {
    /// Encode as the two-bit access type field in byte 2 bits [7:6] of the
    /// Get Channel Access request.
    fn as_access_bits(self) -> u8 {
        match self {
            Self::NonVolatile => 0x40,
            Self::Volatile => 0x80,
        }
    }
}

/// Parsed response from Get Channel Authentication Capabilities (cmd 0x38).
#[derive(Debug, Clone)]
pub struct ChannelAuthCapabilities {
    /// Channel number the capabilities apply to.
    pub channel: u8,
    /// BMC supports IPMI v1.5 sessions on this channel.
    pub ipmi_v1_5: bool,
    /// BMC supports IPMI v2.0 / RMCP+ sessions on this channel.
    pub ipmi_v2_0: bool,
    /// Bitmask of supported IPMI v1.5 authentication types.
    pub auth_types: u8,
    /// Whether a non-null KG key is configured.
    pub kg_status: bool,
    /// Per-message authentication is enabled.
    pub per_message_auth: bool,
    /// User-level authentication is enabled.
    pub user_level_auth: bool,
    /// Non-null usernames are supported.
    pub non_null_usernames: bool,
    /// Null usernames are supported.
    pub null_usernames: bool,
    /// Anonymous login is supported.
    pub anonymous_login: bool,
    /// OEM ID (3 bytes).
    pub oem_id: [u8; 3],
    /// OEM auxiliary data byte.
    pub oem_aux: u8,
}

/// Channel access settings (alerting, authentication, access mode, privilege).
#[derive(Debug, Clone)]
pub struct ChannelAccess {
    /// PEF alerting is enabled on this channel.
    pub alerting_enabled: bool,
    /// Per-message authentication is enabled.
    pub per_message_auth: bool,
    /// User-level authentication is enabled.
    pub user_level_auth: bool,
    /// Access mode: 0=disabled, 1=pre-boot only, 2=always available, 3=shared.
    pub access_mode: u8,
    /// Maximum privilege level allowed on this channel.
    pub privilege_limit: PrivilegeLevel,
}

/// Parsed response from Get Channel Info (cmd 0x42).
#[derive(Debug, Clone)]
pub struct ChannelInfo {
    /// Actual channel number.
    pub channel: u8,
    /// Channel medium type (e.g. 0x01=IPMB, 0x04=LAN 802.3).
    pub medium_type: u8,
    /// Channel protocol type (e.g. 0x01=IPMB-1.0).
    pub protocol_type: u8,
    /// Session support: 0=session-less, 1=single-session, 2=multi-session,
    /// 3=session-based.
    pub session_support: u8,
    /// Number of active sessions on this channel.
    pub active_sessions: u8,
    /// Vendor ID (3 bytes, IANA enterprise number).
    pub vendor_id: [u8; 3],
    /// Auxiliary channel info (2 bytes).
    pub aux_info: [u8; 2],
}

// ==============================================================================
// Commands
// ==============================================================================

/// Get Channel Authentication Capabilities (NetFn=App, Cmd=0x38).
///
/// Queries the BMC for which authentication types, IPMI versions, and login
/// modes are supported on the given channel at the requested privilege level.
///
/// # Errors
///
/// Returns an error if the transport fails, the BMC returns a non-success
/// completion code, or the response is too short.
pub async fn get_channel_auth_capabilities(
    transport: &mut impl IpmiTransport,
    channel: u8,
    privilege: PrivilegeLevel,
) -> Result<ChannelAuthCapabilities> {
    // Byte 0: bits [7] = 1 to request IPMI v2.0 extended data,
    //         bits [3:0] = channel number.
    let byte0 = 0x80 | (channel & 0x0F);
    let byte1: u8 = privilege.into();

    let req = IpmiRequest::with_data(NetFn::App, 0x38, vec![byte0, byte1]);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()?;

    // Response data (after completion code): 8 bytes minimum for IPMI v2.0.
    if resp.data.len() < 8 {
        return Err(IpmitoolError::InvalidResponse(format!(
            "Get Channel Auth Capabilities response too short: expected >= 8 bytes, got {}",
            resp.data.len()
        )));
    }

    let d = &resp.data;
    Ok(ChannelAuthCapabilities {
        channel: d[0] & 0x0F,
        // Byte 1: authentication type support for IPMI v1.5.
        auth_types: d[1],
        // Byte 2: authentication status bits.
        //   bit 5 = KG status (non-null KG)
        //   bit 4 = per-message authentication disabled (inverted sense)
        //   bit 3 = user-level authentication disabled (inverted sense)
        //   bit 2 = non-null usernames supported
        //   bit 1 = null usernames supported
        //   bit 0 = anonymous login supported
        kg_status: (d[2] & 0x20) != 0,
        per_message_auth: (d[2] & 0x10) == 0,
        user_level_auth: (d[2] & 0x08) == 0,
        non_null_usernames: (d[2] & 0x04) != 0,
        null_usernames: (d[2] & 0x02) != 0,
        anonymous_login: (d[2] & 0x01) != 0,
        // Byte 3: extended capabilities.
        //   bit 1 = IPMI v2.0 supported
        //   bit 0 = IPMI v1.5 supported
        ipmi_v2_0: (d[3] & 0x02) != 0,
        ipmi_v1_5: (d[3] & 0x01) != 0,
        // Bytes 4-6: OEM ID, byte 7: OEM auxiliary data.
        oem_id: [d[4], d[5], d[6]],
        oem_aux: d[7],
    })
}

/// Get Channel Access (NetFn=App, Cmd=0x41).
///
/// Reads the access settings (alerting, authentication, privilege limit)
/// for the given channel, either the non-volatile or volatile configuration.
///
/// # Errors
///
/// Returns an error if the transport fails, the BMC returns a non-success
/// completion code, or the response is too short.
pub async fn get_channel_access(
    transport: &mut impl IpmiTransport,
    channel: u8,
    access_type: ChannelAccessType,
) -> Result<ChannelAccess> {
    // Byte 0: channel number (bits [3:0]).
    // Byte 1: bits [7:6] = access type selector, rest reserved.
    let data = vec![channel & 0x0F, access_type.as_access_bits()];

    let req = IpmiRequest::with_data(NetFn::App, 0x41, data);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()?;

    if resp.data.len() < 2 {
        return Err(IpmitoolError::InvalidResponse(format!(
            "Get Channel Access response too short: expected >= 2 bytes, got {}",
            resp.data.len()
        )));
    }

    let d = &resp.data;

    // Byte 0: access settings.
    //   bit 5 = PEF alerting disabled (inverted)
    //   bit 4 = per-message auth disabled (inverted)
    //   bit 3 = user-level auth disabled (inverted)
    //   bits [2:0] = access mode
    let alerting_enabled = (d[0] & 0x20) == 0;
    let per_message_auth = (d[0] & 0x10) == 0;
    let user_level_auth = (d[0] & 0x08) == 0;
    let access_mode = d[0] & 0x07;

    // Byte 1: bits [3:0] = channel privilege level limit.
    let priv_raw = d[1] & 0x0F;
    let privilege_limit = PrivilegeLevel::try_from(priv_raw).map_err(|val| {
        IpmitoolError::InvalidResponse(format!(
            "invalid privilege level in channel access response: 0x{val:02X}"
        ))
    })?;

    Ok(ChannelAccess {
        alerting_enabled,
        per_message_auth,
        user_level_auth,
        access_mode,
        privilege_limit,
    })
}

/// Set Channel Access (NetFn=App, Cmd=0x40).
///
/// Writes the channel access settings. When `persist` is true, the settings
/// are written to both the volatile (active) and non-volatile (persistent)
/// configuration. When false, only the volatile settings are changed.
///
/// # Errors
///
/// Returns an error if the transport fails or the BMC returns a non-success
/// completion code.
pub async fn set_channel_access(
    transport: &mut impl IpmiTransport,
    channel: u8,
    access: &ChannelAccess,
    persist: bool,
) -> Result<()> {
    // Byte 0: channel number (bits [3:0]).
    // Byte 1: bits [7:6] = access set selector.
    //   0b10 = set volatile (active) settings
    //   0b01 = set non-volatile settings
    //   0b11 = set both (not all BMCs support this, so we issue two requests)
    //
    // We always set volatile; if persist is requested, we also set non-volatile
    // by issuing a second command to avoid compatibility issues with BMCs that
    // reject the combined 0b11 selector.

    let access_byte = encode_access_byte(access);
    let priv_byte: u8 = access.privilege_limit.into();

    // Set volatile (active) settings: access set bits = 0b10 (0x80).
    let data_volatile = vec![
        channel & 0x0F,
        0x80 | access_byte,
        0x80 | (priv_byte & 0x0F),
    ];
    let req = IpmiRequest::with_data(NetFn::App, 0x40, data_volatile);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()?;

    if persist {
        // Set non-volatile settings: access set bits = 0b01 (0x40).
        let data_nv = vec![
            channel & 0x0F,
            0x40 | access_byte,
            0x40 | (priv_byte & 0x0F),
        ];
        let req = IpmiRequest::with_data(NetFn::App, 0x40, data_nv);
        let resp = transport.send_recv(&req).await?;
        resp.check_completion()?;
    }

    Ok(())
}

/// Encode the access settings portion of byte 1 for Set Channel Access.
///
/// Bits [5:3] are the inverted enable flags, bits [2:0] are the access mode.
fn encode_access_byte(access: &ChannelAccess) -> u8 {
    let mut byte = access.access_mode & 0x07;
    if !access.alerting_enabled {
        byte |= 0x20;
    }
    if !access.per_message_auth {
        byte |= 0x10;
    }
    if !access.user_level_auth {
        byte |= 0x08;
    }
    byte
}

/// Get Channel Info (NetFn=App, Cmd=0x42).
///
/// Returns the medium type, protocol, session support, and vendor information
/// for the specified channel.
///
/// # Errors
///
/// Returns an error if the transport fails, the BMC returns a non-success
/// completion code, or the response is too short.
pub async fn get_channel_info(
    transport: &mut impl IpmiTransport,
    channel: u8,
) -> Result<ChannelInfo> {
    let req = IpmiRequest::with_data(NetFn::App, 0x42, vec![channel & 0x0F]);
    let resp = transport.send_recv(&req).await?;
    resp.check_completion()?;

    if resp.data.len() < 9 {
        return Err(IpmitoolError::InvalidResponse(format!(
            "Get Channel Info response too short: expected >= 9 bytes, got {}",
            resp.data.len()
        )));
    }

    let d = &resp.data;
    Ok(ChannelInfo {
        channel: d[0] & 0x0F,
        medium_type: d[1] & 0x7F,
        protocol_type: d[2] & 0x1F,
        // Byte 3: bits [7:6] = session support, bits [5:0] = active session count.
        session_support: (d[3] >> 6) & 0x03,
        active_sessions: d[3] & 0x3F,
        vendor_id: [d[4], d[5], d[6]],
        aux_info: [d[7], d[8]],
    })
}

/// Get Channel Cipher Suites (NetFn=App, Cmd=0x54).
///
/// Iterates over the BMC's cipher suite records for the given channel and
/// returns the raw cipher suite ID bytes. Each request returns up to 16 bytes
/// of cipher suite data; we increment `list_index` until the BMC returns
/// fewer than 16 bytes, signaling the end of the list.
///
/// # Errors
///
/// Returns an error if the transport fails or the BMC returns a non-success
/// completion code.
pub async fn get_channel_cipher_suites(
    transport: &mut impl IpmiTransport,
    channel: u8,
) -> Result<Vec<u8>> {
    let mut all_data: Vec<u8> = Vec::new();

    // The maximum number of 16-byte chunks we will fetch before assuming the
    // BMC is misbehaving. IPMI defines at most ~20 standard cipher suites,
    // so 16 iterations (256 bytes) is very generous.
    const MAX_ITERATIONS: u8 = 16;

    for list_index in 0..MAX_ITERATIONS {
        // Byte 0: channel number (bits [3:0]).
        // Byte 1: payload type = 0x00 (IPMI).
        // Byte 2: bits [7:6] = list type (0b10 = list by cipher suite),
        //         bits [5:0] = list index.
        let byte2 = 0x80 | (list_index & 0x3F);
        let data = vec![channel & 0x0F, 0x00, byte2];

        let req = IpmiRequest::with_data(NetFn::App, 0x54, data);
        let resp = transport.send_recv(&req).await?;
        resp.check_completion()?;

        // Byte 0 of response data is the channel number; the remaining
        // bytes are cipher suite record data.
        let payload = if resp.data.len() > 1 {
            &resp.data[1..]
        } else {
            // No cipher suite data in this response — we are done.
            break;
        };

        all_data.extend_from_slice(payload);

        // If we received fewer than 16 bytes of cipher suite data, the
        // BMC has no more records.
        if payload.len() < 16 {
            break;
        }
    }

    Ok(all_data)
}

// ==============================================================================
// Tests
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::mock::{MockTransport, QueueMockTransport};

    #[tokio::test]
    async fn get_channel_auth_capabilities_v2() {
        let mut transport = MockTransport::new();

        // Build a response for Get Channel Auth Capabilities (cmd 0x38):
        //   completion code (0x00) + 8 data bytes.
        let resp = vec![
            0x00, // completion code
            0x01, // channel 1
            0x16, // auth types: MD5 + password + none
            0x04, // non-null usernames, per-msg auth enabled, user-level auth enabled
            0x02, // IPMI v2.0 supported (bit 1), v1.5 not (bit 0 clear)
            0x00, 0x00, 0x00, // OEM ID
            0x00, // OEM aux
        ];
        transport.add_response(0x06, 0x38, resp);

        let caps = get_channel_auth_capabilities(
            &mut transport,
            1,
            PrivilegeLevel::Administrator,
        )
        .await
        .expect("should parse auth capabilities");

        assert_eq!(caps.channel, 1);
        assert!(caps.ipmi_v2_0);
        assert!(!caps.ipmi_v1_5);
        assert!(caps.non_null_usernames);
        assert!(!caps.null_usernames);
        assert!(!caps.anonymous_login);
        assert!(caps.per_message_auth);
        assert!(caps.user_level_auth);
        assert!(!caps.kg_status);
        assert_eq!(caps.auth_types, 0x16);
    }

    #[tokio::test]
    async fn get_channel_auth_capabilities_short_response() {
        let mut transport = MockTransport::new();
        // Only 4 data bytes — too short (need 8).
        transport.add_response(0x06, 0x38, vec![0x00, 0x01, 0x16, 0x04, 0x02]);

        let result = get_channel_auth_capabilities(
            &mut transport,
            1,
            PrivilegeLevel::Administrator,
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_channel_info_parses_lan() {
        let mut transport = MockTransport::new();

        // Response for Get Channel Info (cmd 0x42):
        //   completion code + 9 data bytes.
        let resp = vec![
            0x00, // completion code
            0x01, // channel 1
            0x04, // medium type = LAN 802.3
            0x01, // protocol type = IPMB-1.0
            // Session support (bits [7:6] = 2 = multi-session) +
            // active sessions (bits [5:0] = 3).
            0x83, // 0b10_000011
            0x57, 0x01, 0x00, // vendor ID
            0xAA, 0xBB, // aux info
        ];
        transport.add_response(0x06, 0x42, resp);

        let info = get_channel_info(&mut transport, 1)
            .await
            .expect("should parse channel info");

        assert_eq!(info.channel, 1);
        assert_eq!(info.medium_type, 0x04);
        assert_eq!(info.protocol_type, 0x01);
        assert_eq!(info.session_support, 2);
        assert_eq!(info.active_sessions, 3);
        assert_eq!(info.vendor_id, [0x57, 0x01, 0x00]);
        assert_eq!(info.aux_info, [0xAA, 0xBB]);
    }

    #[tokio::test]
    async fn get_channel_info_short_response() {
        let mut transport = MockTransport::new();
        // Only 5 data bytes — too short (need 9).
        transport.add_response(0x06, 0x42, vec![0x00, 0x01, 0x04, 0x01, 0x83, 0x57]);

        let result = get_channel_info(&mut transport, 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_channel_cipher_suites_multi_packet() {
        let mut transport = QueueMockTransport::new();

        // First response: 16 bytes of cipher suite data (channel byte + 16 payload bytes).
        let mut resp1 = vec![0x00, 0x01]; // completion code + channel
        resp1.extend_from_slice(&[0xC0, 0x17, 0x01, 0x02, 0xC0, 0x03, 0x01, 0x03]);
        resp1.extend_from_slice(&[0xC0, 0x08, 0x02, 0x01, 0xC0, 0x0C, 0x02, 0x03]);
        transport.enqueue(0x06, 0x54, resp1);

        // Second response: fewer than 16 bytes — signals end of list.
        let resp2 = vec![0x00, 0x01, 0xC0, 0x11, 0x03, 0x04]; // 4 cipher bytes
        transport.enqueue(0x06, 0x54, resp2);

        let suites = get_channel_cipher_suites(&mut transport, 1)
            .await
            .expect("should collect cipher suite data");

        // 16 bytes from first packet + 4 from second = 20 total.
        assert_eq!(suites.len(), 20);
        // Verify first byte from each packet.
        assert_eq!(suites[0], 0xC0);
        assert_eq!(suites[16], 0xC0);
    }

    #[tokio::test]
    async fn get_channel_cipher_suites_empty() {
        let mut transport = QueueMockTransport::new();

        // Response with only the channel byte — no cipher suite data.
        transport.enqueue(0x06, 0x54, vec![0x00, 0x01]);

        let suites = get_channel_cipher_suites(&mut transport, 1)
            .await
            .expect("empty cipher suites should succeed");

        assert!(suites.is_empty());
    }

    #[tokio::test]
    async fn get_set_channel_access_roundtrip() {
        let mut transport = QueueMockTransport::new();

        let access = ChannelAccess {
            alerting_enabled: true,
            per_message_auth: true,
            user_level_auth: true,
            access_mode: 2, // always available
            privilege_limit: PrivilegeLevel::Administrator,
        };

        // Set Channel Access: two success responses (volatile + non-volatile).
        transport.enqueue(0x06, 0x40, vec![0x00]);
        transport.enqueue(0x06, 0x40, vec![0x00]);

        // Get Channel Access response.
        // Byte 0: access mode = 2 (always), no disable bits set.
        // Byte 1: privilege limit = 0x04 (Administrator).
        transport.enqueue(0x06, 0x41, vec![0x00, 0x02, 0x04]);

        set_channel_access(&mut transport, 1, &access, true)
            .await
            .expect("set channel access should succeed");

        let read_back = get_channel_access(
            &mut transport,
            1,
            ChannelAccessType::NonVolatile,
        )
        .await
        .expect("get channel access should succeed");

        assert!(read_back.alerting_enabled);
        assert!(read_back.per_message_auth);
        assert!(read_back.user_level_auth);
        assert_eq!(read_back.access_mode, 2);
        assert_eq!(read_back.privilege_limit, PrivilegeLevel::Administrator);
    }

    #[tokio::test]
    async fn set_channel_access_volatile_only() {
        let mut transport = QueueMockTransport::new();

        let access = ChannelAccess {
            alerting_enabled: false,
            per_message_auth: false,
            user_level_auth: false,
            access_mode: 0, // disabled
            privilege_limit: PrivilegeLevel::User,
        };

        // Only one success response needed (volatile only, no persist).
        transport.enqueue(0x06, 0x40, vec![0x00]);

        set_channel_access(&mut transport, 1, &access, false)
            .await
            .expect("set channel access (volatile only) should succeed");
    }

    #[tokio::test]
    async fn encode_access_byte_all_disabled() {
        let access = ChannelAccess {
            alerting_enabled: false,
            per_message_auth: false,
            user_level_auth: false,
            access_mode: 0,
            privilege_limit: PrivilegeLevel::User,
        };
        // All three disable bits set (0x20 | 0x10 | 0x08 = 0x38), mode 0.
        assert_eq!(encode_access_byte(&access), 0x38);
    }

    #[tokio::test]
    async fn encode_access_byte_all_enabled() {
        let access = ChannelAccess {
            alerting_enabled: true,
            per_message_auth: true,
            user_level_auth: true,
            access_mode: 2,
            privilege_limit: PrivilegeLevel::Administrator,
        };
        // No disable bits, mode = 2.
        assert_eq!(encode_access_byte(&access), 0x02);
    }
}

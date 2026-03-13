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

//! RAKP (Remote Authenticated Key-Exchange Protocol) message building and parsing.
//!
//! The RMCP+ session establishment uses a 4-message handshake preceded by an
//! Open Session Request/Response exchange. This module handles serialization
//! and deserialization of all six message types (Open Session Req/Resp +
//! RAKP 1-4), as well as HMAC verification for RAKP 2 and RAKP 4.
//!
//! # Protocol Flow
//!
//! ```text
//! Console                          BMC
//!   │                               │
//!   │── Open Session Request ──────►│  (payload type 0x10)
//!   │◄── Open Session Response ─────│  (payload type 0x11)
//!   │── RAKP Message 1 ───────────►│  (payload type 0x12)
//!   │◄── RAKP Message 2 ───────────│  (payload type 0x13)
//!   │── RAKP Message 3 ───────────►│  (payload type 0x14)
//!   │◄── RAKP Message 4 ───────────│  (payload type 0x15)
//!   │                               │
//!   │  Session is now active.       │
//! ```

use byteorder::{ByteOrder, LittleEndian};

use crate::crypto::hmac_auth;
use crate::error::{IpmitoolError, Result};
use crate::types::{
    AuthAlgorithm, CipherSuiteId, PrivilegeLevel,
};

// ==============================================================================
// RMCP+ Status Codes
// ==============================================================================

/// RMCP+ status codes returned in Open Session Response and RAKP messages.
/// A non-zero status indicates the BMC rejected the request.
fn check_rmcpplus_status(status: u8, context: &str) -> Result<()> {
    if status == 0x00 {
        return Ok(());
    }

    let description = match status {
        0x01 => "insufficient resources to create a session",
        0x02 => "invalid session ID",
        0x03 => "invalid payload type",
        0x04 => "invalid authentication algorithm",
        0x05 => "invalid integrity algorithm",
        0x06 => "no matching authentication payload",
        0x07 => "no matching integrity payload",
        0x09 => "invalid role",
        0x0D => "invalid name length",
        0x0E => "unauthorized name",
        0x0F => "unauthorized GUID",
        0x11 => "invalid integrity check value",
        0x12 => "invalid confidentiality algorithm",
        0x13 => "no cipher suite match with proposed security algorithms",
        0x14 => "illegal or unrecognized parameter",
        _ => "unknown RMCP+ error",
    };

    Err(IpmitoolError::AuthenticationFailed(format!(
        "{context}: status 0x{status:02X} ({description})"
    )))
}

// ==============================================================================
// Open Session Request (payload type 0x10)
// ==============================================================================

/// Build an Open Session Request payload.
///
/// This is the first message in the RMCP+ session establishment, where the
/// console proposes its preferred algorithms (cipher suite) and a session ID.
#[must_use]
pub fn build_open_session_request(
    message_tag: u8,
    privilege: PrivilegeLevel,
    console_session_id: u32,
    cipher_suite: &CipherSuiteId,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(32);

    payload.push(message_tag);
    payload.push(privilege as u8);
    // Reserved (2 bytes).
    payload.extend_from_slice(&[0x00, 0x00]);

    // Console session ID (4 bytes LE).
    let mut sid_buf = [0u8; 4];
    LittleEndian::write_u32(&mut sid_buf, console_session_id);
    payload.extend_from_slice(&sid_buf);

    // Authentication algorithm payload.
    // Format: payload_type(1), reserved(2), payload_length(1), algorithm(1), reserved(3)
    payload.extend_from_slice(&[
        0x00,
        0x00,
        0x00,
        0x08,
        cipher_suite.auth as u8,
        0x00,
        0x00,
        0x00,
    ]);

    // Integrity algorithm payload.
    payload.extend_from_slice(&[
        0x01,
        0x00,
        0x00,
        0x08,
        cipher_suite.integrity as u8,
        0x00,
        0x00,
        0x00,
    ]);

    // Confidentiality algorithm payload.
    payload.extend_from_slice(&[
        0x02,
        0x00,
        0x00,
        0x08,
        cipher_suite.confidentiality as u8,
        0x00,
        0x00,
        0x00,
    ]);

    payload
}

// ==============================================================================
// Open Session Response (payload type 0x11)
// ==============================================================================

/// Parsed Open Session Response.
#[derive(Debug)]
pub struct OpenSessionResponse {
    pub message_tag: u8,
    pub status: u8,
    pub max_privilege: u8,
    pub console_session_id: u32,
    pub managed_session_id: u32,
}

/// Parse an Open Session Response payload.
///
/// # Errors
///
/// Returns an error if the payload is too short or the BMC returned a non-zero
/// status code.
pub fn parse_open_session_response(data: &[u8]) -> Result<OpenSessionResponse> {
    // Minimum length: tag(1) + status(1) + priv(1) + reserved(1) +
    // console_sid(4) + managed_sid(4) = 12 bytes.
    // The response also contains algorithm payloads, but we only need the
    // first 12 bytes for the session IDs.
    if data.len() < 12 {
        return Err(IpmitoolError::InvalidResponse(format!(
            "Open Session Response too short: {} bytes, need at least 12",
            data.len()
        )));
    }

    let response = OpenSessionResponse {
        message_tag: data[0],
        status: data[1],
        max_privilege: data[2],
        // data[3] is reserved
        console_session_id: LittleEndian::read_u32(&data[4..8]),
        managed_session_id: LittleEndian::read_u32(&data[8..12]),
    };

    check_rmcpplus_status(response.status, "Open Session Response")?;

    Ok(response)
}

// ==============================================================================
// RAKP Message 1 (payload type 0x12)
// ==============================================================================

/// Build a RAKP Message 1 payload.
///
/// RAKP 1 sends the console's random number (Rc) and username to the BMC,
/// initiating the key exchange.
#[must_use]
pub fn build_rakp1(
    message_tag: u8,
    managed_session_id: u32,
    rc: &[u8; 16],
    privilege: PrivilegeLevel,
    username: &[u8],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(28 + username.len());

    payload.push(message_tag);
    // Reserved (3 bytes): status=0 + 2 reserved bytes.
    payload.extend_from_slice(&[0x00, 0x00, 0x00]);

    // Managed system session ID (4 bytes LE).
    let mut sid_buf = [0u8; 4];
    LittleEndian::write_u32(&mut sid_buf, managed_session_id);
    payload.extend_from_slice(&sid_buf);

    // Remote console random number Rc (16 bytes).
    payload.extend_from_slice(rc);

    // Requested maximum privilege level (1 byte).
    // Bit 4 set = "name-only lookup" (standard for username/password auth).
    payload.push(privilege as u8 | 0x10);

    // Reserved (2 bytes).
    payload.extend_from_slice(&[0x00, 0x00]);

    // Username length (1 byte).
    payload.push(username.len() as u8);

    // Username (variable).
    payload.extend_from_slice(username);

    payload
}

// ==============================================================================
// RAKP Message 2 (payload type 0x13)
// ==============================================================================

/// Parsed RAKP Message 2 data.
#[derive(Debug)]
pub struct Rakp2Response {
    pub message_tag: u8,
    pub status: u8,
    pub console_session_id: u32,
    /// Managed system random number (Rm, 16 bytes).
    pub rm: [u8; 16],
    /// Managed system GUID (16 bytes).
    pub managed_guid: [u8; 16],
    /// Key exchange authentication code (variable length, depends on auth alg).
    pub key_exchange_auth_code: Vec<u8>,
}

/// Parse a RAKP Message 2 payload.
///
/// # Errors
///
/// Returns an error if the payload is too short or the BMC returned a non-zero
/// status code.
pub fn parse_rakp2(data: &[u8]) -> Result<Rakp2Response> {
    // Minimum: tag(1) + status(1) + reserved(2) + console_sid(4) +
    // Rm(16) + managed_guid(16) = 40 bytes (auth code may follow).
    if data.len() < 40 {
        return Err(IpmitoolError::InvalidResponse(format!(
            "RAKP Message 2 too short: {} bytes, need at least 40",
            data.len()
        )));
    }

    let status = data[1];
    check_rmcpplus_status(status, "RAKP Message 2")?;

    let mut rm = [0u8; 16];
    rm.copy_from_slice(&data[8..24]);

    let mut managed_guid = [0u8; 16];
    managed_guid.copy_from_slice(&data[24..40]);

    let response = Rakp2Response {
        message_tag: data[0],
        status,
        console_session_id: LittleEndian::read_u32(&data[4..8]),
        rm,
        managed_guid,
        key_exchange_auth_code: data[40..].to_vec(),
    };

    Ok(response)
}

/// Parameters for verifying the RAKP Message 2 HMAC.
///
/// Grouped into a struct to keep the function signature readable, since
/// the HMAC input concatenates many fields from different handshake stages.
pub struct Rakp2HmacParams<'a> {
    pub auth_alg: AuthAlgorithm,
    pub password: &'a [u8],
    pub console_session_id: u32,
    pub managed_session_id: u32,
    pub rc: &'a [u8; 16],
    pub rm: &'a [u8; 16],
    pub managed_guid: &'a [u8; 16],
    pub role: u8,
    pub username: &'a [u8],
    pub received_hmac: &'a [u8],
}

/// Verify the HMAC in RAKP Message 2.
///
/// The HMAC is computed over:
///   console_sid(4) || managed_sid(4) || Rc(16) || Rm(16) ||
///   managed_guid(16) || role(1) || username_len(1) || username
///
/// Key = user password.
///
/// # Errors
///
/// Returns an error if the HMAC does not match.
pub fn verify_rakp2_hmac(params: &Rakp2HmacParams<'_>) -> Result<()> {
    let mut hmac_data =
        Vec::with_capacity(4 + 4 + 16 + 16 + 16 + 1 + 1 + params.username.len());

    let mut buf = [0u8; 4];
    LittleEndian::write_u32(&mut buf, params.console_session_id);
    hmac_data.extend_from_slice(&buf);

    LittleEndian::write_u32(&mut buf, params.managed_session_id);
    hmac_data.extend_from_slice(&buf);

    hmac_data.extend_from_slice(params.rc);
    hmac_data.extend_from_slice(params.rm);
    hmac_data.extend_from_slice(params.managed_guid);
    hmac_data.push(params.role);
    hmac_data.push(params.username.len() as u8);
    hmac_data.extend_from_slice(params.username);

    let expected = compute_rakp_hmac(params.auth_alg, params.password, &hmac_data)?;

    if params.received_hmac != expected.as_slice() {
        return Err(IpmitoolError::AuthenticationFailed(
            "RAKP Message 2 HMAC verification failed".to_owned(),
        ));
    }

    Ok(())
}

// ==============================================================================
// RAKP Message 3 (payload type 0x14)
// ==============================================================================

/// Parameters for building RAKP Message 3.
pub struct Rakp3Params<'a> {
    pub message_tag: u8,
    pub managed_session_id: u32,
    pub auth_alg: AuthAlgorithm,
    pub password: &'a [u8],
    pub rm: &'a [u8; 16],
    pub console_session_id: u32,
    pub role: u8,
    pub username: &'a [u8],
}

/// Build a RAKP Message 3 payload.
///
/// The auth code proves the console knows the password:
///   HMAC_password(Rm || console_sid || role || username_len || username)
///
/// # Errors
///
/// Returns an error if the HMAC computation fails.
pub fn build_rakp3(params: &Rakp3Params<'_>) -> Result<Vec<u8>> {
    // Compute the auth code.
    let mut hmac_data = Vec::with_capacity(16 + 4 + 1 + 1 + params.username.len());
    hmac_data.extend_from_slice(params.rm);

    let mut buf = [0u8; 4];
    LittleEndian::write_u32(&mut buf, params.console_session_id);
    hmac_data.extend_from_slice(&buf);

    hmac_data.push(params.role);
    hmac_data.push(params.username.len() as u8);
    hmac_data.extend_from_slice(params.username);

    let auth_code = compute_rakp_hmac(params.auth_alg, params.password, &hmac_data)?;

    // Build the payload.
    let mut payload = Vec::with_capacity(8 + auth_code.len());

    payload.push(params.message_tag);
    // Status = 0 (no error).
    payload.push(0x00);
    // Reserved (2 bytes).
    payload.extend_from_slice(&[0x00, 0x00]);

    // Managed system session ID (4 bytes LE).
    LittleEndian::write_u32(&mut buf, params.managed_session_id);
    payload.extend_from_slice(&buf);

    // Key exchange auth code.
    payload.extend_from_slice(&auth_code);

    Ok(payload)
}

// ==============================================================================
// RAKP Message 4 (payload type 0x15)
// ==============================================================================

/// Parsed RAKP Message 4 data.
#[derive(Debug)]
pub struct Rakp4Response {
    pub message_tag: u8,
    pub status: u8,
    pub console_session_id: u32,
    /// Integrity check value (variable length, depends on auth algorithm).
    pub integrity_check_value: Vec<u8>,
}

/// Parse a RAKP Message 4 payload.
///
/// # Errors
///
/// Returns an error if the payload is too short or the BMC returned a non-zero
/// status code.
pub fn parse_rakp4(data: &[u8]) -> Result<Rakp4Response> {
    // Minimum: tag(1) + status(1) + reserved(2) + console_sid(4) = 8 bytes.
    if data.len() < 8 {
        return Err(IpmitoolError::InvalidResponse(format!(
            "RAKP Message 4 too short: {} bytes, need at least 8",
            data.len()
        )));
    }

    let status = data[1];
    check_rmcpplus_status(status, "RAKP Message 4")?;

    let response = Rakp4Response {
        message_tag: data[0],
        status,
        console_session_id: LittleEndian::read_u32(&data[4..8]),
        integrity_check_value: data[8..].to_vec(),
    };

    Ok(response)
}

/// Verify the integrity check value in RAKP Message 4.
///
/// The ICV is: HMAC_SIK(Rc || managed_sid || managed_guid)
///
/// For SHA256-based auth, the ICV is truncated to the first 16 bytes
/// (HMAC-SHA256-128).
///
/// # Errors
///
/// Returns an error if the ICV does not match.
pub fn verify_rakp4_icv(
    auth_alg: AuthAlgorithm,
    sik: &[u8],
    rc: &[u8; 16],
    managed_session_id: u32,
    managed_guid: &[u8; 16],
    received_icv: &[u8],
) -> Result<()> {
    let mut icv_data = Vec::with_capacity(16 + 4 + 16);
    icv_data.extend_from_slice(rc);

    let mut buf = [0u8; 4];
    LittleEndian::write_u32(&mut buf, managed_session_id);
    icv_data.extend_from_slice(&buf);

    icv_data.extend_from_slice(managed_guid);

    let full_hmac = compute_rakp_hmac(auth_alg, sik, &icv_data)?;

    // The RAKP 4 ICV is truncated based on the auth algorithm:
    // - HMAC-SHA1: first 12 bytes
    // - HMAC-SHA256: first 16 bytes
    // - HMAC-MD5: full 16 bytes (no truncation)
    let expected = match auth_alg {
        AuthAlgorithm::HmacSha1 => &full_hmac[..12],
        AuthAlgorithm::HmacSha256 => &full_hmac[..16],
        AuthAlgorithm::HmacMd5 => &full_hmac,
        AuthAlgorithm::None => return Ok(()),
    };

    if received_icv != expected {
        return Err(IpmitoolError::AuthenticationFailed(
            "RAKP Message 4 integrity check value mismatch".to_owned(),
        ));
    }

    Ok(())
}

// ==============================================================================
// Helpers
// ==============================================================================

/// Compute a RAKP HMAC using the appropriate algorithm.
fn compute_rakp_hmac(
    auth_alg: AuthAlgorithm,
    key: &[u8],
    data: &[u8],
) -> Result<Vec<u8>> {
    match auth_alg {
        AuthAlgorithm::HmacSha1 => hmac_auth::rakp_hmac_sha1(key, data),
        AuthAlgorithm::HmacSha256 => hmac_auth::rakp_hmac_sha256(key, data),
        AuthAlgorithm::HmacMd5 => hmac_auth::rakp_hmac_md5(key, data),
        AuthAlgorithm::None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        cipher_suite_by_id, ConfidentialityAlgorithm, IntegrityAlgorithm,
    };

    // =========================================================================
    // Open Session Request
    // =========================================================================

    #[test]
    fn open_session_request_serialization() {
        let suite = cipher_suite_by_id(17).expect("suite 17 exists");
        let payload = build_open_session_request(
            0x00,
            PrivilegeLevel::Administrator,
            0xDEADBEEF,
            &suite,
        );

        // Total length: 8 (header) + 8 (auth) + 8 (integrity) + 8 (conf) = 32
        assert_eq!(payload.len(), 32);

        // Message tag.
        assert_eq!(payload[0], 0x00);
        // Privilege.
        assert_eq!(payload[1], PrivilegeLevel::Administrator as u8);
        // Console session ID (LE).
        assert_eq!(
            LittleEndian::read_u32(&payload[4..8]),
            0xDEADBEEF
        );
        // Auth algorithm = HMAC-SHA256 (0x03).
        assert_eq!(payload[12], AuthAlgorithm::HmacSha256 as u8);
        // Integrity algorithm = HMAC-SHA256-128 (0x04).
        assert_eq!(payload[20], IntegrityAlgorithm::HmacSha256_128 as u8);
        // Confidentiality algorithm = AES-CBC-128 (0x01).
        assert_eq!(
            payload[28],
            ConfidentialityAlgorithm::AesCbc128 as u8
        );
    }

    #[test]
    fn open_session_request_roundtrip() {
        let suite = cipher_suite_by_id(3).expect("suite 3 exists");
        let payload = build_open_session_request(
            0x42,
            PrivilegeLevel::Operator,
            0x12345678,
            &suite,
        );

        // Verify we can read back the console session ID.
        assert_eq!(LittleEndian::read_u32(&payload[4..8]), 0x12345678);
        assert_eq!(payload[0], 0x42);
    }

    // =========================================================================
    // Open Session Response
    // =========================================================================

    #[test]
    fn parse_open_session_response_success() {
        // Build a minimal success response.
        let mut data = vec![0u8; 36];
        data[0] = 0x00; // message_tag
        data[1] = 0x00; // status = success
        data[2] = 0x04; // max_privilege = Administrator
        // console_session_id = 0xAABBCCDD
        LittleEndian::write_u32(&mut data[4..8], 0xAABBCCDD);
        // managed_session_id = 0x11223344
        LittleEndian::write_u32(&mut data[8..12], 0x11223344);

        let resp = parse_open_session_response(&data).expect("valid response");
        assert_eq!(resp.message_tag, 0x00);
        assert_eq!(resp.console_session_id, 0xAABBCCDD);
        assert_eq!(resp.managed_session_id, 0x11223344);
        assert_eq!(resp.max_privilege, 0x04);
    }

    #[test]
    fn parse_open_session_response_error_status() {
        let mut data = vec![0u8; 12];
        data[1] = 0x01; // status = insufficient resources
        assert!(parse_open_session_response(&data).is_err());
    }

    #[test]
    fn parse_open_session_response_too_short() {
        assert!(parse_open_session_response(&[0u8; 8]).is_err());
    }

    // =========================================================================
    // RAKP Message 1
    // =========================================================================

    #[test]
    fn rakp1_serialization() {
        let rc = [0xAA; 16];
        let payload = build_rakp1(
            0x01,
            0x11223344,
            &rc,
            PrivilegeLevel::Administrator,
            b"admin",
        );

        // Length: tag(1) + reserved(3) + sid(4) + rc(16) + priv(1) +
        // reserved(2) + username_len(1) + username(5) = 33
        assert_eq!(payload.len(), 33);

        // Message tag.
        assert_eq!(payload[0], 0x01);
        // Managed session ID.
        assert_eq!(LittleEndian::read_u32(&payload[4..8]), 0x11223344);
        // Rc.
        assert_eq!(&payload[8..24], &[0xAA; 16]);
        // Privilege level with name-only lookup bit.
        assert_eq!(payload[24], 0x04 | 0x10);
        // Username length.
        assert_eq!(payload[27], 5);
        // Username.
        assert_eq!(&payload[28..33], b"admin");
    }

    #[test]
    fn rakp1_empty_username() {
        let rc = [0; 16];
        let payload = build_rakp1(
            0x00,
            1,
            &rc,
            PrivilegeLevel::User,
            b"",
        );

        // Length: 28 (no username bytes).
        assert_eq!(payload.len(), 28);
        assert_eq!(payload[27], 0); // username length = 0
    }

    // =========================================================================
    // RAKP Message 2
    // =========================================================================

    #[test]
    fn parse_rakp2_success() {
        let mut data = vec![0u8; 72]; // 40 header + 32 auth code (SHA256)
        data[0] = 0x01; // message_tag
        data[1] = 0x00; // status = success
        LittleEndian::write_u32(&mut data[4..8], 0xDEADBEEF); // console_sid
        // Rm.
        for i in 0..16 {
            data[8 + i] = (i + 1) as u8;
        }
        // Managed GUID.
        for i in 0..16 {
            data[24 + i] = (i + 0x10) as u8;
        }
        // Auth code (32 bytes of 0xCC).
        for i in 0..32 {
            data[40 + i] = 0xCC;
        }

        let resp = parse_rakp2(&data).expect("valid RAKP 2");
        assert_eq!(resp.message_tag, 0x01);
        assert_eq!(resp.console_session_id, 0xDEADBEEF);
        assert_eq!(resp.rm[0], 0x01);
        assert_eq!(resp.rm[15], 0x10);
        assert_eq!(resp.managed_guid[0], 0x10);
        assert_eq!(resp.key_exchange_auth_code.len(), 32);
    }

    #[test]
    fn parse_rakp2_error_status() {
        let mut data = vec![0u8; 40];
        data[1] = 0x0E; // unauthorized name
        assert!(parse_rakp2(&data).is_err());
    }

    // =========================================================================
    // RAKP Message 2 HMAC Verification
    // =========================================================================

    #[test]
    fn verify_rakp2_hmac_known_vector() {
        // Use known values to compute and verify the HMAC.
        let password = b"test_password";
        let console_sid: u32 = 0x01020304;
        let managed_sid: u32 = 0x05060708;
        let rc = [0x11u8; 16];
        let rm = [0x22u8; 16];
        let guid = [0x33u8; 16];
        let role: u8 = 0x14; // Administrator with name-only lookup
        let username = b"admin";

        // Compute the expected HMAC.
        let mut hmac_data = Vec::new();
        let mut buf = [0u8; 4];
        LittleEndian::write_u32(&mut buf, console_sid);
        hmac_data.extend_from_slice(&buf);
        LittleEndian::write_u32(&mut buf, managed_sid);
        hmac_data.extend_from_slice(&buf);
        hmac_data.extend_from_slice(&rc);
        hmac_data.extend_from_slice(&rm);
        hmac_data.extend_from_slice(&guid);
        hmac_data.push(role);
        hmac_data.push(username.len() as u8);
        hmac_data.extend_from_slice(username);

        let expected_hmac =
            hmac_auth::hmac_sha256(password, &hmac_data).expect("hmac computation");

        // Verification should succeed with the correct HMAC.
        verify_rakp2_hmac(&Rakp2HmacParams {
            auth_alg: AuthAlgorithm::HmacSha256,
            password,
            console_session_id: console_sid,
            managed_session_id: managed_sid,
            rc: &rc,
            rm: &rm,
            managed_guid: &guid,
            role,
            username,
            received_hmac: &expected_hmac,
        })
        .expect("HMAC should verify");
    }

    #[test]
    fn verify_rakp2_hmac_wrong_password() {
        let password = b"correct_password";
        let wrong_password = b"wrong_password";
        let console_sid: u32 = 0x01020304;
        let managed_sid: u32 = 0x05060708;
        let rc = [0x11u8; 16];
        let rm = [0x22u8; 16];
        let guid = [0x33u8; 16];
        let role: u8 = 0x14;
        let username = b"admin";

        // Compute HMAC with wrong password.
        let mut hmac_data = Vec::new();
        let mut buf = [0u8; 4];
        LittleEndian::write_u32(&mut buf, console_sid);
        hmac_data.extend_from_slice(&buf);
        LittleEndian::write_u32(&mut buf, managed_sid);
        hmac_data.extend_from_slice(&buf);
        hmac_data.extend_from_slice(&rc);
        hmac_data.extend_from_slice(&rm);
        hmac_data.extend_from_slice(&guid);
        hmac_data.push(role);
        hmac_data.push(username.len() as u8);
        hmac_data.extend_from_slice(username);

        let wrong_hmac =
            hmac_auth::hmac_sha256(wrong_password, &hmac_data).expect("hmac computation");

        // Verification should fail.
        assert!(verify_rakp2_hmac(&Rakp2HmacParams {
            auth_alg: AuthAlgorithm::HmacSha256,
            password,
            console_session_id: console_sid,
            managed_session_id: managed_sid,
            rc: &rc,
            rm: &rm,
            managed_guid: &guid,
            role,
            username,
            received_hmac: &wrong_hmac,
        })
        .is_err());
    }

    // =========================================================================
    // RAKP Message 3
    // =========================================================================

    #[test]
    fn rakp3_auth_code_computation() {
        let rm = [0x22u8; 16];
        let console_sid: u32 = 0x01020304;
        let role: u8 = 0x14;
        let username = b"admin";
        let password = b"test_password";

        let payload = build_rakp3(&Rakp3Params {
            message_tag: 0x02,
            managed_session_id: 0x05060708,
            auth_alg: AuthAlgorithm::HmacSha256,
            password,
            rm: &rm,
            console_session_id: console_sid,
            role,
            username,
        })
        .expect("build RAKP 3");

        // Header: tag(1) + status(1) + reserved(2) + sid(4) = 8.
        assert!(payload.len() > 8, "payload should contain auth code");
        assert_eq!(payload[0], 0x02); // message_tag
        assert_eq!(payload[1], 0x00); // status = success
        assert_eq!(
            LittleEndian::read_u32(&payload[4..8]),
            0x05060708
        );

        // Verify the auth code by computing it independently.
        let mut hmac_data = Vec::new();
        hmac_data.extend_from_slice(&rm);
        let mut buf = [0u8; 4];
        LittleEndian::write_u32(&mut buf, console_sid);
        hmac_data.extend_from_slice(&buf);
        hmac_data.push(role);
        hmac_data.push(username.len() as u8);
        hmac_data.extend_from_slice(username);

        let expected_auth_code =
            hmac_auth::hmac_sha256(password, &hmac_data).expect("hmac computation");

        assert_eq!(&payload[8..], &expected_auth_code);
    }

    // =========================================================================
    // RAKP Message 4
    // =========================================================================

    #[test]
    fn parse_rakp4_success() {
        let mut data = vec![0u8; 24]; // 8 header + 16 ICV (SHA256-128)
        data[0] = 0x02; // message_tag
        data[1] = 0x00; // status = success
        LittleEndian::write_u32(&mut data[4..8], 0xDEADBEEF);
        for i in 0..16 {
            data[8 + i] = (i + 1) as u8;
        }

        let resp = parse_rakp4(&data).expect("valid RAKP 4");
        assert_eq!(resp.message_tag, 0x02);
        assert_eq!(resp.console_session_id, 0xDEADBEEF);
        assert_eq!(resp.integrity_check_value.len(), 16);
    }

    #[test]
    fn verify_rakp4_icv_sha256() {
        let sik = vec![0xAA; 32];
        let rc = [0x11u8; 16];
        let managed_sid: u32 = 0x05060708;
        let guid = [0x33u8; 16];

        // Compute the expected ICV.
        let mut icv_data = Vec::new();
        icv_data.extend_from_slice(&rc);
        let mut buf = [0u8; 4];
        LittleEndian::write_u32(&mut buf, managed_sid);
        icv_data.extend_from_slice(&buf);
        icv_data.extend_from_slice(&guid);

        let full_hmac =
            hmac_auth::hmac_sha256(&sik, &icv_data).expect("hmac computation");
        // SHA256 ICV is truncated to 16 bytes.
        let expected_icv = &full_hmac[..16];

        verify_rakp4_icv(
            AuthAlgorithm::HmacSha256,
            &sik,
            &rc,
            managed_sid,
            &guid,
            expected_icv,
        )
        .expect("ICV should verify");
    }

    #[test]
    fn verify_rakp4_icv_wrong_sik() {
        let sik = vec![0xAA; 32];
        let wrong_sik = vec![0xBB; 32];
        let rc = [0x11u8; 16];
        let managed_sid: u32 = 0x05060708;
        let guid = [0x33u8; 16];

        // Compute ICV with wrong SIK.
        let mut icv_data = Vec::new();
        icv_data.extend_from_slice(&rc);
        let mut buf = [0u8; 4];
        LittleEndian::write_u32(&mut buf, managed_sid);
        icv_data.extend_from_slice(&buf);
        icv_data.extend_from_slice(&guid);

        let wrong_hmac =
            hmac_auth::hmac_sha256(&wrong_sik, &icv_data).expect("hmac computation");
        let wrong_icv = &wrong_hmac[..16];

        assert!(verify_rakp4_icv(
            AuthAlgorithm::HmacSha256,
            &sik,
            &rc,
            managed_sid,
            &guid,
            wrong_icv,
        )
        .is_err());
    }
}

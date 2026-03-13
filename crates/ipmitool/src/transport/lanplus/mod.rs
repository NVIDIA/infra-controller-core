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

//! RMCP+ / IPMI v2.0 lanplus transport implementation.
//!
//! This module implements the full RMCP+ session lifecycle:
//! 1. Open Session Request/Response
//! 2. RAKP 1-4 handshake (mutual authentication + key exchange)
//! 3. Authenticated IPMI command exchange
//! 4. Session close

pub mod header;
pub mod packet;
pub mod rakp;
pub mod session;

use std::time::Duration;

use byteorder::{ByteOrder, LittleEndian};
use rand::Rng;
use tokio::net::UdpSocket;
use tracing::{debug, instrument, warn};

use self::header::{IpmiMsgHeader, PayloadType};
use self::packet::{build_authenticated_packet, build_pre_session_packet, parse_packet};
use self::rakp::{
    build_open_session_request, build_rakp1, build_rakp3, parse_open_session_response,
    parse_rakp2, parse_rakp4, verify_rakp2_hmac, verify_rakp4_icv, Rakp2HmacParams,
    Rakp3Params,
};
use self::session::{ActiveSession, SessionState};
use crate::crypto::{aes_cbc, hmac_auth, keys};
use crate::error::{IpmitoolError, Result};
use crate::transport::IpmiTransport;
use crate::types::{
    CipherSuiteId, IpmiRequest, IpmiResponse, IntegrityAlgorithm, NetFn,
    PrivilegeLevel,
};
use crate::ConnectionConfig;

/// Maximum size of a single RMCP+ UDP packet.
const MAX_PACKET_SIZE: usize = 1024;

/// Default initial timeout for UDP recv.
const INITIAL_TIMEOUT: Duration = Duration::from_secs(2);

/// The RMCP+ lanplus transport.
///
/// Manages a UDP socket, the session state machine, and the IPMI message
/// sequence number. Created via [`LanplusTransport::connect`], which performs
/// the full RAKP handshake before returning.
pub struct LanplusTransport {
    socket: UdpSocket,
    session: SessionState,
    config: ConnectionConfig,
    cipher_suite: CipherSuiteId,
    /// 6-bit IPMI message sequence number (0-63), used in the inner IPMI
    /// message header to correlate requests with responses.
    rq_seq: u8,
}

impl LanplusTransport {
    /// Connect to a BMC and establish an RMCP+ session.
    ///
    /// Opens a UDP socket, then runs the full handshake:
    /// Open Session → RAKP 1 → RAKP 2 → RAKP 3 → RAKP 4.
    ///
    /// # Errors
    ///
    /// Returns an error if the network is unreachable, the BMC rejects the
    /// session, or the RAKP authentication fails.
    #[instrument(skip(config), fields(host = %config.host, port = config.port))]
    pub async fn connect(config: ConnectionConfig) -> Result<Self> {
        let cipher_suite = crate::types::cipher_suite_by_id(config.cipher_suite).ok_or_else(
            || {
                IpmitoolError::Transport(format!(
                    "unsupported cipher suite: {}",
                    config.cipher_suite
                ))
            },
        )?;

        // Bind to an ephemeral local port and connect to the BMC.
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let addr = format!("{}:{}", config.host, config.port);
        socket.connect(&addr).await?;

        debug!(%addr, cipher_suite_id = cipher_suite.id, "UDP socket connected");

        let mut transport = Self {
            socket,
            session: SessionState::Inactive,
            config,
            cipher_suite,
            rq_seq: 0,
        };

        transport.run_rakp_handshake().await?;

        Ok(transport)
    }

    /// Advance and return the next 6-bit IPMI request sequence number (0-63).
    fn next_rq_seq(&mut self) -> u8 {
        self.rq_seq = (self.rq_seq + 1) & 0x3F;
        self.rq_seq
    }

    /// Returns the integrity auth code length for the negotiated algorithm.
    fn auth_code_len(&self) -> usize {
        match self.cipher_suite.integrity {
            IntegrityAlgorithm::None => 0,
            IntegrityAlgorithm::HmacSha1_96 => 12,
            IntegrityAlgorithm::HmacMd5_128 | IntegrityAlgorithm::Md5_128 => 16,
            IntegrityAlgorithm::HmacSha256_128 => 16,
        }
    }

    // ==========================================================================
    // RAKP Handshake
    // ==========================================================================

    /// Run the complete RAKP handshake sequence.
    async fn run_rakp_handshake(&mut self) -> Result<()> {
        // Generate a non-zero console session ID.
        let console_session_id = generate_nonzero_session_id();
        let privilege = PrivilegeLevel::Administrator;

        // Step 1: Open Session.
        let open_req = build_open_session_request(
            0x00,
            privilege,
            console_session_id,
            &self.cipher_suite,
        );
        let open_req_packet =
            build_pre_session_packet(PayloadType::OpenSessionRequest, &open_req);

        self.session = SessionState::OpenSessionSent { console_session_id };
        let open_resp_bytes = self.send_recv_raw(&open_req_packet).await?;
        let open_resp_parsed = parse_packet(&open_resp_bytes, 0)?;
        let open_resp = parse_open_session_response(open_resp_parsed.payload)?;

        debug!(
            managed_session_id = open_resp.managed_session_id,
            "Open Session Response received"
        );

        // Step 2: RAKP Message 1.
        let mut rc = [0u8; 16];
        rand::rng().fill(&mut rc);
        let role = privilege as u8 | 0x10; // name-only lookup

        let rakp1 = build_rakp1(
            0x00,
            open_resp.managed_session_id,
            &rc,
            privilege,
            self.config.username.as_bytes(),
        );
        let rakp1_packet = build_pre_session_packet(PayloadType::Rakp1, &rakp1);

        self.session = SessionState::Rakp1Sent {
            console_session_id,
            managed_session_id: open_resp.managed_session_id,
            rc,
        };
        let rakp2_bytes = self.send_recv_raw(&rakp1_packet).await?;
        let rakp2_parsed = parse_packet(&rakp2_bytes, 0)?;
        let rakp2 = parse_rakp2(rakp2_parsed.payload)?;

        debug!("RAKP Message 2 received, verifying HMAC");

        // Verify RAKP 2 HMAC.
        verify_rakp2_hmac(&Rakp2HmacParams {
            auth_alg: self.cipher_suite.auth,
            password: self.config.password.as_bytes(),
            console_session_id,
            managed_session_id: open_resp.managed_session_id,
            rc: &rc,
            rm: &rakp2.rm,
            managed_guid: &rakp2.managed_guid,
            role,
            username: self.config.username.as_bytes(),
            received_hmac: &rakp2.key_exchange_auth_code,
        })?;

        // Derive SIK. If Kg is all zeros (most BMCs), use the password as Kg.
        let kg = self.config.password.as_bytes();
        let sik = keys::derive_sik(
            self.cipher_suite.auth,
            kg,
            &rakp2.rm,
            &rc,
            role,
            self.config.username.as_bytes(),
        )?;

        // Step 3: RAKP Message 3.
        let rakp3 = build_rakp3(&Rakp3Params {
            message_tag: 0x00,
            managed_session_id: open_resp.managed_session_id,
            auth_alg: self.cipher_suite.auth,
            password: self.config.password.as_bytes(),
            rm: &rakp2.rm,
            console_session_id,
            role,
            username: self.config.username.as_bytes(),
        })?;
        let rakp3_packet = build_pre_session_packet(PayloadType::Rakp3, &rakp3);

        self.session = SessionState::Rakp3Sent {
            console_session_id,
            managed_session_id: open_resp.managed_session_id,
            rc,
            sik: sik.clone(),
            managed_guid: rakp2.managed_guid,
        };
        let rakp4_bytes = self.send_recv_raw(&rakp3_packet).await?;
        let rakp4_parsed = parse_packet(&rakp4_bytes, 0)?;
        let rakp4 = parse_rakp4(rakp4_parsed.payload)?;

        debug!("RAKP Message 4 received, verifying ICV");

        // Verify RAKP 4 ICV.
        verify_rakp4_icv(
            self.cipher_suite.auth,
            &sik,
            &rc,
            open_resp.managed_session_id,
            &rakp2.managed_guid,
            &rakp4.integrity_check_value,
        )?;

        // Derive session keys.
        let k1 = keys::derive_k1(self.cipher_suite.auth, &sik)?;
        let k2 = keys::derive_k2(self.cipher_suite.auth, &sik)?;

        debug!("RAKP handshake complete, session active");

        self.session = SessionState::Active {
            session: ActiveSession::new(
                open_resp.managed_session_id,
                console_session_id,
                sik,
                k1,
                k2,
                self.cipher_suite.auth,
            ),
        };

        Ok(())
    }

    // ==========================================================================
    // Raw UDP Send/Receive
    // ==========================================================================

    /// Send a raw packet and receive the response, with retries and escalating
    /// timeout.
    async fn send_recv_raw(&self, packet: &[u8]) -> Result<Vec<u8>> {
        let max_retries = self.config.retries;
        let mut timeout = INITIAL_TIMEOUT;

        for attempt in 0..=max_retries {
            self.socket.send(packet).await?;

            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            match tokio::time::timeout(timeout, self.socket.recv(&mut buf)).await {
                Ok(Ok(n)) => {
                    buf.truncate(n);
                    return Ok(buf);
                }
                Ok(Err(e)) => return Err(IpmitoolError::Io(e)),
                Err(_) => {
                    if attempt < max_retries {
                        warn!(
                            attempt = attempt + 1,
                            max_retries,
                            "timeout, retrying with increased timeout"
                        );
                        // Escalate timeout by 1 second per retry.
                        timeout += Duration::from_secs(1);
                    }
                }
            }
        }

        Err(IpmitoolError::Timeout {
            retries: max_retries,
        })
    }
}

impl IpmiTransport for LanplusTransport {
    /// Send an IPMI request and wait for the response.
    ///
    /// Builds the inner IPMI message, encrypts it, adds integrity, sends over
    /// UDP, then receives, verifies integrity, decrypts, and parses the
    /// response.
    async fn send_recv(&mut self, req: &IpmiRequest) -> Result<IpmiResponse> {
        // Extract everything we need from the session up front to avoid
        // holding a mutable borrow on `self.session` while we also need
        // `&mut self` for `next_rq_seq` and `send_recv_raw`.
        let (aes_key, k1, seq, managed_sid) = match &mut self.session {
            SessionState::Active { session } => {
                let aes_key = session.k2[..16].to_vec();
                let k1 = session.k1.clone();
                let seq = session.next_seq();
                let managed_sid = session.managed_session_id;
                (aes_key, k1, seq, managed_sid)
            }
            other => {
                return Err(IpmitoolError::InvalidSessionState {
                    expected: "Active",
                    actual: other.name(),
                });
            }
        };

        let rq_seq = self.next_rq_seq();
        let netfn: u8 = req.netfn.into();

        // Build the inner IPMI message (header + data + checksums).
        let ipmi_msg_header = IpmiMsgHeader::request(netfn, req.cmd, rq_seq);
        let ipmi_msg = ipmi_msg_header.build_message(&req.data);

        let integrity_alg = self.cipher_suite.integrity;

        // Build the authenticated packet.
        let packet = build_authenticated_packet(
            managed_sid,
            seq,
            &ipmi_msg,
            |plaintext| aes_cbc::encrypt(&aes_key, plaintext),
            |data| compute_integrity(integrity_alg, &k1, data),
        )?;

        // Send and receive.
        let resp_bytes = self.send_recv_raw(&packet).await?;

        // Parse the response packet.
        let auth_code_len = self.auth_code_len();
        let parsed = parse_packet(&resp_bytes, auth_code_len)?;

        // Verify integrity if authenticated.
        if parsed.session.is_authenticated() {
            packet::verify_integrity(&resp_bytes, auth_code_len, |data| {
                compute_integrity(integrity_alg, &k1, data)
            })?;
        }

        // Decrypt the payload if encrypted.
        let plaintext = if parsed.session.is_encrypted() {
            aes_cbc::decrypt(&aes_key, parsed.payload)?
        } else {
            parsed.payload.to_vec()
        };

        // Parse the inner IPMI message.
        let (_resp_header, response_data) = IpmiMsgHeader::parse_message(&plaintext)?;

        IpmiResponse::from_bytes(response_data)
    }

    /// Close the RMCP+ session by sending a Close Session command.
    async fn close(&mut self) -> Result<()> {
        // Only attempt close if we have an active session.
        if let SessionState::Active { session } = &self.session {
            let managed_sid = session.managed_session_id;

            // Close Session command: NetFn=App (0x06), Cmd=0x3C.
            // Data = 4-byte LE session ID.
            let mut close_data = [0u8; 4];
            LittleEndian::write_u32(&mut close_data, managed_sid);

            let close_req = IpmiRequest::with_data(NetFn::App, 0x3C, close_data.to_vec());

            // Best-effort close — don't propagate errors since we're tearing
            // down anyway.
            match self.send_recv(&close_req).await {
                Ok(_) => debug!("session closed successfully"),
                Err(e) => warn!(error = %e, "error closing session (ignored)"),
            }
        }

        self.session = SessionState::Closed;
        Ok(())
    }
}

// ==============================================================================
// Helpers
// ==============================================================================

/// Generate a random non-zero u32 for use as a session ID.
fn generate_nonzero_session_id() -> u32 {
    loop {
        let id: u32 = rand::rng().random();
        if id != 0 {
            return id;
        }
    }
}

/// Compute the integrity check value for the given algorithm and key.
fn compute_integrity(
    alg: IntegrityAlgorithm,
    k1: &[u8],
    data: &[u8],
) -> Result<Vec<u8>> {
    match alg {
        IntegrityAlgorithm::None => Ok(Vec::new()),
        IntegrityAlgorithm::HmacSha1_96 => hmac_auth::integrity_hmac_sha1_96(k1, data),
        IntegrityAlgorithm::HmacMd5_128 => hmac_auth::integrity_hmac_md5_128(k1, data),
        IntegrityAlgorithm::HmacSha256_128 => hmac_auth::integrity_hmac_sha256_128(k1, data),
        IntegrityAlgorithm::Md5_128 => {
            // MD5-128 uses a raw MD5 hash (not HMAC) in the IPMI spec,
            // but for our purposes we treat it as HMAC-MD5 since that's
            // how most implementations handle it.
            hmac_auth::integrity_hmac_md5_128(k1, data)
        }
    }
}

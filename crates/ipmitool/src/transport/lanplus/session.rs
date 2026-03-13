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

//! RMCP+ session state machine.
//!
//! Tracks the session through the RAKP handshake phases and into the
//! active (authenticated) state. Each state carries only the data needed
//! for the next transition, keeping earlier handshake material from
//! leaking into later states.

use crate::types::AuthAlgorithm;

// ==============================================================================
// Session State Machine
// ==============================================================================

/// Session states for the RMCP+ handshake.
///
/// The handshake progresses linearly:
///
/// ```text
/// Inactive → OpenSessionSent → Rakp1Sent → Rakp3Sent → Active → Closed
/// ```
///
/// Any error during the handshake returns the session to `Inactive`.
pub enum SessionState {
    /// No session established.
    Inactive,

    /// Open Session Request sent; waiting for Open Session Response.
    OpenSessionSent {
        console_session_id: u32,
    },

    /// RAKP Message 1 sent; waiting for RAKP Message 2.
    Rakp1Sent {
        console_session_id: u32,
        managed_session_id: u32,
        /// Remote console random number (Rc).
        rc: [u8; 16],
    },

    /// RAKP Message 3 sent; waiting for RAKP Message 4.
    Rakp3Sent {
        console_session_id: u32,
        managed_session_id: u32,
        /// Remote console random number (Rc), retained for RAKP 4 verification.
        rc: [u8; 16],
        /// Session Integrity Key, pre-computed after RAKP 2.
        sik: Vec<u8>,
        /// Managed system GUID from RAKP 2, needed for RAKP 4 ICV check.
        managed_guid: [u8; 16],
    },

    /// Session fully established and ready for IPMI commands.
    Active {
        session: ActiveSession,
    },

    /// Session has been closed.
    Closed,
}

impl SessionState {
    /// Returns a human-readable name for the current state, for diagnostics.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Inactive => "Inactive",
            Self::OpenSessionSent { .. } => "OpenSessionSent",
            Self::Rakp1Sent { .. } => "Rakp1Sent",
            Self::Rakp3Sent { .. } => "Rakp3Sent",
            Self::Active { .. } => "Active",
            Self::Closed => "Closed",
        }
    }
}

// ==============================================================================
// Active Session
// ==============================================================================

/// A fully-established RMCP+ session with derived keys.
pub struct ActiveSession {
    /// The managed system's session ID (used in outgoing packet headers).
    pub managed_session_id: u32,
    /// The console's session ID (used to match incoming packets).
    pub console_session_id: u32,
    /// Monotonically increasing session sequence number.
    session_seq: u32,
    /// Session Integrity Key (derived from RAKP exchange).
    pub sik: Vec<u8>,
    /// K1 — integrity key for per-message HMACs.
    pub k1: Vec<u8>,
    /// K2 — confidentiality key for AES encryption. Only the first 16 bytes
    /// are used as the AES-128 key.
    pub k2: Vec<u8>,
    /// Authentication algorithm negotiated during session open.
    pub auth_algorithm: AuthAlgorithm,
}

impl ActiveSession {
    /// Create a new active session with initial sequence number 0.
    ///
    /// The first call to `next_seq` will return 1 (sequence 0 is reserved
    /// per the IPMI spec for pre-session messages).
    pub fn new(
        managed_session_id: u32,
        console_session_id: u32,
        sik: Vec<u8>,
        k1: Vec<u8>,
        k2: Vec<u8>,
        auth_algorithm: AuthAlgorithm,
    ) -> Self {
        Self {
            managed_session_id,
            console_session_id,
            session_seq: 0,
            sik,
            k1,
            k2,
            auth_algorithm,
        }
    }

    /// Advance and return the next session sequence number.
    ///
    /// Sequence 0 is reserved for pre-session traffic, so the first call
    /// returns 1 and subsequent calls increment normally.
    pub fn next_seq(&mut self) -> u32 {
        self.session_seq = self.session_seq.wrapping_add(1);

        // Sequence 0 is reserved — skip it on wrap-around.
        if self.session_seq == 0 {
            self.session_seq = 1;
        }

        self.session_seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Sequence Number Tests
    // =========================================================================

    #[test]
    fn next_seq_starts_at_one() {
        let mut session = ActiveSession::new(
            1,
            2,
            vec![0; 32],
            vec![0; 32],
            vec![0; 32],
            AuthAlgorithm::HmacSha256,
        );
        assert_eq!(session.next_seq(), 1);
    }

    #[test]
    fn next_seq_increments() {
        let mut session = ActiveSession::new(
            1,
            2,
            vec![0; 32],
            vec![0; 32],
            vec![0; 32],
            AuthAlgorithm::HmacSha256,
        );
        assert_eq!(session.next_seq(), 1);
        assert_eq!(session.next_seq(), 2);
        assert_eq!(session.next_seq(), 3);
    }

    #[test]
    fn next_seq_skips_zero_on_wraparound() {
        let mut session = ActiveSession::new(
            1,
            2,
            vec![0; 32],
            vec![0; 32],
            vec![0; 32],
            AuthAlgorithm::HmacSha256,
        );

        // Manually set sequence to just before wrap.
        session.session_seq = u32::MAX;

        // Next should wrap past 0 to 1.
        assert_eq!(session.next_seq(), 1);
        assert_eq!(session.next_seq(), 2);
    }

    // =========================================================================
    // State Name Tests
    // =========================================================================

    #[test]
    fn state_names() {
        assert_eq!(SessionState::Inactive.name(), "Inactive");
        assert_eq!(
            SessionState::OpenSessionSent {
                console_session_id: 1
            }
            .name(),
            "OpenSessionSent"
        );
        assert_eq!(
            SessionState::Rakp1Sent {
                console_session_id: 1,
                managed_session_id: 2,
                rc: [0; 16],
            }
            .name(),
            "Rakp1Sent"
        );
        assert_eq!(
            SessionState::Rakp3Sent {
                console_session_id: 1,
                managed_session_id: 2,
                rc: [0; 16],
                sik: vec![],
                managed_guid: [0; 16],
            }
            .name(),
            "Rakp3Sent"
        );
        assert_eq!(SessionState::Closed.name(), "Closed");
    }
}

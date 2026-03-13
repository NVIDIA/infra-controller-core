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

//! Mock IPMI transport for testing.
//!
//! Returns canned responses keyed by (netfn, cmd) without performing any
//! network I/O, allowing command-level logic to be tested in isolation.

use std::collections::HashMap;

use crate::error::{IpmitoolError, Result};
use crate::transport::IpmiTransport;
use crate::types::{IpmiRequest, IpmiResponse};

/// A mock IPMI transport that returns pre-configured responses.
///
/// Responses are keyed by `(netfn, cmd)`. The stored `Vec<u8>` should be
/// the raw response bytes starting with the completion code, matching what
/// [`IpmiResponse::from_bytes`] expects.
pub struct MockTransport {
    responses: HashMap<(u8, u8), Vec<u8>>,
}

impl MockTransport {
    /// Create a new mock transport with no configured responses.
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }

    /// Register a canned response for a given (netfn, cmd) pair.
    ///
    /// The `response_bytes` should start with the completion code byte,
    /// followed by any response data.
    pub fn add_response(&mut self, netfn: u8, cmd: u8, response_bytes: Vec<u8>) {
        self.responses.insert((netfn, cmd), response_bytes);
    }
}

impl IpmiTransport for MockTransport {
    async fn send_recv(&mut self, req: &IpmiRequest) -> Result<IpmiResponse> {
        let netfn: u8 = req.netfn.into();
        let key = (netfn, req.cmd);

        let response_bytes = self.responses.get(&key).ok_or_else(|| {
            IpmitoolError::Transport(format!(
                "MockTransport: no response configured for netfn=0x{:02X} cmd=0x{:02X}",
                netfn, req.cmd
            ))
        })?;

        IpmiResponse::from_bytes(response_bytes)
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CompletionCode, NetFn};

    #[tokio::test]
    async fn mock_returns_correct_response() {
        let mut transport = MockTransport::new();

        // Register a successful Get Device ID response.
        // Completion code 0x00 + some device ID data.
        let device_id_data = vec![
            0x00, // completion code = success
            0x20, // device ID
            0x01, // device revision
            0x02, // firmware major
            0x03, // firmware minor
        ];
        transport.add_response(0x06, 0x01, device_id_data);

        let req = IpmiRequest::new(NetFn::App, 0x01);
        let resp = transport.send_recv(&req).await.expect("should succeed");

        assert!(resp.completion_code.is_success());
        assert_eq!(resp.data, vec![0x20, 0x01, 0x02, 0x03]);
    }

    #[tokio::test]
    async fn mock_returns_error_for_unknown_command() {
        let mut transport = MockTransport::new();
        let req = IpmiRequest::new(NetFn::App, 0xFF);

        let result = transport.send_recv(&req).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, IpmitoolError::Transport(msg) if msg.contains("no response configured")),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn mock_returns_non_zero_completion_code() {
        let mut transport = MockTransport::new();

        // Register a response with a non-zero completion code.
        transport.add_response(0x00, 0x02, vec![0xC1]); // Invalid command

        let req = IpmiRequest::new(NetFn::Chassis, 0x02);
        let resp = transport.send_recv(&req).await.expect("should parse");

        assert_eq!(resp.completion_code, CompletionCode::InvalidCommand);
        assert!(resp.data.is_empty());
    }

    #[tokio::test]
    async fn mock_close_succeeds() {
        let mut transport = MockTransport::new();
        transport.close().await.expect("close should succeed");
    }
}

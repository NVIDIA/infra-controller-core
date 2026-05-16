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
use std::net::Ipv4Addr;

use dhcproto::v4::relay::RelayInfo;
use dhcproto::v4::{Message, relay};
use dhcproto::{Encodable, Encoder, v4};

pub const RELAY_IP: &str = "127.1.2.3";

pub struct DHCPFactory {}

impl DHCPFactory {
    pub fn encode(msg: Message) -> Result<Vec<u8>, eyre::Report> {
        let mut buf = Vec::with_capacity(300); // msg is 279 bytes
        let mut e = Encoder::new(&mut buf);
        msg.encode(&mut e)?;
        Ok(buf)
    }

    // Make and encode a relayed DHCP_DISCOVER packet
    // The idx is used as the last byte of the MAC and Link addresses to make them unique.
    pub fn discover(idx: u8) -> Message {
        Self::base_relayed_message(idx, v4::MessageType::Discover)
    }

    /// Build a SELECTING-state DHCPREQUEST (the REQUEST that follows an OFFER).
    /// Includes option 50 (requested-address) and option 54 (server-identifier).
    /// `requested_addr` is the IP the client claims it was offered.
    #[allow(dead_code)]
    pub fn request_selecting(idx: u8, requested_addr: Ipv4Addr, server_id: Ipv4Addr) -> Message {
        let mut msg = Self::base_relayed_message(idx, v4::MessageType::Request);
        use v4::DhcpOption::*;
        let opts = msg.opts_mut();
        opts.insert(RequestedIpAddress(requested_addr));
        opts.insert(ServerIdentifier(server_id));
        msg
    }

    /// Build an INIT-REBOOT-state DHCPREQUEST: option 50 set, no ciaddr, no option 54.
    /// Used to simulate a client that remembers a prior IP across reboot and is
    /// asking the network to confirm it.
    #[allow(dead_code)]
    pub fn request_init_reboot(idx: u8, requested_addr: Ipv4Addr) -> Message {
        let mut msg = Self::base_relayed_message(idx, v4::MessageType::Request);
        msg.opts_mut()
            .insert(v4::DhcpOption::RequestedIpAddress(requested_addr));
        msg
    }

    /// Build a RENEWING-state DHCPREQUEST: ciaddr set to current lease IP, no
    /// option 50, no option 54. Per RFC 2131 this would be unicast to the server,
    /// but our test harness pretends to be the relay so we just send it through
    /// the same relayed path as everything else.
    #[allow(dead_code)]
    pub fn request_renewing(idx: u8, ciaddr: Ipv4Addr) -> Message {
        let mut msg = Self::base_relayed_message(idx, v4::MessageType::Request);
        msg.set_ciaddr(ciaddr);
        msg
    }

    fn base_relayed_message(idx: u8, msg_type: v4::MessageType) -> Message {
        // 0x02 prefix is a 'locally administered address'
        let mac = vec![0x02, 0x00, 0x00, 0x00, 0x00, idx];

        // Five colon separated fields. Our parser (vendor_class.rs) only uses fields 0 and 2.
        // 7 is MachineArchitecture::EfiX64, HTTP version
        let uefi_vendor_class = b"HTTPClient::7::".to_vec();

        let mut relay_agent = relay::RelayAgentInformation::default();
        relay_agent.insert(RelayInfo::AgentCircuitId(b"eth0".to_vec()));
        let link_address = [172, 16, 42, idx];
        relay_agent.insert(RelayInfo::LinkSelection(link_address.into()));

        let gateway_ip = RELAY_IP.parse::<Ipv4Addr>().unwrap();

        let mut msg = v4::Message::default();
        let opts = msg
            .set_chaddr(&mac)
            .set_giaddr(gateway_ip) // This says message was relayed
            .set_hops(1) // a real relayed packet would have this. not necessary for the test.
            .opts_mut();
        use v4::DhcpOption::*;
        opts.insert(ClassIdentifier(uefi_vendor_class)); // 60
        opts.insert(RelayAgentInformation(relay_agent)); // 82
        opts.insert(ClientSystemArchitecture(v4::Architecture::Intelx86PC)); // 93
        opts.insert(MessageType(msg_type));

        msg
    }
}

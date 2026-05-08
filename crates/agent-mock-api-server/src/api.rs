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

use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use carbide_network::virtualization::VpcVirtualizationType;
use carbide_version::v;
use prost_types::Timestamp;
use tonic::{Request, Response, Status};

use crate::MockApiServer;
use crate::generated::forge::forge_server::Forge;
use crate::generated::{common as rpc_common, forge as rpc};

const DPU_ID: &str = "fm100dsvstfujf6mis0gpsoi81tadmllicv7rqo4s7gc16gi0t2478672vg";
const DEST_DPU_ID: &str = "fm100dsjd1vuk6gklgvh0ao8t7r7tk1pt101ub5ck0g3j7lqcm8h3rf1p8g";

#[tonic::async_trait]
impl Forge for MockApiServer {
    async fn version(
        &self,
        _request: Request<rpc::VersionRequest>,
    ) -> Result<Response<rpc::BuildInfo>, Status> {
        let runtime_config = Some(rpc::RuntimeConfig {
            sitename: self.config.sitename.clone(),
            ..Default::default()
        });

        Ok(Response::new(rpc::BuildInfo {
            build_version: v!(build_version).to_string(),
            build_date: v!(build_date).to_string(),
            git_sha: v!(git_sha).to_string(),
            rust_version: v!(rust_version).to_string(),
            build_user: v!(build_user).to_string(),
            build_hostname: v!(build_hostname).to_string(),
            runtime_config,
        }))
    }

    async fn get_managed_host_network_config(
        &self,
        _request: Request<rpc::ManagedHostNetworkConfigRequest>,
    ) -> Result<Response<rpc::ManagedHostNetworkConfigResponse>, Status> {
        self.state
            .num_netconf_fetches
            .fetch_add(1, Ordering::SeqCst);

        Ok(Response::new(build_managed_host_network_config(
            self.config.virtualization_type,
        )))
    }

    async fn record_dpu_network_status(
        &self,
        _request: Request<rpc::DpuNetworkStatus>,
    ) -> Result<Response<()>, Status> {
        self.state.num_health_reports.fetch_add(1, Ordering::SeqCst);

        Ok(Response::new(()))
    }

    async fn dpu_agent_upgrade_check(
        &self,
        _request: Request<rpc::DpuAgentUpgradeCheckRequest>,
    ) -> Result<Response<rpc::DpuAgentUpgradeCheckResponse>, Status> {
        self.state
            .has_checked_for_upgrade
            .store(true, Ordering::SeqCst);

        Ok(Response::new(rpc::DpuAgentUpgradeCheckResponse {
            should_upgrade: self.config.upgrade_response.should_upgrade,
            package_version: self.config.upgrade_response.package_version.clone(),
            server_version: self.config.upgrade_response.server_version.clone(),
        }))
    }

    async fn discover_machine(
        &self,
        _request: Request<rpc::MachineDiscoveryInfo>,
    ) -> Result<Response<rpc::MachineDiscoveryResult>, Status> {
        self.state.has_discovered.store(true, Ordering::SeqCst);

        Ok(Response::new(rpc::MachineDiscoveryResult {
            machine_id: Some(
                "fm100dsasb5dsh6e6ogogslpovne4rj82rp9jlf00qd7mcvmaadv85phk3g"
                    .parse()
                    .expect("valid machine id"),
            ),
            machine_certificate: None,
            attest_key_challenge: None,
            machine_interface_id: None,
        }))
    }

    async fn find_interfaces(
        &self,
        _request: Request<rpc::InterfaceSearchQuery>,
    ) -> Result<Response<rpc::InterfaceList>, Status> {
        Ok(Response::new(rpc::InterfaceList {
            interfaces: vec![rpc::MachineInterface {
                id: Some(machine_interface_id("c5ab152e-5ba6-4785-bce0-04e9711f6dc6")),
                attached_dpu_machine_id: Some(
                    "fm100ds7f2c7e5i3nlho0cfq4ke3ma8chtpn49qm6j12rv63l6fa527j8c0"
                        .parse()
                        .expect("valid machine id"),
                ),
                machine_id: Some(
                    "fm100hthn93o41u6eq8b9ijnjtpce73m8uuh7hd462gtj9p0cvl08oo5r0g"
                        .parse()
                        .expect("valid machine id"),
                ),
                segment_id: Some(network_segment_id("63ad6dcf-2a60-476b-a2c0-e3a85cd326d0")),
                hostname: "10-217-100-219".to_string(),
                domain_id: Some(domain_id("fd37cb4a-cad9-4d50-be07-b54f818dcde3")),
                primary_interface: false,
                mac_address: "9C:63:C0:E6:9F:50".to_string(),
                address: vec!["10.217.100.219".to_string()],
                vendor: None,
                created: Some(timestamp_from_secs_nanos(1773084037, 3824000)),
                last_dhcp: Some(timestamp_from_secs_nanos(1773097243, 70533000)),
                is_bmc: None,
                power_shelf_id: None,
                switch_id: None,
                association_type: Some(rpc::InterfaceAssociationType::Machine.into()),
            }],
        }))
    }

    async fn get_dpu_info_list(
        &self,
        _request: Request<rpc::GetDpuInfoListRequest>,
    ) -> Result<Response<rpc::GetDpuInfoListResponse>, Status> {
        self.state.num_get_dpu_ips.fetch_add(1, Ordering::SeqCst);

        Ok(Response::new(rpc::GetDpuInfoListResponse {
            dpu_list: vec![
                rpc::DpuInfo {
                    id: DPU_ID.to_string(),
                    loopback_ip: "172.20.0.119".to_string(),
                },
                rpc::DpuInfo {
                    id: DEST_DPU_ID.to_string(),
                    loopback_ip: "172.20.0.200".to_string(),
                },
            ],
        }))
    }

    async fn update_agent_reported_inventory(
        &self,
        _request: Request<rpc::DpuAgentInventoryReport>,
    ) -> Result<Response<()>, Status> {
        Ok(Response::new(()))
    }
}

fn build_managed_host_network_config(
    virtualization_type: VpcVirtualizationType,
) -> rpc::ManagedHostNetworkConfigResponse {
    let config_version = format!("V1-T{}", now_timestamp_micros());

    let vpc_peer_prefixes = match virtualization_type {
        VpcVirtualizationType::EthernetVirtualizer => vec!["10.217.6.176/29".to_string()],
        VpcVirtualizationType::Fnn => vec![],
        VpcVirtualizationType::EthernetVirtualizerWithNvue => vec!["10.217.6.176/29".to_string()],
    };

    let vpc_peer_vnis = match virtualization_type {
        VpcVirtualizationType::EthernetVirtualizer => vec![],
        VpcVirtualizationType::Fnn => vec![1025186, 1025197],
        VpcVirtualizationType::EthernetVirtualizerWithNvue => vec![],
    };

    let admin_interface = rpc::FlatInterfaceConfig {
        function_type: rpc::InterfaceFunctionType::Physical.into(),
        vlan_id: 10,
        vni: 10100,
        vpc_vni: 10101,
        gateway: "192.168.0.0/16".to_string(),
        ip: "192.168.0.12".to_string(),
        interface_prefix: "192.168.0.12/32".to_string(),
        virtual_function_id: None,
        vpc_prefixes: vec![],
        vpc_peer_prefixes: vec![],
        vpc_peer_vnis: vec![1025186, 1025197],
        prefix: "192.168.0.1/32".to_string(),
        fqdn: "host1".to_string(),
        booturl: None,
        svi_ip: None,
        tenant_vrf_loopback_ip: Some("10.1.1.1".to_string()),
        is_l2_segment: false,
        network_security_group: None,
        internal_uuid: None,
        mtu: None,
        ipv6_interface_config: None,
    };

    let tenant_interface = rpc::FlatInterfaceConfig {
        function_type: rpc::InterfaceFunctionType::Physical.into(),
        vlan_id: 10,
        vni: 10100,
        vpc_vni: 10101,
        gateway: "192.168.1.0/16".to_string(),
        ip: "192.168.1.12".to_string(),
        interface_prefix: "192.168.1.12/32".to_string(),
        virtual_function_id: None,
        vpc_prefixes: vec![],
        vpc_peer_prefixes,
        vpc_peer_vnis,
        prefix: "192.168.1.1/32".to_string(),
        fqdn: "host1".to_string(),
        booturl: None,
        svi_ip: None,
        tenant_vrf_loopback_ip: Some("10.1.1.1".to_string()),
        is_l2_segment: false,
        network_security_group: Some(rpc::FlatInterfaceNetworkSecurityGroupConfig {
            id: "5b931164-d9c6-11ef-8292-232e57575621".to_string(),
            version: "V1-1".to_string(),
            source: rpc::NetworkSecurityGroupSource::NsgSourceVpc.into(),
            stateful_egress: true,
            rules: vec![
                nsg_rule(
                    rpc::NetworkSecurityGroupRuleDirection::NsgRuleDirectionIngress,
                    false,
                    Some((80, 81)),
                    Some((80, 81)),
                    "0.0.0.0/0",
                    "0.0.0.0/0",
                ),
                nsg_rule_with_source_net(
                    rpc::NetworkSecurityGroupRuleDirection::NsgRuleDirectionEgress,
                    false,
                    Some((80, 81)),
                    Some((80, 81)),
                    "0.0.0.0/0",
                    "1.0.0.0/0",
                    "1.0.0.0/0",
                ),
                nsg_rule_with_source_net(
                    rpc::NetworkSecurityGroupRuleDirection::NsgRuleDirectionEgress,
                    false,
                    None,
                    Some((8080, 8080)),
                    "0.0.0.0/0",
                    "1.0.0.0/0",
                    "1.0.0.0/0",
                ),
                nsg_rule(
                    rpc::NetworkSecurityGroupRuleDirection::NsgRuleDirectionIngress,
                    true,
                    Some((80, 81)),
                    Some((80, 81)),
                    "2001:db8:3333:4444:5555:6666:7777:8888/128",
                    "2001:db8:3333:4444:5555:6666:7777:9999/128",
                ),
                nsg_rule(
                    rpc::NetworkSecurityGroupRuleDirection::NsgRuleDirectionEgress,
                    true,
                    Some((80, 81)),
                    Some((80, 81)),
                    "2001:db8:3333:4444:5555:6666:7777:8888/128",
                    "2001:db8:3333:4444:5555:6666:7777:9999/128",
                ),
            ],
        }),
        internal_uuid: None,
        mtu: None,
        ipv6_interface_config: None,
    };

    rpc::ManagedHostNetworkConfigResponse {
        bgp_leaf_session_password: Some("this_is_not_a_real_password".to_string()),
        site_global_vpc_vni: None,
        asn: 65535,
        datacenter_asn: 11414,
        common_internal_route_target: Some(rpc_common::RouteTarget {
            asn: 11415,
            vni: 200,
        }),
        additional_route_target_imports: vec![rpc_common::RouteTarget {
            asn: 11111,
            vni: 22222,
        }],
        routing_profile: Some(rpc::RoutingProfile {
            tenant_leak_communities_accepted: false,
            leak_default_route_from_underlay: false,
            leak_tenant_host_routes_to_underlay: false,
            accepted_leaks_from_underlay: vec![],
            route_target_imports: vec![rpc_common::RouteTarget {
                asn: 44444,
                vni: 55555,
            }],
            route_targets_on_exports: vec![rpc_common::RouteTarget {
                asn: 77415,
                vni: 800,
            }],
        }),
        anycast_site_prefixes: vec!["5.255.255.0/24".to_string()],
        tenant_host_asn: Some(65100),
        traffic_intercept_config: Some(rpc::TrafficInterceptConfig {
            bridging: Some(rpc::TrafficInterceptBridging {
                internal_bridge_routing_prefix: "10.255.255.0/29".to_string(),
                host_intercept_bridge_name: "br-host".to_string(),
                vf_intercept_bridge_name: "br-dpu".to_string(),
                vf_intercept_bridge_port: "pfdpu000br-dpu".to_string(),
                vf_intercept_bridge_sf: "pf0dpu5".to_string(),
                host_intercept_bridge_port: "pfdpu000br-host".to_string(),
            }),
            additional_overlay_vtep_ip: Some("10.2.2.1".to_string()),
            public_prefixes: vec!["7.8.0.0/16".to_string()],
        }),
        dhcp_servers: vec!["127.0.0.1".to_string()],
        vni_device: "".to_string(),
        managed_host_config: Some(rpc::ManagedHostNetworkConfig {
            loopback_ip: "127.0.0.1".to_string(),
            quarantine_state: None,
        }),
        managed_host_config_version: config_version.clone(),
        use_admin_network: true,
        admin_interface: Some(admin_interface),
        tenant_interfaces: vec![tenant_interface],
        network_security_policy_overrides: network_security_policy_overrides(),
        instance_network_config_version: config_version,
        instance_id: None,
        network_virtualization_type: Some(rpc_virtualization_type(virtualization_type).into()),
        vpc_vni: None,
        route_servers: vec![],
        remote_id: "".to_string(),
        deny_prefixes: vec!["1.1.1.1/32".to_string()],
        site_fabric_prefixes: vec!["2.2.2.2/32".to_string()],
        vpc_isolation_behavior: rpc::VpcIsolationBehaviorType::VpcIsolationMutual.into(),
        deprecated_deny_prefixes: vec![],
        enable_dhcp: true,
        host_interface_id: None,
        min_dpu_functioning_links: None,
        is_primary_dpu: true,
        dpu_network_pinger_type: Some("HbnExec".to_string()),
        internet_l3_vni: Some(1337),
        stateful_acls_enabled: true,
        instance: Some(instance()),
        dpu_extension_services: vec![],
    }
}

fn network_security_policy_overrides() -> Vec<rpc::ResolvedNetworkSecurityGroupRule> {
    vec![
        nsg_rule(
            rpc::NetworkSecurityGroupRuleDirection::NsgRuleDirectionIngress,
            false,
            Some((80, 81)),
            Some((80, 81)),
            "0.0.0.0/0",
            "0.0.0.0/0",
        ),
        nsg_rule_with_source_net(
            rpc::NetworkSecurityGroupRuleDirection::NsgRuleDirectionEgress,
            false,
            Some((80, 81)),
            Some((80, 81)),
            "0.0.0.0/0",
            "1.0.0.0/0",
            "1.0.0.0/0",
        ),
        nsg_rule(
            rpc::NetworkSecurityGroupRuleDirection::NsgRuleDirectionIngress,
            true,
            Some((80, 81)),
            Some((80, 81)),
            "2001:db8:3333:4444:5555:6666:7777:8888/128",
            "2001:db8:3333:4444:5555:6666:7777:9999/128",
        ),
        nsg_rule(
            rpc::NetworkSecurityGroupRuleDirection::NsgRuleDirectionEgress,
            true,
            Some((80, 81)),
            Some((80, 81)),
            "2001:db8:3333:4444:5555:6666:7777:8888/128",
            "2001:db8:3333:4444:5555:6666:7777:9999/128",
        ),
    ]
}

fn nsg_rule(
    direction: rpc::NetworkSecurityGroupRuleDirection,
    ipv6: bool,
    src_port: Option<(u32, u32)>,
    dst_port: Option<(u32, u32)>,
    src_prefix: &str,
    dst_prefix: &str,
) -> rpc::ResolvedNetworkSecurityGroupRule {
    nsg_rule_with_source_net(
        direction, ipv6, src_port, dst_port, src_prefix, src_prefix, dst_prefix,
    )
}

fn nsg_rule_with_source_net(
    direction: rpc::NetworkSecurityGroupRuleDirection,
    ipv6: bool,
    src_port: Option<(u32, u32)>,
    dst_port: Option<(u32, u32)>,
    src_prefix: &str,
    source_net_prefix: &str,
    dst_prefix: &str,
) -> rpc::ResolvedNetworkSecurityGroupRule {
    rpc::ResolvedNetworkSecurityGroupRule {
        src_prefixes: vec![src_prefix.to_string()],
        dst_prefixes: vec![dst_prefix.to_string()],
        rule: Some(rpc::NetworkSecurityGroupRuleAttributes {
            id: Some("anything".to_string()),
            direction: direction.into(),
            ipv6,
            src_port_start: src_port.map(|(start, _)| start),
            src_port_end: src_port.map(|(_, end)| end),
            dst_port_start: dst_port.map(|(start, _)| start),
            dst_port_end: dst_port.map(|(_, end)| end),
            protocol: rpc::NetworkSecurityGroupRuleProtocol::NsgRuleProtoTcp.into(),
            action: rpc::NetworkSecurityGroupRuleAction::NsgRuleActionDeny.into(),
            priority: 9001,
            source_net: Some(
                rpc::network_security_group_rule_attributes::SourceNet::SrcPrefix(
                    source_net_prefix.to_string(),
                ),
            ),
            destination_net: Some(
                rpc::network_security_group_rule_attributes::DestinationNet::DstPrefix(
                    dst_prefix.to_string(),
                ),
            ),
        }),
    }
}

fn instance() -> rpc::Instance {
    rpc::Instance {
        id: Some(instance_id("9afaedd3-b36e-4603-a029-8b94a82b89a0")),
        machine_id: Some(
            "fm100htjsaledfasinabqqer70e2ua5ksqj4kfjii0v0a90vulps48c1h7g"
                .parse()
                .expect("valid machine id"),
        ),
        metadata: None,
        instance_type_id: None,
        config: Some(rpc::InstanceConfig {
            tenant: Some(rpc::TenantConfig {
                tenant_organization_id: "Forge-simulation-tenant".to_string(),
                hostname: None,
                tenant_keyset_ids: vec![],
            }),
            os: Some(rpc::InstanceOperatingSystemConfig {
                phone_home_enabled: false,
                run_provisioning_instructions_on_every_boot: false,
                user_data: Some("".to_string()),
                variant: Some(rpc::instance_operating_system_config::Variant::Ipxe(
                    rpc::InlineIpxe {
                        ipxe_script: " chain http://10.217.126.4/public/blobs/internal/x86_64/qcow-imager.efi loglevel=7 console=ttyS0,115200 console=tty0 pci=realloc=off image_url=https://pbss.s8k.io/v1/AUTH_team-forge/images.qcow2/carbide-dev-environment/carbide-dev-environment-latest.qcow2".to_string(),
                        user_data: Some("".to_string()),
                    },
                )),
            }),
            network: Some(rpc::InstanceNetworkConfig {
                interfaces: vec![rpc::InstanceInterfaceConfig {
                    function_type: rpc::InterfaceFunctionType::Physical.into(),
                    network_segment_id: Some(network_segment_id(
                        "a7cdeab1-84ec-48a2-ab59-62863d311f26",
                    )),
                    network_details: Some(
                        rpc::instance_interface_config::NetworkDetails::SegmentId(
                            network_segment_id("a7cdeab1-84ec-48a2-ab59-62863d311f26"),
                        ),
                    ),
                    device: None,
                    device_instance: 0,
                    virtual_function_id: None,
                    ip_address: None,
                    ipv6_interface_config: None,
                }],
            }),
            infiniband: None,
            network_security_group_id: None,
            dpu_extension_services: None,
            nvlink: None,
        }),
        status: Some(rpc::InstanceStatus {
            tenant: Some(rpc::InstanceTenantStatus {
                state: rpc::TenantState::Ready.into(),
                state_details: "".to_string(),
            }),
            network: Some(rpc::InstanceNetworkStatus {
                interfaces: vec![rpc::InstanceInterfaceStatus {
                    virtual_function_id: None,
                    mac_address: Some("5C:25:73:9E:92:F2".to_string()),
                    addresses: vec!["10.217.104.146".to_string()],
                    gateways: vec!["10.217.104.145/30".to_string()],
                    prefixes: vec!["10.217.104.146/32".to_string()],
                    device: None,
                    device_instance: 0,
                }],
                configs_synced: rpc::SyncState::Synced.into(),
            }),
            infiniband: Some(rpc::InstanceInfinibandStatus {
                ib_interfaces: vec![],
                configs_synced: rpc::SyncState::Synced.into(),
            }),
            dpu_extension_services: Some(rpc::InstanceDpuExtensionServicesStatus {
                dpu_extension_services: vec![],
                configs_synced: rpc::SyncState::Synced.into(),
            }),
            nvlink: Some(rpc::InstanceNvLinkStatus {
                gpu_statuses: vec![],
                configs_synced: rpc::SyncState::Synced.into(),
            }),
            configs_synced: rpc::SyncState::Synced.into(),
            update: None,
        }),
        network_config_version: "V1-T1748645613333257".to_string(),
        ib_config_version: "V1-T1748645613333260".to_string(),
        config_version: "V1-T1748645613333260".to_string(),
        dpu_extension_service_version: "V1-T1748645613333257".to_string(),
        tpm_ek_certificate: None,
        nvlink_config_version: "V1-T1748645613333260".to_string(),
    }
}

fn rpc_virtualization_type(
    virtualization_type: VpcVirtualizationType,
) -> rpc::VpcVirtualizationType {
    match virtualization_type {
        VpcVirtualizationType::EthernetVirtualizer
        | VpcVirtualizationType::EthernetVirtualizerWithNvue => {
            rpc::VpcVirtualizationType::EthernetVirtualizer
        }
        VpcVirtualizationType::Fnn => rpc::VpcVirtualizationType::Fnn,
    }
}

fn now_timestamp_micros() -> u128 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch");

    now.as_secs() as u128 * 1_000_000 + now.subsec_micros() as u128
}

fn timestamp_from_secs_nanos(secs: i64, nanos: i32) -> Timestamp {
    let duration = Duration::from_secs(secs as u64) + Duration::from_nanos(nanos as u64);
    Timestamp::from(UNIX_EPOCH + duration)
}

fn instance_id(value: &str) -> rpc_common::InstanceId {
    rpc_common::InstanceId {
        value: value.to_string(),
    }
}

fn machine_interface_id(value: &str) -> rpc_common::MachineInterfaceId {
    rpc_common::MachineInterfaceId {
        value: value.to_string(),
    }
}

fn network_segment_id(value: &str) -> rpc_common::NetworkSegmentId {
    rpc_common::NetworkSegmentId {
        value: value.to_string(),
    }
}

fn domain_id(value: &str) -> rpc_common::DomainId {
    rpc_common::DomainId {
        value: value.to_string(),
    }
}

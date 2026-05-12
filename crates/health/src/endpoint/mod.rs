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

mod model;
mod sources;

pub use model::{
    BmcAddr, BmcCredentials, BmcEndpoint, BoxFuture, CredentialProvider, EndpointMetadata,
    EndpointSource, MachineData, PowerShelfData, SwitchData,
};
pub use sources::{CompositeEndpointSource, StaticEndpointSource};

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use carbide_uuid::power_shelf::{PowerShelfIdSource, PowerShelfType};
    use carbide_uuid::switch::{SwitchIdSource, SwitchType};
    use mac_address::MacAddress;

    use super::*;
    use crate::HealthError;
    use crate::config::{
        StaticBmcEndpoint, StaticMachineEndpoint, StaticPowerShelfEndpoint, StaticSwitchEndpoint,
    };

    fn make_test_endpoint(mac: MacAddress) -> BmcEndpoint {
        BmcEndpoint::with_fixed_credentials(
            BmcAddr {
                ip: "10.0.0.1".parse().unwrap(),
                port: Some(443),
                mac,
            },
            BmcCredentials::UsernamePassword {
                username: "admin".to_string(),
                password: Some("password".to_string()),
            },
            None,
            None,
        )
    }

    fn test_switch_id(label: &str) -> carbide_uuid::switch::SwitchId {
        let mut hash = [0u8; 32];
        let bytes = label.as_bytes();
        hash[..bytes.len().min(32)].copy_from_slice(&bytes[..bytes.len().min(32)]);
        carbide_uuid::switch::SwitchId::new(SwitchIdSource::Tpm, hash, SwitchType::NvLink)
    }

    fn test_power_shelf_id(label: &str) -> carbide_uuid::power_shelf::PowerShelfId {
        let mut hash = [0u8; 32];
        let bytes = label.as_bytes();
        hash[..bytes.len().min(32)].copy_from_slice(&bytes[..bytes.len().min(32)]);
        carbide_uuid::power_shelf::PowerShelfId::new(
            PowerShelfIdSource::ProductBoardChassisSerial,
            hash,
            PowerShelfType::Rack,
        )
    }

    #[tokio::test]
    async fn test_static_endpoint_source_shares_arc_data() {
        let endpoints = vec![
            make_test_endpoint(MacAddress::from_str("00:11:22:33:44:55").unwrap()),
            make_test_endpoint(MacAddress::from_str("aa:bb:cc:dd:ee:ff").unwrap()),
        ];
        let source = StaticEndpointSource::new(endpoints);

        let first = source.fetch_bmc_hosts().await.unwrap();
        let second = source.fetch_bmc_hosts().await.unwrap();

        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 2);
        assert!(Arc::ptr_eq(&first[0], &second[0]));
        assert!(Arc::ptr_eq(&first[1], &second[1]));
    }

    #[tokio::test]
    async fn test_composite_endpoint_source_preserves_arc_sharing() {
        let endpoints1 = vec![make_test_endpoint(
            MacAddress::from_str("00:11:22:33:44:55").unwrap(),
        )];
        let endpoints2 = vec![make_test_endpoint(
            MacAddress::from_str("aa:bb:cc:dd:ee:ff").unwrap(),
        )];

        let source1 = Arc::new(StaticEndpointSource::new(endpoints1));
        let source2 = Arc::new(StaticEndpointSource::new(endpoints2));

        let composite = CompositeEndpointSource::new(vec![source1.clone(), source2.clone()]);

        let composite_result = composite.fetch_bmc_hosts().await.unwrap();
        let source1_result = source1.fetch_bmc_hosts().await.unwrap();
        let source2_result = source2.fetch_bmc_hosts().await.unwrap();

        assert_eq!(composite_result.len(), 2);
        assert!(Arc::ptr_eq(&composite_result[0], &source1_result[0]));
        assert!(Arc::ptr_eq(&composite_result[1], &source2_result[0]));
    }

    #[test]
    fn test_to_url_uses_http_for_port_80_and_https_otherwise() {
        let addr_http = BmcAddr {
            ip: "10.0.0.1".parse().expect("valid ip"),
            port: Some(80),
            mac: MacAddress::from_str("00:11:22:33:44:55").unwrap(),
        };
        let addr_https = BmcAddr {
            ip: "10.0.0.2".parse().expect("valid ip"),
            port: Some(443),
            mac: MacAddress::from_str("aa:bb:cc:dd:ee:ff").unwrap(),
        };

        let url_http = addr_http.to_url().expect("url should build");
        let url_https = addr_https.to_url().expect("url should build");

        assert_eq!(url_http.scheme(), "http");
        assert_eq!(url_https.scheme(), "https");
    }

    #[tokio::test]
    async fn test_static_endpoint_source_filters_invalid_ip() {
        let configs = vec![
            StaticBmcEndpoint {
                ip: "10.0.0.1".to_string(),
                port: Some(443),
                mac: "00:11:22:33:44:55".to_string(),
                username: "admin".to_string(),
                password: Some("pass".to_string()),
                machine: None,
                power_shelf: None,
                switch: None,
                rack_id: None,
            },
            StaticBmcEndpoint {
                ip: "not-an-ip".to_string(),
                port: Some(443),
                mac: "aa:bb:cc:dd:ee:ff".to_string(),
                username: "admin".to_string(),
                password: Some("pass".to_string()),
                machine: None,
                power_shelf: None,
                switch: None,
                rack_id: None,
            },
        ];

        let source = StaticEndpointSource::from_config(&configs);
        let endpoints = source.fetch_bmc_hosts().await.expect("fetch should work");

        assert_eq!(endpoints.len(), 1);
        assert_eq!(
            endpoints[0].addr.mac,
            MacAddress::from_str("00:11:22:33:44:55").unwrap()
        );
    }

    #[tokio::test]
    async fn test_static_endpoint_with_switch_serial_sets_metadata() {
        let switch_id = test_switch_id("switch-a");
        let configs = vec![StaticBmcEndpoint {
            ip: "10.0.1.1".to_string(),
            port: Some(443),
            mac: "11:22:33:44:55:66".to_string(),
            username: "cumulus".to_string(),
            password: Some("pass".to_string()),
            machine: None,
            power_shelf: None,
            switch: Some(StaticSwitchEndpoint {
                id: Some(switch_id.to_string()),
                serial: Some("SN-001".to_string()),
            }),
            rack_id: None,
        }];

        let source = StaticEndpointSource::from_config(&configs);
        let endpoints = source.fetch_bmc_hosts().await.unwrap();

        assert_eq!(endpoints.len(), 1);
        match &endpoints[0].metadata {
            Some(EndpointMetadata::Switch(s)) => {
                assert_eq!(s.id, Some(switch_id));
                assert_eq!(s.serial, "SN-001");
            }
            other => panic!("expected Switch metadata, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_static_endpoint_with_power_shelf_metadata() {
        let power_shelf_id = test_power_shelf_id("power-shelf-a");
        let configs = vec![StaticBmcEndpoint {
            ip: "10.0.2.1".to_string(),
            port: Some(443),
            mac: "22:33:44:55:66:77".to_string(),
            username: "admin".to_string(),
            password: Some("pass".to_string()),
            machine: None,
            power_shelf: Some(StaticPowerShelfEndpoint {
                id: Some(power_shelf_id.to_string()),
                serial: Some("PS-001".to_string()),
            }),
            switch: None,
            rack_id: None,
        }];

        let source = StaticEndpointSource::from_config(&configs);
        let endpoints = source.fetch_bmc_hosts().await.unwrap();

        assert_eq!(endpoints.len(), 1);
        match &endpoints[0].metadata {
            Some(EndpointMetadata::PowerShelf(power_shelf)) => {
                assert_eq!(power_shelf.id, Some(power_shelf_id));
                assert_eq!(power_shelf.serial, "PS-001");
            }
            other => panic!("expected PowerShelf metadata, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_static_machine_endpoint_sets_placement_and_nvlink_metadata() {
        let machine_id = "fm100htjtiaehv1n5vh67tbmqq4eabcjdng40f7jupsadbedhruh6rag1l0"
            .parse()
            .expect("valid machine id");
        let domain_uuid = "00000000-0000-0000-0000-000000000000"
            .parse()
            .expect("valid NVLink domain UUID");
        let configs = vec![StaticBmcEndpoint {
            ip: "10.0.1.2".to_string(),
            port: Some(443),
            mac: "11:22:33:44:55:11".to_string(),
            username: "admin".to_string(),
            password: Some("pass".to_string()),
            machine: Some(StaticMachineEndpoint {
                id: "fm100htjtiaehv1n5vh67tbmqq4eabcjdng40f7jupsadbedhruh6rag1l0".to_string(),
                serial: Some("MN-001".to_string()),
                slot_number: Some(15),
                tray_index: Some(5),
                nvlink_domain_uuid: Some("00000000-0000-0000-0000-000000000000".to_string()),
            }),
            power_shelf: None,
            switch: None,
            rack_id: Some("RACK_1".to_string()),
        }];

        let source = StaticEndpointSource::from_config(&configs);
        let endpoints = source.fetch_bmc_hosts().await.unwrap();

        assert_eq!(endpoints.len(), 1);
        assert_eq!(
            endpoints[0]
                .rack_id
                .as_ref()
                .map(|rack_id| rack_id.as_str()),
            Some("RACK_1")
        );
        match &endpoints[0].metadata {
            Some(EndpointMetadata::Machine(machine)) => {
                assert_eq!(machine.machine_id, machine_id);
                assert_eq!(machine.machine_serial.as_deref(), Some("MN-001"));
                assert_eq!(machine.slot_number, Some(15));
                assert_eq!(machine.tray_index, Some(5));
                assert_eq!(machine.nvlink_domain_uuid, Some(domain_uuid));
            }
            other => panic!("expected Machine metadata, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_static_endpoint_without_switch_serial_has_no_metadata() {
        let configs = vec![StaticBmcEndpoint {
            ip: "10.0.0.1".to_string(),
            port: Some(443),
            mac: "aa:bb:cc:dd:ee:ff".to_string(),
            username: "admin".to_string(),
            password: Some("pass".to_string()),
            machine: None,
            power_shelf: None,
            switch: None,
            rack_id: None,
        }];

        let source = StaticEndpointSource::from_config(&configs);
        let endpoints = source.fetch_bmc_hosts().await.unwrap();

        assert_eq!(endpoints.len(), 1);
        assert!(endpoints[0].metadata.is_none());
    }

    struct FailingSource;

    impl EndpointSource for FailingSource {
        fn fetch_bmc_hosts<'a>(
            &'a self,
        ) -> BoxFuture<'a, Result<Vec<Arc<BmcEndpoint>>, HealthError>> {
            Box::pin(async {
                Err(HealthError::GenericError(
                    "simulated endpoint source failure".to_string(),
                ))
            })
        }
    }

    #[tokio::test]
    async fn test_composite_endpoint_source_propagates_errors() {
        let source_ok = Arc::new(StaticEndpointSource::new(vec![make_test_endpoint(
            MacAddress::from_str("00:11:22:33:44:55").unwrap(),
        )]));
        let source_fail = Arc::new(FailingSource);
        let composite = CompositeEndpointSource::new(vec![source_ok, source_fail]);

        let result = composite.fetch_bmc_hosts().await;

        assert!(result.is_err());
    }
}

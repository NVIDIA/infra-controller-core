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

use std::fs;
use std::io::Write;
use std::time::Duration;

use axum::http::StatusCode;
use carbide_agent_mock_api_server::{MockApiServer, MockUpgradeCheckResponse};
use carbide_network::virtualization::VpcVirtualizationType;
use eyre::WrapErr;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::rt::TokioExecutor;
use tokio::task::JoinSet;

use crate::tests::common;
use crate::traffic_intercept_bridging;
use crate::util::compare_lines;

#[derive(Default, Debug)]
struct TestOut {
    is_skip: bool,
    hbn_root_dir: Option<tempfile::TempDir>,
}

// test_etv_nvue tests that config is being generated successfully
// for the OG networking config, but using nvue templating mechanism.
// NOTE: This is currently a _very_ light test because it takes the
// UseAdminNetwork paths in the template, which leaves out a lot
// of config.  Some of what's missing seems to be covered in
// ethernet_virtualization tests, though.
#[tokio::test(flavor = "multi_thread")]
async fn test_etv_nvue() -> eyre::Result<()> {
    let expected = include_str!("../../templates/tests/full_nvue_startup_etv.yaml.expected");
    test_nvue_generic(VpcVirtualizationType::EthernetVirtualizer, expected).await
}

// test_fnn_l3 tests that config is being generated successfully
// via nvue templating against the FNN L3 template.
#[tokio::test(flavor = "multi_thread")]
async fn test_fnn_l3() -> eyre::Result<()> {
    let expected = include_str!("../../templates/tests/full_nvue_startup_fnn_l3.yaml.expected");
    test_nvue_generic(VpcVirtualizationType::Fnn, expected).await
}

#[tokio::test(flavor = "multi_thread")]
async fn test_traffic_intercept_bridging() -> eyre::Result<()> {
    let expected = include_str!("../../templates/tests/update_intercept_bridging.sh.expected");
    let bridging = traffic_intercept_bridging::build(
        traffic_intercept_bridging::TrafficInterceptBridgingConfig {
            secondary_overlay_vtep_ip: "1.1.1.1".to_string(),
            vf_intercept_bridge_ip: "10.10.10.2".to_string(),
            vf_intercept_bridge_name: "pfdpu000br-dpu".to_string(),
            intercept_bridge_prefix_len: 29,
        },
    )?;

    let r = compare_lines(bridging.as_str(), expected, None);
    eprint!("Diff output:\n{}", r.report());
    assert!(
        r.is_identical(),
        "generated bridging script does not match expected bridging script"
    );

    Ok(())
}

// All of the new tests are leveraging nvue for configs, regardless
// of template, so have a test_nvue_generic that just takes a virtualization
// type.
async fn test_nvue_generic(
    virtualization_type: VpcVirtualizationType,
    expected: &str,
) -> eyre::Result<()> {
    let out = run_common_parts(virtualization_type, false).await?;
    if out.is_skip {
        return Ok(());
    }

    // Make sure the nvue startup file was written where
    // it was supposed to be written (crate::nvue::PATH
    // within the test-specific temp dir).
    let td = out.hbn_root_dir.unwrap();
    let hbn_root = td.path();
    let startup_yaml = hbn_root.join(crate::nvue::PATH);
    assert!(
        startup_yaml.exists(),
        "could not find {} startup_yaml at path: {:?}",
        virtualization_type,
        startup_yaml.to_str()
    );

    // And now check that the output nvue config YAML
    // is actually valid YAML. If it's not, write out
    // whatever the error is to ERR_FILE, so we can go
    // check and see what's up.
    const ERR_FILE: &str = "/tmp/test_nvue_startup.yaml";
    let startup_yaml = fs::read_to_string(startup_yaml)?;
    let yaml_obj: Vec<serde_yaml::Value> = serde_yaml::from_str(&startup_yaml)
        .inspect_err(|_| {
            let mut f = fs::File::create(ERR_FILE).unwrap();
            f.write_all(startup_yaml.as_bytes()).unwrap();
        })
        .wrap_err(format!("YAML parser error. Output written to {ERR_FILE}"))?;
    assert_eq!(yaml_obj.len(), 2); // 'header' and 'set'

    let r = compare_lines(startup_yaml.as_str(), expected, None);
    eprint!("Diff output:\n{}", r.report());
    assert!(
        r.is_identical(),
        "generated startup_yaml does not match expected startup_yaml for {virtualization_type}"
    );

    Ok(())
}

// Query the FMDS endpoint to retrieve tenant metadata
// and make sure it matches expected values. run_common_parts
// launches the forge_dpu_agent, and by passing in true to run_common_parts,
// we are asking it to launch the metadata service. run_common_parts also launches
// a gRPC server that returns data in response to GetManagedHostNetworkConfig call,
// and that data populates the data retrieved by the metadata endpoint server.
#[tokio::test(flavor = "multi_thread")]
// Test retrieving instance metadata using FMDS
pub async fn test_fmds_get_data() -> eyre::Result<()> {
    let out = run_common_parts(VpcVirtualizationType::EthernetVirtualizer, true).await?;
    if out.is_skip {
        return Ok(());
    }

    // Test get hostname
    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build_http();
    let request: hyper::Request<Full<Bytes>> = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri("http://0.0.0.0:7777/latest/meta-data/hostname".to_string())
        .body("".into())
        .unwrap();

    let response = client.request(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();

    assert_eq!(body_str, "9afaedd3-b36e-4603-a029-8b94a82b89a0");

    // Test get machine_id
    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build_http();
    let request: hyper::Request<Full<Bytes>> = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri("http://0.0.0.0:7777/latest/meta-data/machine-id".to_string())
        .body("".into())
        .unwrap();

    let response = client.request(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();

    assert_eq!(
        body_str,
        "fm100htjsaledfasinabqqer70e2ua5ksqj4kfjii0v0a90vulps48c1h7g"
    );

    // Test get instance-id
    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build_http();
    let request: hyper::Request<Full<Bytes>> = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri("http://0.0.0.0:7777/latest/meta-data/instance-id".to_string())
        .body("".into())
        .unwrap();

    let response = client.request(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();

    assert_eq!(body_str, "9afaedd3-b36e-4603-a029-8b94a82b89a0");

    // Test get asn
    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build_http();
    let request: hyper::Request<Full<Bytes>> = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri("http://0.0.0.0:7777/latest/meta-data/asn".to_string())
        .body("".into())
        .unwrap();

    let response = client.request(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();

    assert_eq!(body_str, "65535");

    // Test get sitename
    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build_http();
    let request: hyper::Request<Full<Bytes>> = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri("http://0.0.0.0:7777/latest/meta-data/sitename".to_string())
        .body("".into())
        .unwrap();

    let response = client.request(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();

    assert_eq!(body_str, "testsite");

    Ok(())
}

// run_common_parts exists, because most of the test is
// shared between the [legacy] ETV files mechanism and the
// new nvue templating mechanism.
async fn run_common_parts(
    virtualization_type: VpcVirtualizationType,
    test_metadata_service: bool,
) -> eyre::Result<TestOut> {
    carbide_host_support::init_logging()?;

    let mut join_set = JoinSet::new();
    let mock_server = MockApiServer::new()
        .with_virtualization_type(virtualization_type)
        .with_upgrade_response(MockUpgradeCheckResponse::no_upgrade_current_build());
    let mock_server_handle = mock_server.spawn(&mut join_set).await?;
    let addr = mock_server_handle.addr;

    let td: tempfile::TempDir = tempfile::tempdir()?;
    let agent_config_file = tempfile::NamedTempFile::new()?;
    let opts =
        match common::setup_agent_run_env(&addr, &td, &agent_config_file, test_metadata_service) {
            Ok(Some(opts)) => opts,
            Ok(None) => {
                return Ok(TestOut {
                    is_skip: true,
                    ..Default::default()
                });
            }
            Err(e) => {
                return Err(e);
            }
        };

    // Start forge-dpu-agent
    tokio::spawn(async move {
        if let Err(e) = crate::start(opts).await {
            tracing::error!("Failed to start DPU agent: {:#}", e);
        }
    });

    // Wait until we report health at least 2 times
    // At that point in time the first configuration should have been applied
    // and the check for updates should have occured
    let start = std::time::Instant::now();
    loop {
        if mock_server_handle.num_health_reports() > 1
            && mock_server_handle.num_netconf_fetches() > 1
        {
            break;
        }

        if start.elapsed() > std::time::Duration::from_secs(60) {
            return Err(eyre::eyre!(
                "Health report was not sent 2 times in 60s. health_reports={}, netconf_fetches={}",
                mock_server_handle.num_health_reports(),
                mock_server_handle.num_netconf_fetches(),
            ));
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // The gRPC calls were made
    assert!(mock_server_handle.has_discovered());
    assert!(mock_server_handle.has_checked_for_upgrade());
    assert!(mock_server_handle.num_health_reports() > 1);
    // Since Network config fetching runs in a separate task, it might not have
    // happened 2 times but just a single time
    assert!(mock_server_handle.num_netconf_fetches() > 0);
    assert!(mock_server_handle.num_get_dpu_ips() > 0);

    std::mem::drop(mock_server_handle);
    join_set.join_all().await;

    Ok(TestOut {
        is_skip: false,
        hbn_root_dir: Some(td),
    })
}

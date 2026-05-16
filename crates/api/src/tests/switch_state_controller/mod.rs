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

use std::sync::Arc;
use std::time::Duration;

use carbide_uuid::switch::SwitchId;
use db::switch as db_switch;
use forge_secrets::credentials::{
    CredentialKey, CredentialReader, CredentialWriter, Credentials, TestCredentialManager,
};
use librms::protos::rack_manager as rms;
use model::switch::{ConfiguringState, SwitchControllerState};
use rpc::forge::forge_server::Forge;
use tokio_util::sync::CancellationToken;

use crate::state_controller::common_services::CommonStateHandlerServices;
use crate::state_controller::config::IterationConfig;
use crate::state_controller::controller::StateController;
use crate::state_controller::switch::handler::SwitchStateHandler;
use crate::state_controller::switch::io::SwitchStateControllerIO;
use crate::tests::common;
use crate::tests::common::api_fixtures::create_test_env;

mod fixtures;
use fixtures::switch::{mark_switch_as_deleted, set_switch_controller_state};

fn switch_password_response(
    switch_id: &SwitchId,
    status: rms::ReturnCode,
    error_message: &str,
) -> rms::UpdateSwitchSystemPasswordResponse {
    let success = status == rms::ReturnCode::Success;
    rms::UpdateSwitchSystemPasswordResponse {
        response: Some(rms::NodeBatchResponse {
            status: status as i32,
            message: if success {
                "Updated switch system password on 1 of 1 devices".to_string()
            } else {
                "Updated switch system password on 0 of 1 devices".to_string()
            },
            total_nodes: 1,
            successful_nodes: i32::from(success),
            failed_nodes: i32::from(!success),
            node_results: vec![rms::NodeResult {
                node_id: switch_id.to_string(),
                status: status as i32,
                error_message: error_message.to_string(),
            }],
            ..Default::default()
        }),
    }
}

async fn create_configuring_switch_with_nvos_password(
    env: &common::api_fixtures::TestEnv,
    pool: &sqlx::PgPool,
) -> Result<(SwitchId, mac_address::MacAddress), Box<dyn std::error::Error>> {
    let switch_id = common::api_fixtures::site_explorer::new_switch(
        env,
        Some("Switch4".to_string()),
        Some("Data Center A, Rack 1".to_string()),
    )
    .await?;

    let mut txn = pool.begin().await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    let bmc_mac_address = switch
        .bmc_mac_address
        .expect("test switch should have a BMC MAC address");
    set_switch_controller_state(
        txn.as_mut(),
        &switch_id,
        SwitchControllerState::Configuring {
            config_state: ConfiguringState::RotateOsPassword,
        },
    )
    .await?;
    txn.commit().await?;

    Ok((switch_id, bmc_mac_address))
}

#[crate::sqlx_test]
async fn test_switch_state_transition_validation(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;

    // Create a switch
    let switch_id = common::api_fixtures::site_explorer::new_switch(
        &env,
        Some("Switch2".to_string()),
        Some("Data Center A, Rack 1".to_string()),
    )
    .await?;

    // Verify initial state is Initializing
    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id).await?;
    assert!(switch.is_some());
    let switch = switch.unwrap();
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::Created
    ));

    // Test state transitions by manually setting different states
    let states = vec![
        SwitchControllerState::Configuring {
            config_state: ConfiguringState::RotateOsPassword,
        },
        SwitchControllerState::Ready,
        SwitchControllerState::Error {
            cause: "Test error".to_string(),
        },
    ];

    for state in states {
        set_switch_controller_state(pool.acquire().await?.as_mut(), &switch_id, state.clone())
            .await?;

        // Verify the state was set correctly
        let mut txn = pool.acquire().await?;
        let switch = db_switch::find_by_id(&mut txn, &switch_id).await?;
        assert!(switch.is_some());
        let switch = switch.unwrap();
        assert!(
            matches!(switch.controller_state.value, _ if switch.controller_state.value == state)
        );
    }

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_rotate_os_password_calls_rms_and_persists_admin_credentials(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let (switch_id, bmc_mac_address) =
        create_configuring_switch_with_nvos_password(&env, &pool).await?;
    let credential_key = CredentialKey::SwitchNvosAdmin { bmc_mac_address };

    env.test_credential_manager
        .set_credentials(
            &credential_key,
            &Credentials::UsernamePassword {
                username: "admin".to_string(),
                password: "old-pass".to_string(),
            },
        )
        .await
        .expect("failed to seed existing NVOS credentials");
    env.rms_sim
        .queue_update_switch_system_password_response(Ok(switch_password_response(
            &switch_id,
            rms::ReturnCode::Success,
            "",
        )))
        .await;

    env.run_switch_controller_iteration().await;

    let requests = env
        .rms_sim
        .submitted_switch_system_password_requests()
        .await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.username, "admin");
    assert_eq!(request.password, "nvos_pass1");

    let nodes = request.nodes.as_ref().expect("nodes should be set");
    assert_eq!(nodes.devices.len(), 1);
    let device = &nodes.devices[0];
    assert_eq!(device.node_id, switch_id.to_string());
    assert_eq!(device.r#type, Some(rms::NodeType::Switch as i32));
    assert!(
        device
            .bmc_endpoint
            .as_ref()
            .and_then(|endpoint| endpoint.interface.as_ref())
            .is_some(),
        "BMC endpoint interface should be populated"
    );
    let host_credentials = device
        .host_endpoint
        .as_ref()
        .and_then(|endpoint| endpoint.credentials.as_ref())
        .and_then(|credentials| credentials.auth.as_ref())
        .expect("host credentials should be set");
    match host_credentials {
        rms::credentials::Auth::UserPass(user_pass) => {
            assert_eq!(user_pass.username, "admin");
            assert_eq!(user_pass.password, "old-pass");
        }
        rms::credentials::Auth::SessionToken(_) => {
            panic!("host credentials should use username/password")
        }
    }

    let credentials = env
        .test_credential_manager
        .get_credentials(&credential_key)
        .await
        .expect("failed to read rotated credentials from vault")
        .expect("rotated credentials should be stored in vault");
    assert_eq!(
        credentials,
        Credentials::UsernamePassword {
            username: "admin".to_string(),
            password: "nvos_pass1".to_string(),
        }
    );

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::Validating { .. }
    ));

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_rotate_os_password_rms_failure_does_not_update_vault(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let (switch_id, bmc_mac_address) =
        create_configuring_switch_with_nvos_password(&env, &pool).await?;
    let credential_key = CredentialKey::SwitchNvosAdmin { bmc_mac_address };
    let existing_credentials = Credentials::UsernamePassword {
        username: "admin".to_string(),
        password: "old-pass".to_string(),
    };

    env.test_credential_manager
        .set_credentials(&credential_key, &existing_credentials)
        .await
        .expect("failed to seed existing NVOS credentials");
    env.rms_sim
        .queue_update_switch_system_password_response(Ok(switch_password_response(
            &switch_id,
            rms::ReturnCode::Failure,
            "mock rotation failed",
        )))
        .await;

    env.run_switch_controller_iteration().await;

    let credentials = env
        .test_credential_manager
        .get_credentials(&credential_key)
        .await
        .expect("failed to read existing credentials from vault")
        .expect("existing credentials should remain in vault");
    assert_eq!(credentials, existing_credentials);

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::Error { ref cause } if cause.contains("mock rotation failed")
    ));

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_rotate_os_password_rms_transport_error_does_not_update_vault(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let (switch_id, bmc_mac_address) =
        create_configuring_switch_with_nvos_password(&env, &pool).await?;
    let credential_key = CredentialKey::SwitchNvosAdmin { bmc_mac_address };
    let existing_credentials = Credentials::UsernamePassword {
        username: "admin".to_string(),
        password: "old-pass".to_string(),
    };

    env.test_credential_manager
        .set_credentials(&credential_key, &existing_credentials)
        .await
        .expect("failed to seed existing NVOS credentials");
    env.rms_sim
        .queue_update_switch_system_password_response(Err(
            librms::RackManagerError::ApiInvocationError(tonic::Status::unavailable("rms down")),
        ))
        .await;

    env.run_switch_controller_iteration().await;

    let credentials = env
        .test_credential_manager
        .get_credentials(&credential_key)
        .await
        .expect("failed to read existing credentials from vault")
        .expect("existing credentials should remain in vault");
    assert_eq!(credentials, existing_credentials);

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::Error { ref cause } if cause.contains("rms down")
    ));

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_deletion_with_state_controller(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;

    // Create a switch
    let switch_id = common::api_fixtures::site_explorer::new_switch(
        &env,
        Some("Switch1".to_string()),
        Some("Data Center A, Rack 1".to_string()),
    )
    .await?;

    // Start the state controller
    let switch_handler = Arc::new(SwitchStateHandler::default());
    const ITERATION_TIME: Duration = Duration::from_millis(50);

    let handler_services = Arc::new(CommonStateHandlerServices {
        db_pool: pool.clone(),
        db_reader: pool.clone().into(),
        redfish_client_pool: env.redfish_sim.clone(),
        ib_fabric_manager: env.ib_fabric_manager.clone(),
        ib_pools: env.common_pools.infiniband.clone(),
        ipmi_tool: env.ipmi_tool.clone(),
        site_config: env.config.clone(),
        dpa_info: None,
        rms_client: None,
        switch_system_image_rms_client: None,
        credential_manager: Arc::new(TestCredentialManager::default()),
    });

    let cancel_token = CancellationToken::new();
    let mut controller = StateController::<SwitchStateControllerIO>::builder()
        .iteration_config(IterationConfig {
            iteration_time: ITERATION_TIME,
            processor_dispatch_interval: Duration::from_millis(10),
            ..Default::default()
        })
        .database(pool.clone(), env.api.work_lock_manager_handle.clone())
        .processor_id(uuid::Uuid::new_v4().to_string())
        .services(handler_services.clone())
        .state_handler(switch_handler.clone())
        .build_for_manual_iterations(cancel_token.clone())
        .unwrap();

    // Walk through state machine
    for _ in 0..20 {
        controller.run_single_iteration().await;
    }

    let switch = env
        .api
        .find_switches_by_ids(tonic::Request::new(rpc::forge::SwitchesByIdsRequest {
            switch_ids: vec![switch_id],
        }))
        .await?
        .into_inner()
        .switches
        .remove(0);
    assert_eq!(switch.controller_state, "{\"state\":\"ready\"}".to_string());

    // Mark the switch as deleted
    mark_switch_as_deleted(pool.acquire().await?.as_mut(), &switch_id).await?;

    // Walk through state machine
    for _ in 0..20 {
        controller.run_single_iteration().await;
    }

    // Verify that the DB object is gone
    let switches = env
        .api
        .find_switches_by_ids(tonic::Request::new(rpc::forge::SwitchesByIdsRequest {
            switch_ids: vec![switch_id],
        }))
        .await?
        .into_inner()
        .switches;
    assert!(switches.is_empty());

    Ok(())
}

/// Tests the entire Switch ControllerState transition flow: Initializing -> Configuring
/// (RotateOsPassword) -> Validating (ValidationComplete) -> BomValidating
/// (BomValidationComplete) -> Ready. Uses the real SwitchStateHandler so each state handler
/// performs its transition.
#[crate::sqlx_test]
async fn test_switch_entire_state_transition_flow(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;

    let switch_id = common::api_fixtures::site_explorer::new_switch(
        &env,
        Some("Switch3".to_string()),
        Some("Data Center A, Rack 1".to_string()),
    )
    .await?;

    // Verify initial state is Initializing
    {
        let mut txn = pool.acquire().await?;
        let switch = db_switch::find_by_id(&mut txn, &switch_id).await?;
        let switch = switch.expect("switch should exist");
        assert!(
            matches!(
                switch.controller_state.value,
                SwitchControllerState::Created
            ),
            "initial state should be Created, got {:?}",
            switch.controller_state.value
        );
    }

    // Start the state controller with the real handler
    let switch_handler = Arc::new(SwitchStateHandler::default());
    const ITERATION_TIME: Duration = Duration::from_millis(50);

    let handler_services = Arc::new(env.state_handler_services());

    let cancel_token = CancellationToken::new();
    let mut controller = StateController::<SwitchStateControllerIO>::builder()
        .iteration_config(IterationConfig {
            iteration_time: ITERATION_TIME,
            processor_dispatch_interval: Duration::from_millis(10),
            ..Default::default()
        })
        .database(pool.clone(), env.api.work_lock_manager_handle.clone())
        .processor_id(uuid::Uuid::new_v4().to_string())
        .services(handler_services.clone())
        .state_handler(switch_handler.clone())
        .build_for_manual_iterations(cancel_token.clone())
        .unwrap();

    // iterate a few times
    controller.run_single_iteration().await;
    controller.run_single_iteration().await;
    controller.run_single_iteration().await;
    controller.run_single_iteration().await;
    controller.run_single_iteration().await;
    controller.run_single_iteration().await;

    // Final assertion: state is Ready
    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id).await?;
    let switch = switch.expect("switch should exist");
    assert!(
        matches!(switch.controller_state.value, SwitchControllerState::Ready),
        "expected Ready, got {:?}",
        switch.controller_state.value
    );

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_waiting_for_rack_firmware_upgrade_waits_for_terminal_status(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = common::api_fixtures::site_explorer::new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_reprovisioning_requested(txn.as_mut(), switch_id, "rack-test").await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    let requested_at = switch
        .switch_reprovisioning_requested
        .as_ref()
        .expect("switch reprovision request should exist")
        .requested_at;
    db_switch::try_update_controller_state(
        txn.as_mut(),
        switch_id,
        switch.controller_state.version,
        switch.controller_state.version.increment(),
        &SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForRackFirmwareUpgrade,
        },
    )
    .await?;
    db_switch::update_firmware_upgrade_status(
        txn.as_mut(),
        switch_id,
        Some(&model::rack::RackFirmwareUpgradeStatus {
            task_id: "rack-job".to_string(),
            status: model::rack::RackFirmwareUpgradeState::InProgress,
            started_at: Some(requested_at),
            ended_at: None,
        }),
    )
    .await?;
    txn.commit().await?;

    env.run_switch_controller_iteration().await;

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForRackFirmwareUpgrade,
        }
    ));
    assert!(switch.switch_reprovisioning_requested.is_some());

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_waiting_for_rack_firmware_upgrade_transitions_to_waiting_for_nvos_on_completion(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = common::api_fixtures::site_explorer::new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_reprovisioning_requested(txn.as_mut(), switch_id, "rack-test").await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    let requested_at = switch
        .switch_reprovisioning_requested
        .as_ref()
        .expect("switch reprovision request should exist")
        .requested_at;
    db_switch::try_update_controller_state(
        txn.as_mut(),
        switch_id,
        switch.controller_state.version,
        switch.controller_state.version.increment(),
        &SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForRackFirmwareUpgrade,
        },
    )
    .await?;
    db_switch::update_firmware_upgrade_status(
        txn.as_mut(),
        switch_id,
        Some(&model::rack::RackFirmwareUpgradeStatus {
            task_id: "rack-job".to_string(),
            status: model::rack::RackFirmwareUpgradeState::Completed,
            started_at: Some(requested_at),
            ended_at: Some(chrono::Utc::now()),
        }),
    )
    .await?;
    txn.commit().await?;

    env.run_switch_controller_iteration().await;

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForNVOSUpgrade,
        }
    ));
    assert!(switch.switch_reprovisioning_requested.is_some());

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_waiting_for_rack_firmware_upgrade_returns_ready_for_firmware_only_request(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = common::api_fixtures::site_explorer::new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_reprovisioning_requested_with_firmware_continuation(
        txn.as_mut(),
        switch_id,
        "rack-test",
        false,
    )
    .await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    let requested_at = switch
        .switch_reprovisioning_requested
        .as_ref()
        .expect("switch reprovision request should exist")
        .requested_at;
    db_switch::try_update_controller_state(
        txn.as_mut(),
        switch_id,
        switch.controller_state.version,
        switch.controller_state.version.increment(),
        &SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForRackFirmwareUpgrade,
        },
    )
    .await?;
    db_switch::update_firmware_upgrade_status(
        txn.as_mut(),
        switch_id,
        Some(&model::rack::RackFirmwareUpgradeStatus {
            task_id: "rack-job".to_string(),
            status: model::rack::RackFirmwareUpgradeState::Completed,
            started_at: Some(requested_at),
            ended_at: Some(chrono::Utc::now()),
        }),
    )
    .await?;
    txn.commit().await?;

    env.run_switch_controller_iteration().await;

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::Ready,
    ));
    assert!(switch.switch_reprovisioning_requested.is_none());

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_waiting_for_rack_firmware_upgrade_accepts_completion_when_only_ended_at_is_current(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = common::api_fixtures::site_explorer::new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_reprovisioning_requested(txn.as_mut(), switch_id, "rack-test").await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    let requested_at = switch
        .switch_reprovisioning_requested
        .as_ref()
        .expect("switch reprovision request should exist")
        .requested_at;
    db_switch::try_update_controller_state(
        txn.as_mut(),
        switch_id,
        switch.controller_state.version,
        switch.controller_state.version.increment(),
        &SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForRackFirmwareUpgrade,
        },
    )
    .await?;
    db_switch::update_firmware_upgrade_status(
        txn.as_mut(),
        switch_id,
        Some(&model::rack::RackFirmwareUpgradeStatus {
            task_id: "rack-job".to_string(),
            status: model::rack::RackFirmwareUpgradeState::Completed,
            started_at: Some(requested_at - chrono::Duration::seconds(1)),
            ended_at: Some(requested_at + chrono::Duration::seconds(1)),
        }),
    )
    .await?;
    txn.commit().await?;

    env.run_switch_controller_iteration().await;

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForNVOSUpgrade,
        }
    ));
    assert!(switch.switch_reprovisioning_requested.is_some());

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_ready_routes_rack_requests_to_waiting_for_rack_firmware_upgrade(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = common::api_fixtures::site_explorer::new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_reprovisioning_requested(txn.as_mut(), switch_id, "rack-test").await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    db_switch::try_update_controller_state(
        txn.as_mut(),
        switch_id,
        switch.controller_state.version,
        switch.controller_state.version.increment(),
        &SwitchControllerState::Ready,
    )
    .await?;
    txn.commit().await?;

    env.run_switch_controller_iteration().await;

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForRackFirmwareUpgrade,
        }
    ));

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_waiting_for_nvos_upgrade_transitions_to_waiting_for_nmxc_on_completion(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = common::api_fixtures::site_explorer::new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_reprovisioning_requested(txn.as_mut(), switch_id, "rack-nvos-test")
        .await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    let requested_at = switch
        .switch_reprovisioning_requested
        .as_ref()
        .expect("switch reprovision request should exist")
        .requested_at;
    db_switch::try_update_controller_state(
        txn.as_mut(),
        switch_id,
        switch.controller_state.version,
        switch.controller_state.version.increment(),
        &SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForNVOSUpgrade,
        },
    )
    .await?;
    db_switch::update_nvos_update_status(
        txn.as_mut(),
        switch_id,
        Some(&model::switch::SwitchNvosUpdateStatus {
            task_id: "nvos-job".to_string(),
            firmware_id: "fw-1".to_string(),
            image_filename: "nvos-image.bin".to_string(),
            status: model::switch::SwitchNvosUpdateState::Completed,
            started_at: Some(requested_at),
            ended_at: Some(chrono::Utc::now()),
        }),
    )
    .await?;
    txn.commit().await?;

    env.run_switch_controller_iteration().await;

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForNMXCConfigure,
        }
    ));
    assert!(switch.switch_reprovisioning_requested.is_some());

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_waiting_for_nvos_upgrade_waits_for_current_cycle_status(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = common::api_fixtures::site_explorer::new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_reprovisioning_requested(txn.as_mut(), switch_id, "rack-nvos-test")
        .await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    let requested_at = switch
        .switch_reprovisioning_requested
        .as_ref()
        .expect("switch reprovision request should exist")
        .requested_at;
    db_switch::try_update_controller_state(
        txn.as_mut(),
        switch_id,
        switch.controller_state.version,
        switch.controller_state.version.increment(),
        &SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForNVOSUpgrade,
        },
    )
    .await?;
    db_switch::update_nvos_update_status(
        txn.as_mut(),
        switch_id,
        Some(&model::switch::SwitchNvosUpdateStatus {
            task_id: "old-nvos-job".to_string(),
            firmware_id: "old-fw".to_string(),
            image_filename: "old-nvos-image.bin".to_string(),
            status: model::switch::SwitchNvosUpdateState::Completed,
            started_at: Some(requested_at - chrono::Duration::seconds(10)),
            ended_at: Some(requested_at - chrono::Duration::seconds(1)),
        }),
    )
    .await?;
    txn.commit().await?;

    env.run_switch_controller_iteration().await;

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForNVOSUpgrade,
        }
    ));
    assert!(switch.switch_reprovisioning_requested.is_some());

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_waiting_for_nvos_upgrade_transitions_to_error_on_failure(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = common::api_fixtures::site_explorer::new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_reprovisioning_requested(txn.as_mut(), switch_id, "rack-nvos-test")
        .await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    let requested_at = switch
        .switch_reprovisioning_requested
        .as_ref()
        .expect("switch reprovision request should exist")
        .requested_at;
    db_switch::try_update_controller_state(
        txn.as_mut(),
        switch_id,
        switch.controller_state.version,
        switch.controller_state.version.increment(),
        &SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForNVOSUpgrade,
        },
    )
    .await?;
    db_switch::update_nvos_update_status(
        txn.as_mut(),
        switch_id,
        Some(&model::switch::SwitchNvosUpdateStatus {
            task_id: "nvos-job".to_string(),
            firmware_id: "fw-1".to_string(),
            image_filename: "nvos-image.bin".to_string(),
            status: model::switch::SwitchNvosUpdateState::Failed {
                cause: "image install failed".to_string(),
            },
            started_at: Some(requested_at),
            ended_at: Some(chrono::Utc::now()),
        }),
    )
    .await?;
    txn.commit().await?;

    env.run_switch_controller_iteration().await;

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::Error { ref cause } if cause == "image install failed"
    ));
    assert!(switch.switch_reprovisioning_requested.is_none());

    Ok(())
}

#[crate::sqlx_test]
async fn test_switch_waiting_for_nmxc_configure_returns_ready_when_fm_is_running(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = common::api_fixtures::site_explorer::new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_reprovisioning_requested(txn.as_mut(), switch_id, "rack-nmxc-test")
        .await?;
    let switch = db_switch::find_by_id(txn.as_mut(), &switch_id)
        .await?
        .expect("switch should exist");
    db_switch::try_update_controller_state(
        txn.as_mut(),
        switch_id,
        switch.controller_state.version,
        switch.controller_state.version.increment(),
        &SwitchControllerState::ReProvisioning {
            reprovisioning_state: model::switch::ReProvisioningState::WaitingForNMXCConfigure,
        },
    )
    .await?;
    db_switch::update_fabric_manager_status(
        txn.as_mut(),
        switch_id,
        Some(&model::switch::FabricManagerStatus {
            fabric_manager_state: model::switch::FabricManagerState::Ok,
            addition_info: Some("CONTROL_PLANE_STATE_CONFIGURED".to_string()),
            reason: None,
            error_message: None,
        }),
    )
    .await?;
    txn.commit().await?;

    env.run_switch_controller_iteration().await;

    let mut txn = pool.acquire().await?;
    let switch = db_switch::find_by_id(&mut txn, &switch_id)
        .await?
        .expect("switch should exist");
    assert!(matches!(
        switch.controller_state.value,
        SwitchControllerState::Ready
    ));
    assert!(switch.switch_reprovisioning_requested.is_none());

    Ok(())
}

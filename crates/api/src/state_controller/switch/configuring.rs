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

//! Handler for SwitchControllerState::Configuring.

use carbide_uuid::switch::SwitchId;
use forge_secrets::credentials::{CredentialKey, Credentials};
use librms::protos::rack_manager as rms;
use model::switch::{ConfiguringState, Switch, SwitchControllerState, ValidatingState};

use crate::state_controller::state_handler::{
    StateHandlerContext, StateHandlerError, StateHandlerOutcome,
};
use crate::state_controller::switch::context::SwitchStateHandlerContextObjects;

const NVOS_ADMIN_USERNAME: &str = "admin";

/// Handles the Configuring state for a switch.
pub async fn handle_configuring(
    switch_id: &SwitchId,
    state: &mut Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
) -> Result<StateHandlerOutcome<SwitchControllerState>, StateHandlerError> {
    let config_state = match &state.controller_state.value {
        SwitchControllerState::Configuring { config_state } => config_state,
        _ => unreachable!("handle_configuring called with non-Configuring state"),
    };

    match config_state {
        ConfiguringState::RotateOsPassword => {
            handle_rotate_os_password(switch_id, state, ctx).await
        }
    }
}

async fn handle_rotate_os_password(
    switch_id: &SwitchId,
    state: &mut Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
) -> Result<StateHandlerOutcome<SwitchControllerState>, StateHandlerError> {
    let Some(bmc_mac_address) = state.bmc_mac_address else {
        return Ok(StateHandlerOutcome::transition(
            SwitchControllerState::Error {
                cause: "No BMC MAC address on switch".to_string(),
            },
        ));
    };

    let key = CredentialKey::SwitchNvosAdmin { bmc_mac_address };

    let mut txn = ctx.services.db_pool.begin().await?;
    let expected_switch =
        db::expected_switch::find_by_bmc_mac_address(&mut txn, bmc_mac_address).await?;
    let switch_endpoint =
        db::switch::find_switch_endpoints_by_ids(txn.as_mut(), std::slice::from_ref(switch_id))
            .await?
            .into_iter()
            .next();
    txn.commit().await?;

    let expected_switch = match expected_switch {
        Some(es) => es,
        None => {
            return Ok(error_transition(format!(
                "No expected switch found for BMC MAC {}",
                bmc_mac_address
            )));
        }
    };

    let target_password = match expected_switch.nvos_password.clone() {
        Some(password) if !password.is_empty() => password,
        Some(_) => {
            return Ok(error_transition(format!(
                "Switch {:?}: NVOS admin password is empty for BMC MAC {}",
                switch_id, bmc_mac_address
            )));
        }
        None => {
            tracing::info!(
                "Switch {:?}: no target NVOS admin password for BMC MAC {}, skipping",
                switch_id,
                bmc_mac_address
            );
            return Ok(validating_complete_transition());
        }
    };

    let current_credentials = ctx
        .services
        .credential_manager
        .get_credentials(&key)
        .await
        .map_err(|e| {
            StateHandlerError::GenericError(eyre::eyre!(
                "Switch {:?}: failed to read NVOS admin credentials from vault: {}",
                switch_id,
                e
            ))
        })?;

    let current_credentials = match current_credentials {
        Some(Credentials::UsernamePassword { username, password }) => {
            if username == NVOS_ADMIN_USERNAME && password == target_password {
                tracing::info!(
                    "Switch {:?}: target NVOS admin credentials already exist in vault for BMC MAC {}",
                    switch_id,
                    bmc_mac_address
                );
                return Ok(validating_complete_transition());
            }

            (username, password)
        }
        None => (
            expected_switch
                .nvos_username
                .clone()
                .unwrap_or_else(|| NVOS_ADMIN_USERNAME.to_string()),
            target_password.clone(),
        ),
    };

    if current_credentials.0.is_empty() || current_credentials.1.is_empty() {
        return Ok(error_transition(format!(
            "Switch {:?}: missing current NVOS credentials for BMC MAC {}",
            switch_id, bmc_mac_address
        )));
    }

    let rack_id = state
        .rack_id
        .clone()
        .or_else(|| expected_switch.rack_id.clone());

    let request = match build_update_switch_system_password_request(
        switch_id,
        rack_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        bmc_mac_address,
        expected_switch.bmc_ip_address,
        switch_endpoint.as_ref(),
        current_credentials,
        target_password.clone(),
    ) {
        Ok(request) => request,
        Err(cause) => return Ok(error_transition(cause)),
    };

    let Some(rms_client) = ctx.services.rms_client.as_ref() else {
        return Ok(error_transition("RMS client not configured"));
    };

    let response = match rms_client.update_switch_system_password(request).await {
        Ok(response) => response,
        Err(error) => {
            return Ok(error_transition(format!(
                "Switch {:?}: failed to update NVOS admin password through RMS: {}",
                switch_id, error
            )));
        }
    };

    if let Err(cause) = validate_update_switch_system_password_response(&response) {
        return Ok(error_transition(format!(
            "Switch {:?}: failed to update NVOS admin password through RMS: {}",
            switch_id, cause
        )));
    }

    let credentials = Credentials::UsernamePassword {
        username: NVOS_ADMIN_USERNAME.to_string(),
        password: target_password,
    };

    ctx.services
        .credential_manager
        .set_credentials(&key, &credentials)
        .await
        .map_err(|e| {
            StateHandlerError::GenericError(eyre::eyre!(
                "Switch {:?}: failed to store NVOS credentials in vault: {}",
                switch_id,
                e
            ))
        })?;

    tracing::info!(
        "Switch {:?}: rotated NVOS admin password through RMS and stored credentials in vault for BMC MAC {}",
        switch_id,
        bmc_mac_address
    );

    Ok(validating_complete_transition())
}

fn build_update_switch_system_password_request(
    switch_id: &SwitchId,
    rack_id: String,
    bmc_mac_address: mac_address::MacAddress,
    expected_bmc_ip: Option<std::net::IpAddr>,
    switch_endpoint: Option<&db::switch::SwitchEndpointRow>,
    current_credentials: (String, String),
    target_password: String,
) -> Result<rms::UpdateSwitchSystemPasswordRequest, String> {
    let bmc_ip = switch_endpoint
        .map(|endpoint| endpoint.bmc_ip)
        .or(expected_bmc_ip);
    let mut host_interfaces = Vec::new();

    if let Some(endpoint) = switch_endpoint
        && (endpoint.nvos_ip.is_some() || endpoint.nvos_mac.is_some())
    {
        host_interfaces.push(rms::NetworkInterface {
            ip_address: endpoint
                .nvos_ip
                .map(|ip_address| ip_address.to_string())
                .unwrap_or_default(),
            mac_address: endpoint
                .nvos_mac
                .map(|mac_address| mac_address.to_string())
                .unwrap_or_default(),
        });
    }

    if bmc_ip.is_none() && host_interfaces.is_empty() {
        return Err(format!(
            "no BMC or NVOS endpoint found for switch {}",
            switch_id
        ));
    }

    Ok(rms::UpdateSwitchSystemPasswordRequest {
        metadata: None,
        nodes: Some(rms::NodeSet {
            devices: vec![rms::NewNodeInfo {
                node_id: switch_id.to_string(),
                rack_id,
                r#type: Some(rms::NodeType::Switch as i32),
                bmc_endpoint: bmc_ip.map(|ip_address| rms::BmcEndpoint {
                    interface: Some(rms::NetworkInterface {
                        ip_address: ip_address.to_string(),
                        mac_address: bmc_mac_address.to_string(),
                    }),
                    port: 443,
                    credentials: None,
                }),
                host_endpoint: Some(rms::HostEndpoint {
                    interfaces: host_interfaces,
                    port: 0,
                    credentials: Some(rms::Credentials {
                        auth: Some(rms::credentials::Auth::UserPass(rms::UsernamePassword {
                            username: current_credentials.0,
                            password: current_credentials.1,
                        })),
                    }),
                }),
            }],
        }),
        username: NVOS_ADMIN_USERNAME.to_string(),
        password: target_password,
    })
}

fn validate_update_switch_system_password_response(
    response: &rms::UpdateSwitchSystemPasswordResponse,
) -> Result<(), String> {
    let batch_response = response
        .response
        .as_ref()
        .ok_or_else(|| "RMS response missing NodeBatchResponse".to_string())?;

    if let Some(failed_result) = batch_response.node_results.iter().find(|result| {
        result.status != rms::ReturnCode::Success as i32 || !result.error_message.is_empty()
    }) {
        let error_message = if failed_result.error_message.is_empty() {
            format!(
                "RMS reported password update failure for node {}",
                failed_result.node_id
            )
        } else {
            failed_result.error_message.clone()
        };
        return Err(error_message);
    }

    if batch_response.status != rms::ReturnCode::Success as i32 || batch_response.failed_nodes > 0 {
        let message = if batch_response.message.is_empty() {
            "RMS reported password update failure".to_string()
        } else {
            batch_response.message.clone()
        };
        return Err(message);
    }

    Ok(())
}

fn validating_complete_transition() -> StateHandlerOutcome<SwitchControllerState> {
    StateHandlerOutcome::transition(SwitchControllerState::Validating {
        validating_state: ValidatingState::ValidationComplete,
    })
}

fn error_transition(cause: impl Into<String>) -> StateHandlerOutcome<SwitchControllerState> {
    StateHandlerOutcome::transition(SwitchControllerState::Error {
        cause: cause.into(),
    })
}

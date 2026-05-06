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

use carbide_redfish::libredfish::RedfishClientPool;
use carbide_uuid::machine::MachineId;
use chrono::Utc;
use model::attestation::spdm::{
    SpdmAttestationState, SpdmDeviceAttestationDetails, SpdmHandlerError,
};
use model::machine::{
    AttestationMode, FailureCause, FailureDetails, FailureSource, MachineState, ManagedHostState,
    ManagedHostStateSnapshot, SpdmMeasuringState, StateMachineArea,
};
use rpc::forge::SpdmAttestationStatus;
use sqlx::PgPool;

use crate::handlers::attestation as attestation_handlers;
use crate::state_controller::machine::context::MachineStateHandlerContextObjects;
use crate::state_controller::state_handler::{
    StateHandlerContext, StateHandlerError, StateHandlerOutcome,
};

/// When SPDM attestation failed, check whether attestation was restarted (admin / status) or
/// disabled in config; if so, transition back to the appropriate measuring state based on
/// [`FailureDetails::source`].
pub(crate) async fn handle_spdm_attestation_failed_recovery(
    ctx: &mut StateHandlerContext<'_, MachineStateHandlerContextObjects>,
    host_machine_id: &MachineId,
    details: &FailureDetails,
) -> Result<StateHandlerOutcome<ManagedHostState>, StateHandlerError> {
    let mut txn = ctx.services.db_pool.begin().await?;
    let should_resume_attestation = if !ctx.services.site_config.spdm.enabled {
        true
    } else {
        let attestation_status =
            db::attestation::spdm::get_attestation_status_for_machine_id(&mut txn, host_machine_id)
                .await?;
        attestation_status == SpdmAttestationStatus::SpdmAttInProgress
            || attestation_status == SpdmAttestationStatus::SpdmAttCancelled
            || attestation_status == SpdmAttestationStatus::SpdmAttPassed
    };
    if should_resume_attestation {
        match &details.source {
            FailureSource::StateMachineArea(StateMachineArea::HostInit) => {
                Ok(StateHandlerOutcome::transition(ManagedHostState::HostInit {
                    machine_state: MachineState::SpdmMeasuring {
                        spdm_measuring_state: SpdmMeasuringState::PollResult,
                    },
                })
                .with_txn(txn))
            }
            FailureSource::StateMachineArea(StateMachineArea::AssignedInstance) => Ok(
                StateHandlerOutcome::transition(ManagedHostState::PostAssignedMeasuring {
                    attestation_mode: AttestationMode::SpdmAttestation {
                        spdm_measuring_state: SpdmMeasuringState::PollResult,
                    },
                })
                .with_txn(txn),
            ),
            FailureSource::StateMachineArea(StateMachineArea::MainFlow) => Ok(
                StateHandlerOutcome::transition(ManagedHostState::PreAssignedMeasuring {
                    spdm_measuring_state: SpdmMeasuringState::PollResult,
                })
                .with_txn(txn),
            ),
            _ => Ok(StateHandlerOutcome::do_nothing()),
        }
    } else {
        Ok(StateHandlerOutcome::do_nothing())
    }
}

pub(crate) async fn handle_spdm_trigger_state(
    db_pool: &PgPool,
    redfish_client_pool: Arc<dyn RedfishClientPool>,
    mh_snapshot: &mut ManagedHostStateSnapshot,
    host_machine_id: &MachineId,
    next_spdm_state: ManagedHostState,
    next_skip_state: ManagedHostState,
) -> Result<StateHandlerOutcome<ManagedHostState>, StateHandlerError> {
    // create redfish client
    let redfish_client = redfish_client_pool
        .create_client_from_machine(&mh_snapshot.host_snapshot, db_pool)
        .await?;

    let devices_scheduled = attestation_handlers::trigger_attestation(
        db_pool,
        redfish_client,
        &mh_snapshot.host_snapshot.bmc_info,
        host_machine_id,
        std::time::Duration::MAX,
    )
    .await
    .map_err(|e| SpdmHandlerError::TriggerMeasurementFail(e.to_string()))?;

    // if 0 devices scheduled - this means it is unsupported
    // so we just proceed to the next state
    if devices_scheduled == 0 {
        tracing::info!(
            machine_id = %host_machine_id,
            "No devices scheduled for SPDM attestation"
        );
        Ok(StateHandlerOutcome::transition(next_skip_state))
    } else {
        Ok(StateHandlerOutcome::transition(next_spdm_state))
    }
}

pub(crate) async fn handle_spdm_poll_state(
    db_pool: &PgPool,
    host_machine_id: &MachineId,
    failure_source: FailureSource,
    next_skip_state: ManagedHostState,
) -> Result<StateHandlerOutcome<ManagedHostState>, StateHandlerError> {
    let mut txn = db_pool.begin().await?;

    // get attestation status for the entire machine
    let attestation_status =
        db::attestation::spdm::get_attestation_status_for_machine_id(&mut txn, host_machine_id)
            .await?;

    // passed or cancelled -> just move to the next state
    // failed -> get states for all devices and log to the Failed state logging them there
    match attestation_status {
        SpdmAttestationStatus::SpdmAttPassed | SpdmAttestationStatus::SpdmAttCancelled => {
            Ok(StateHandlerOutcome::transition(next_skip_state).with_txn(txn))
        }
        SpdmAttestationStatus::SpdmAttFailed => {
            let attestation_states =
                db::attestation::spdm::get_attestations_for_machine_id(&mut txn, host_machine_id)
                    .await?;
            // here, move to failed state with a full details
            Ok(StateHandlerOutcome::transition(ManagedHostState::Failed {
                details: FailureDetails {
                    cause: FailureCause::SpdmAttestationFailed {
                        err: attestation_states
                            .iter()
                            .filter(|elem| matches!(elem.state, SpdmAttestationState::Failed(_)))
                            .fold(
                                String::new(),
                                |mut accum, x: &SpdmDeviceAttestationDetails| {
                                    accum.push_str(&x.get_failure_cause().unwrap_or_default());
                                    accum.push_str(". ");
                                    accum
                                },
                            ),
                    },
                    failed_at: Utc::now(),
                    source: failure_source,
                },
                retry_count: 0,
                machine_id: *host_machine_id,
            })
            .with_txn(txn))
        }
        SpdmAttestationStatus::SpdmAttInProgress => Ok(StateHandlerOutcome::do_nothing()),
    }
}

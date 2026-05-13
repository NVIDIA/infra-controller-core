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

//! State Handler implementation for PowerShelves (mirrors Switch state handler structure).

use carbide_uuid::power_shelf::PowerShelfId;
use model::power_shelf::{
    PowerShelf, PowerShelfControllerState, derive_power_shelf_aggregate_health,
};
use tracing::instrument;

use crate::state_controller::power_shelf::configuring::handle_configuring;
use crate::state_controller::power_shelf::context::PowerShelfStateHandlerContextObjects;
use crate::state_controller::power_shelf::deleting::handle_deleting;
use crate::state_controller::power_shelf::error_state::handle_error;
use crate::state_controller::power_shelf::fetching_data::handle_fetching_data;
use crate::state_controller::power_shelf::initializing::handle_initializing;
use crate::state_controller::power_shelf::maintenance::handle_maintenance;
use crate::state_controller::power_shelf::ready::handle_ready;
use crate::state_controller::state_handler::{
    StateHandler, StateHandlerContext, StateHandlerError, StateHandlerOutcome,
};

/// The actual PowerShelf State handler (structure mirrors SwitchStateHandler).
#[derive(Debug, Default, Clone)]
pub struct PowerShelfStateHandler {}

impl PowerShelfStateHandler {
    fn record_metrics(
        &self,
        state: &PowerShelf,
        ctx: &mut StateHandlerContext<'_, PowerShelfStateHandlerContextObjects>,
    ) {
        let aggregate_health = derive_power_shelf_aggregate_health(&state.health_reports);
        ctx.metrics.health.populate(
            state.id.to_string(),
            &aggregate_health,
            &state.health_reports,
        );
    }

    /// Attempts a state transition by delegating to the appropriate state handler.
    async fn attempt_state_transition(
        &self,
        power_shelf_id: &PowerShelfId,
        state: &mut PowerShelf,
        ctx: &mut StateHandlerContext<'_, PowerShelfStateHandlerContextObjects>,
    ) -> Result<StateHandlerOutcome<PowerShelfControllerState>, StateHandlerError> {
        let controller_state = &state.controller_state.value;

        match controller_state {
            PowerShelfControllerState::Initializing => {
                handle_initializing(power_shelf_id, state, ctx).await
            }
            PowerShelfControllerState::FetchingData => {
                handle_fetching_data(power_shelf_id, state, ctx).await
            }
            PowerShelfControllerState::Configuring => {
                handle_configuring(power_shelf_id, state, ctx).await
            }
            PowerShelfControllerState::Ready => handle_ready(power_shelf_id, state, ctx).await,
            PowerShelfControllerState::Maintenance { .. } => {
                handle_maintenance(power_shelf_id, state, ctx).await
            }
            PowerShelfControllerState::Deleting => {
                handle_deleting(power_shelf_id, state, ctx).await
            }
            PowerShelfControllerState::Error { .. } => {
                handle_error(power_shelf_id, state, ctx).await
            }
        }
    }
}

#[async_trait::async_trait]
impl StateHandler for PowerShelfStateHandler {
    type ObjectId = PowerShelfId;
    type State = PowerShelf;
    type ControllerState = PowerShelfControllerState;
    type ContextObjects = PowerShelfStateHandlerContextObjects;

    #[instrument(skip_all, fields(object_id=%power_shelf_id))]
    async fn handle_object_state(
        &self,
        power_shelf_id: &PowerShelfId,
        state: &mut PowerShelf,
        _controller_state: &PowerShelfControllerState,
        ctx: &mut StateHandlerContext<Self::ContextObjects>,
    ) -> Result<StateHandlerOutcome<PowerShelfControllerState>, StateHandlerError> {
        self.record_metrics(state, ctx);
        self.attempt_state_transition(power_shelf_id, state, ctx)
            .await
    }
}

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

//! Handler for PowerShelfControllerState::Error.

use carbide_uuid::power_shelf::PowerShelfId;
use model::power_shelf::{PowerShelf, PowerShelfControllerState};

use crate::state_controller::power_shelf::context::PowerShelfStateHandlerContextObjects;
use crate::state_controller::state_handler::{
    StateHandlerContext, StateHandlerError, StateHandlerOutcome,
};

/// Handles the Error state for a power shelf.
///
/// If marked for deletion, transition to `Deleting`; otherwise hold the
/// shelf in `Error` for manual intervention.
pub async fn handle_error(
    power_shelf_id: &PowerShelfId,
    state: &mut PowerShelf,
    _ctx: &mut StateHandlerContext<'_, PowerShelfStateHandlerContextObjects>,
) -> Result<StateHandlerOutcome<PowerShelfControllerState>, StateHandlerError> {
    tracing::info!("PowerShelf {} is in error state", power_shelf_id);
    if state.is_marked_as_deleted() {
        Ok(StateHandlerOutcome::transition(
            PowerShelfControllerState::Deleting,
        ))
    } else {
        Ok(StateHandlerOutcome::do_nothing())
    }
}

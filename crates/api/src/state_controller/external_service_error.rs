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

use carbide_dpf::DpfError;
use carbide_redfish::libredfish::RedfishClientCreationError;
use libredfish::RedfishError;
use librms::RackManagerError;

use crate::state_controller::state_handler::{ExternalServiceError, StateHandlerError};

// Keep concrete service client errors at the carbide-api boundary. The
// state-controller crate deliberately depends only on the opaque
// ExternalServiceError type.

pub(crate) fn redfish_client_creation_error(
    error: RedfishClientCreationError,
) -> StateHandlerError {
    ExternalServiceError::with_source(
        "redfish",
        "create_client",
        error.to_string(),
        "redfish_client_creation_error",
        error,
    )
    .into()
}

pub(crate) fn redfish_error(operation: &'static str, error: RedfishError) -> StateHandlerError {
    ExternalServiceError::with_source(
        "redfish",
        operation,
        error.to_string(),
        redfish_operation_metric_label(operation),
        error,
    )
    .into()
}

pub(crate) fn rack_manager_error(
    operation: &'static str,
    error: RackManagerError,
) -> StateHandlerError {
    ExternalServiceError::with_source(
        "rack_manager",
        operation,
        error.to_string(),
        "rack_manager_error",
        error,
    )
    .into()
}

pub(crate) fn dpf_error(error: DpfError) -> StateHandlerError {
    ExternalServiceError::with_source("dpf", "", error.to_string(), "dpf_error", error).into()
}

pub(crate) fn ufm_error(operation: &'static str, error: eyre::Report) -> StateHandlerError {
    ExternalServiceError::new("ufm", operation, error.to_string(), "ib_fabric_error").into()
}

fn redfish_operation_metric_label(operation: &'static str) -> &'static str {
    match operation {
        "restart" => "redfish_restart_error",
        "lockdown" => "redfish_lockdown_error",
        _ => "redfish_other_error",
    }
}

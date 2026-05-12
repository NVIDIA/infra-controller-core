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

// This is temporary module that will be moved to rpc crate once all
// rpc-related code will be isolated here.

pub mod allocation_type;
pub mod attestation;
pub mod bmc_info;
pub mod compute_allocation;
pub mod controller_outcome;
pub mod dhcp_record;
pub mod dns;
pub mod dpa_interface;
pub mod dpu_remediation;
pub mod expected_machine;
pub mod expected_power_shelf;
pub mod expected_rack;
pub mod expected_switch;
pub mod extension_service;
pub mod hardware_info;
pub mod health;
pub mod ib_partition;
pub mod instance;
pub mod instance_type;
pub mod operating_system_definition;
pub mod os;
pub mod power_manager;
pub mod power_shelf;
pub mod pxe;
pub mod rack;
pub mod rack_firmware;
pub mod rack_type;
pub mod redfish;
pub mod resource_pool;
pub mod route_server;
pub mod site_explorer;
pub mod sku;
pub mod state_history;
pub mod storage;
pub mod switch;
pub mod tenant;
pub mod trim_table;
pub mod vpc;
pub mod vpc_prefix;

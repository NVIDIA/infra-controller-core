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

use crate::machine::upgrade_policy::AgentUpgradePolicy;

// To the RPC
impl From<AgentUpgradePolicy> for i32 {
    fn from(p: AgentUpgradePolicy) -> Self {
        use AgentUpgradePolicy::*;
        match p {
            Off => rpc::forge::AgentUpgradePolicy::Off as i32,
            UpOnly => rpc::forge::AgentUpgradePolicy::UpOnly as i32,
            UpDown => rpc::forge::AgentUpgradePolicy::UpDown as i32,
        }
    }
}

// From the RPC
impl From<i32> for AgentUpgradePolicy {
    fn from(rpc_policy: i32) -> Self {
        use rpc::forge::AgentUpgradePolicy::*;
        match rpc_policy {
            n if n == Off as i32 => AgentUpgradePolicy::Off,
            n if n == UpOnly as i32 => AgentUpgradePolicy::UpOnly,
            n if n == UpDown as i32 => AgentUpgradePolicy::UpDown,
            _ => {
                unreachable!();
            }
        }
    }
}

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

mod api;
mod container;

use std::path::Path;

pub use api::ApiApplier;
use async_trait::async_trait;
pub use container::ContainerApplier;

/// Trait for applying NVUE configuration to HBN.
#[async_trait]
pub trait NvueApplier: Send + Sync {
    /// Apply the given NVUE YAML config. Returns `Ok(true)` if the config
    /// changed and was applied, `Ok(false)` if it was unchanged.
    async fn apply(&self, yaml_content: String) -> eyre::Result<bool>;

    /// The root directory of the HBN container filesystem.
    fn hbn_root(&self) -> &Path;

    /// Whether post-write commands (container exec, etc.) should be skipped.
    /// Used in dev/test mode via `agent_config.hbn.skip_reload`.
    fn skip_reload(&self) -> bool;

    /// Hint that the next `apply()` should force-write even if the config
    /// appears unchanged or oversized. Used when switching to the admin network
    /// to ensure a bloated tenant config doesn't block the transition.
    /// Default is a no-op; only `ContainerApplier` acts on this.
    fn set_force_next_write(&self, _force: bool) {}
}

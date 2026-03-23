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

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use eyre::WrapErr;

use super::NvueApplier;
use crate::ethernet_virtualization::FPath;

/// Apply config by writing it to the HBN container's filesystem and exec'ing
/// `nv config replace` + `nv config apply` inside the container.
pub struct ContainerApplier {
    hbn_root: PathBuf,
    skip_reload: bool,
    /// When true, force the next write even if it exceeds MAX_EXPECTED_SIZE.
    /// Set via the trait's `set_force_next_write` when switching to admin network.
    /// AtomicBool instead of Cell<bool> because the NvueApplier trait requires Sync
    /// (imposed by async_trait + &self). In practice this is only accessed
    /// sequentially from the main loop.
    force_next_write: AtomicBool,
}

impl ContainerApplier {
    pub fn new(hbn_root: PathBuf, skip_reload: bool) -> Self {
        Self {
            hbn_root,
            skip_reload,
            force_next_write: AtomicBool::new(false),
        }
    }

    /// Exec `nv config replace` + `nv config apply` in the HBN container,
    /// with backup/error file recovery on failure.
    async fn exec_apply(&self, config_path: &FPath) -> eyre::Result<()> {
        match crate::nvue::run_apply(&self.hbn_root, &config_path.0).await {
            Ok(()) => {
                config_path.del("BAK");
                Ok(())
            }
            Err(err) => {
                tracing::error!("update_nvue post command failed: {err:#}");

                // Move the failed config to .error for inspection.
                let path_error = config_path.with_ext("error");
                if path_error.exists()
                    && let Err(e) = std::fs::remove_file(path_error.clone())
                {
                    tracing::warn!(
                        "Failed to remove previous error file ({}): {e}",
                        path_error.display()
                    );
                }

                if let Err(rename_err) = std::fs::rename(config_path, &path_error) {
                    eyre::bail!(
                        "rename {config_path} to {} on error: {rename_err:#}",
                        path_error.display()
                    );
                }
                // Restore the backup so the next run will retry with the previous config.
                let path_bak = config_path.backup();
                if path_bak.exists()
                    && let Err(rename_err) = std::fs::rename(&path_bak, config_path)
                {
                    eyre::bail!(
                        "rename {} to {config_path}, reverting on error: {rename_err:#}",
                        path_bak.display(),
                    );
                }

                Err(err)
            }
        }
    }
}

#[async_trait]
impl NvueApplier for ContainerApplier {
    fn hbn_root(&self) -> &std::path::Path {
        &self.hbn_root
    }

    async fn apply(&self, yaml_content: String) -> eyre::Result<bool> {
        // nvue can save a copy of the config here. If that exists nvue uses it on boot.
        // We always want to use the most recent `nv config apply`, so ensure this doesn't exist.
        let saved_config = self.hbn_root.join(crate::nvue::SAVE_PATH);
        if saved_config.exists()
            && let Err(err) = std::fs::remove_file(&saved_config)
        {
            tracing::warn!(
                "Failed removing old startup.yaml at {}: {err:#}",
                saved_config.display()
            );
        }

        let path = FPath(self.hbn_root.join(crate::nvue::PATH));
        path.cleanup();

        let force = self.force_next_write.swap(false, Ordering::Relaxed);
        if !crate::ethernet_virtualization::write(yaml_content, &path, "NVUE", force)
            .wrap_err(format!("NVUE config at {path}"))?
        {
            return Ok(false);
        }

        if !self.skip_reload {
            self.exec_apply(&path).await?;
        }
        Ok(true)
    }

    fn skip_reload(&self) -> bool {
        self.skip_reload
    }

    fn set_force_next_write(&self, force: bool) {
        self.force_next_write.store(force, Ordering::Relaxed);
    }
}

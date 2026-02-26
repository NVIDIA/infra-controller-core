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

use std::future::Future;
use std::pin::Pin;

use tokio::sync::{Mutex, OnceCell};

/// SummaryFormat allows the caller to configure how they
/// want a summary reported at the end of of the run.
#[derive(Debug, Clone)]
pub enum SummaryFormat {
    PlainText,
    Json,
    Yaml,
}

/// FileEnsure specifies whether a file should be present
/// or absent on the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEnsure {
    /// Present indicates the file should exist with the
    /// specified content and attributes.
    Present,
    /// Absent indicates the file should not exist and
    /// should be deleted if present.
    Absent,
}

/// SyncOptions allows the caller to control various
/// aspects of the duppet sync.
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// dry_run allows the caller to perform a dry run
    /// on the sync -- no files will be created or updated,
    /// and it will simply log and report what would have
    /// been done.
    pub dry_run: bool,
    /// quiet will make it so duppet doesn't log individual
    /// file updates, and will leave it until the end when
    /// a summary is printed.
    pub quiet: bool,
    /// no_color will exclude the beautiful colors that
    /// are included in messages, if that's what you really
    /// want.
    pub no_color: bool,
    /// summary_format is the format of the report summary
    /// at the end of the run (plaintext, json, yaml).
    pub summary_format: SummaryFormat,
}

type GeneratedContent = Pin<Box<dyn Future<Output = String> + Send + 'static>>;
type ContentGenerator = Box<dyn FnOnce() -> GeneratedContent + Send + Sync>;

pub struct FileContent {
    value: OnceCell<String>,
    generator: Mutex<Option<ContentGenerator>>,
}

#[derive(Debug)]
pub enum FileContentError {
    NotReady,
}

impl std::fmt::Display for FileContentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileContentError::NotReady => write!(f, "FileContent content not ready"),
        }
    }
}

impl std::error::Error for FileContentError {}

impl From<FileContentError> for std::io::Error {
    fn from(err: FileContentError) -> std::io::Error {
        match err {
            FileContentError::NotReady => std::io::Error::new(std::io::ErrorKind::WouldBlock, err),
        }
    }
}

impl FileContent {
    pub fn immediate(val: String) -> Self {
        let cell = OnceCell::new();
        cell.set(val).ok();
        FileContent {
            value: cell,
            generator: Mutex::new(None),
        }
    }

    pub fn deferred(f: ContentGenerator) -> Self {
        FileContent {
            value: OnceCell::new(),
            generator: Mutex::new(Some(f)),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.value.get().is_some()
    }

    pub async fn get_async(&self) -> &str {
        if let Some(v) = self.value.get() {
            return v.as_str();
        }
        let mut guard = self.generator.lock().await;
        if let Some(f) = guard.take() {
            let generated = (f)().await;
            let _ = self.value.set(generated);
            return self.value.get().unwrap().as_str();
        }
        unreachable!(
            "Use FileContent::immediate() or FileContent::deferred() to create a valid instance."
        );
    }

    /// Returns the immediate or cached content as a reference, or None if not ready.
    pub fn get_if_ready(&self) -> Option<&str> {
        self.value.get().map(|s| s.as_str())
    }

    /// Returns the immediate or cached content as a reference, or throws an error if not ready.
    pub fn get(&self) -> Result<&str, FileContentError> {
        self.get_if_ready().ok_or(FileContentError::NotReady)
    }
}

impl std::fmt::Debug for FileContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(v) = self.value.get() {
            let display_len = 100;
            let preview = if v.len() > display_len {
                format!("{}...", &v[..display_len])
            } else {
                v.clone()
            };
            f.debug_struct("content")
                .field("len", &v.len())
                .field("preview", &preview)
                .finish()
        } else {
            write!(f, "<content missing or not yet generated>")
        }
    }
}

impl From<String> for FileContent {
    fn from(s: String) -> Self {
        FileContent::immediate(s)
    }
}

impl From<&str> for FileContent {
    fn from(s: &str) -> Self {
        FileContent::immediate(s.to_string())
    }
}

impl<F, Fut> From<F> for FileContent
where
    F: FnOnce() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = String> + Send + 'static,
{
    fn from(f: F) -> Self {
        let generator: ContentGenerator = Box::new(move || Box::pin(f()));
        FileContent::deferred(generator)
    }
}

/// FileSpec defines a file specification for the
/// desired state of the file being created, including
/// the content, the permissions, the owner, and the
/// group.
#[derive(Debug)]
pub struct FileSpec {
    /// content is the actual file content to set, either an
    /// immediate value or a closure to generate a value.
    pub content: Option<FileContent>,
    /// permissions are the optional permissions to
    /// set on the file. If None, no permission management
    /// will happen, and system defaults will be used, and
    /// no attempts to keep permissions in sync will occur.
    pub permissions: Option<u32>,
    /// owner is an optional owner to set for the file. If
    /// None, then no owner management will happen, and
    /// the system default will be used, and no attempts
    /// to keep the owner in sync will occur.
    pub owner: Option<String>,
    /// group is an optional group to set for the file. If None,
    /// then no group management will happen, and the system
    /// default will be used, and no attempts to keep the group
    /// in sync will occur.
    pub group: Option<String>,
    /// ensure specifies whether the file should be present
    /// or absent on the filesystem.
    pub ensure: FileEnsure,
    /// exec_on_change triggers the execution of a file after
    /// it has been created.
    pub exec_on_change: bool,
}

impl FileSpec {
    /// new creates a new FileSpec with default values: empty content,
    /// permissions set to 0o644, no owner/group management, and
    /// ensure set to Present.
    pub fn new() -> Self {
        FileSpec {
            content: None,
            permissions: Some(0o644),
            owner: None,
            group: None,
            ensure: FileEnsure::Present,
            exec_on_change: false,
        }
    }

    /// with_exec_on_change is a builder method that sets the exec_on_change
    /// flag on the FileSpec, which will trigger the execution of the file
    /// on create or update.
    pub fn with_exec_on_change(mut self) -> Self {
        self.exec_on_change = true;
        self
    }

    /// with_content is a builder method that sets the content
    /// field on an existing FileSpec.
    pub fn with_content(mut self, content: impl Into<FileContent>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// with_perms is a builder method that sets the permissions
    /// field on an existing FileSpec.
    pub fn with_perms(mut self, permissions: u32) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// with_ownership is a builder method that sets the owner
    /// and group fields on an existing FileSpec.
    pub fn with_ownership(mut self, owner: Option<String>, group: Option<String>) -> Self {
        self.owner = owner;
        self.group = group;
        self
    }

    /// with_ensure is a builder method that sets the ensure
    /// field on an existing FileSpec.
    pub fn with_ensure(mut self, ensure: FileEnsure) -> Self {
        self.ensure = ensure;
        self
    }

    pub async fn get_content_async(&self) -> &str {
        match &self.content {
            Some(content) => content.get_async().await,
            None => "",
        }
    }

    /// Returns the immediate or cached content as a reference, or None if not ready.
    pub fn get_content_if_ready(&self) -> Option<&str> {
        self.content
            .as_ref()
            .map_or(Some(""), |content| content.get_if_ready())
    }

    /// Returns the immediate or cached content as a reference, or throws an error if not ready.
    pub fn get_content(&self) -> Result<&str, FileContentError> {
        self.content
            .as_ref()
            .map_or(Ok(""), |content| content.get())
    }
}

impl Default for FileSpec {
    fn default() -> Self {
        Self::new()
    }
}

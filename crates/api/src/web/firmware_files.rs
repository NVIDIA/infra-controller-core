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

use axum::extract::{Path as AxumPath, State as AxumState};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{Router, get};
use http::StatusCode;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use tokio_util::io::ReaderStream;

use crate::api::Api;
use crate::auth::{AuthContext, Principal};

pub fn router(api: Arc<Api>) -> Router {
    Router::new()
        .route("/{*path}", get(serve_firmware_file))
        .layer(axum::middleware::from_fn(require_scout))
        .with_state(api)
}

async fn require_scout(req: http::Request<axum::body::Body>, next: Next) -> Response {
    // technically this can also be any other machine (e.g. dpu) but the name makes the intent clearer
    let is_scout = req.extensions().get::<AuthContext>().is_some_and(|ctx| {
        ctx.principals
            .iter()
            .any(|p| matches!(p, Principal::SpiffeMachineIdentifier(_)))
    });

    if !is_scout {
        return StatusCode::FORBIDDEN.into_response();
    }

    next.run(req).await
}

async fn serve_firmware_file(
    state: AxumState<Arc<Api>>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    let firmware_directory = &state.runtime_config.firmware_global.firmware_directory;
    let requested = firmware_directory.join(&path);

    let canonical = match tokio::fs::canonicalize(&requested).await {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (StatusCode::NOT_FOUND, "file not found").into_response();
        }
        Err(err) => {
            tracing::error!(%err, %path, "serve_firmware_file canonicalize");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if !canonical.starts_with(firmware_directory) {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }

    let file = match tokio::fs::File::open(&canonical).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (StatusCode::NOT_FOUND, "file not found").into_response();
        }
        Err(err) => {
            tracing::error!(%err, %path, "serve_firmware_file open");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let metadata = match file.metadata().await {
        Ok(m) => m,
        Err(err) => {
            tracing::error!(%err, %path, "serve_firmware_file metadata");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let stream = ReaderStream::with_capacity(file, 64 * 1024);
    let body = axum::body::Body::from_stream(stream);

    (
        [
            (CONTENT_TYPE, "application/octet-stream"),
            (CONTENT_LENGTH, &metadata.len().to_string()),
        ],
        body,
    )
        .into_response()
}

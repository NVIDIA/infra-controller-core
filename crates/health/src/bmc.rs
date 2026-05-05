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
use std::sync::atomic::{AtomicU64, Ordering};

use futures::TryStreamExt;
use nv_redfish::bmc_http::HttpBmc;
use nv_redfish::bmc_http::reqwest::{BmcError, Client as ReqwestClient};
use nv_redfish::core::query::{ExpandQuery, FilterQuery};
use nv_redfish::core::{
    Action, Bmc, BoxTryStream, EntityTypeRef, Expandable, ModificationResponse, ODataETag, ODataId,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::HealthError;
use crate::endpoint::BmcEndpoint;

/// BMC client wrapper that refreshes endpoint credentials after auth failures.
pub struct AuthRefreshingBmc {
    inner: HttpBmc<ReqwestClient>,
    endpoint: Arc<BmcEndpoint>,
    credential_generation: AtomicU64,
    refresh_lock: Mutex<()>,
}

impl AuthRefreshingBmc {
    pub(crate) fn new(inner: HttpBmc<ReqwestClient>, endpoint: Arc<BmcEndpoint>) -> Self {
        Self {
            inner,
            endpoint,
            credential_generation: AtomicU64::new(0),
            refresh_lock: Mutex::new(()),
        }
    }

    async fn refresh_credentials(
        &self,
        error: &HealthError,
        observed_generation: Option<u64>,
    ) -> Result<(), HealthError> {
        let _guard = self.refresh_lock.lock().await;
        if observed_generation.is_some_and(|generation| {
            generation != self.credential_generation.load(Ordering::Acquire)
        }) {
            return Ok(());
        }

        tracing::warn!(
            error = ?error,
            endpoint = ?self.endpoint.addr,
            "Authentication failed, refreshing BMC credentials"
        );

        let credentials = self.endpoint.refresh().await.map_err(|refresh_error| {
            HealthError::GenericError(format!(
                "Failed to refresh credentials after auth error {error}: {refresh_error}"
            ))
        })?;
        self.inner.set_credentials(credentials.into());
        self.credential_generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    async fn refresh_auth_if_needed(
        &self,
        error: HealthError,
        observed_generation: u64,
    ) -> HealthError {
        if is_auth_error(&error)
            && let Err(refresh_error) = self
                .refresh_credentials(&error, Some(observed_generation))
                .await
        {
            tracing::error!(
                error = ?refresh_error,
                original_error = ?error,
                endpoint = ?self.endpoint.addr,
                "Failed to refresh BMC credentials after authentication error"
            );
        }

        error
    }
}

impl Bmc for AuthRefreshingBmc {
    type Error = HealthError;

    async fn expand<T: Expandable>(
        &self,
        id: &ODataId,
        query: ExpandQuery,
    ) -> Result<Arc<T>, Self::Error> {
        let credential_generation = self.credential_generation.load(Ordering::Acquire);
        match self
            .inner
            .expand(id, query)
            .await
            .map_err(HealthError::from)
        {
            Ok(value) => Ok(value),
            Err(error) => Err(self
                .refresh_auth_if_needed(error, credential_generation)
                .await),
        }
    }

    async fn get<T: EntityTypeRef + for<'de> Deserialize<'de> + 'static>(
        &self,
        id: &ODataId,
    ) -> Result<Arc<T>, Self::Error> {
        let credential_generation = self.credential_generation.load(Ordering::Acquire);
        match self.inner.get(id).await.map_err(HealthError::from) {
            Ok(value) => Ok(value),
            Err(error) => Err(self
                .refresh_auth_if_needed(error, credential_generation)
                .await),
        }
    }

    async fn filter<T: EntityTypeRef + for<'de> Deserialize<'de> + 'static>(
        &self,
        id: &ODataId,
        query: FilterQuery,
    ) -> Result<Arc<T>, Self::Error> {
        let credential_generation = self.credential_generation.load(Ordering::Acquire);
        match self
            .inner
            .filter(id, query)
            .await
            .map_err(HealthError::from)
        {
            Ok(value) => Ok(value),
            Err(error) => Err(self
                .refresh_auth_if_needed(error, credential_generation)
                .await),
        }
    }

    async fn create<V: Send + Sync + Serialize, R: Send + Sync + for<'de> Deserialize<'de>>(
        &self,
        id: &ODataId,
        query: &V,
    ) -> Result<ModificationResponse<R>, Self::Error> {
        self.inner
            .create(id, query)
            .await
            .map_err(HealthError::from)
    }

    async fn update<
        V: Sync + Send + Serialize,
        R: Send + Sync + Sized + for<'de> Deserialize<'de>,
    >(
        &self,
        id: &ODataId,
        etag: Option<&ODataETag>,
        update: &V,
    ) -> Result<ModificationResponse<R>, Self::Error> {
        self.inner
            .update(id, etag, update)
            .await
            .map_err(HealthError::from)
    }

    async fn delete<R: EntityTypeRef + for<'de> Deserialize<'de>>(
        &self,
        id: &ODataId,
    ) -> Result<ModificationResponse<R>, Self::Error> {
        self.inner.delete(id).await.map_err(HealthError::from)
    }

    async fn action<
        T: Send + Sync + Serialize,
        R: Send + Sync + Sized + for<'de> Deserialize<'de>,
    >(
        &self,
        action: &Action<T, R>,
        params: &T,
    ) -> Result<ModificationResponse<R>, Self::Error> {
        self.inner
            .action(action, params)
            .await
            .map_err(HealthError::from)
    }

    async fn stream<T: Sized + for<'de> Deserialize<'de> + Send + 'static>(
        &self,
        uri: &str,
    ) -> Result<BoxTryStream<T, Self::Error>, Self::Error> {
        let credential_generation = self.credential_generation.load(Ordering::Acquire);
        match self.inner.stream(uri).await.map_err(HealthError::from) {
            Ok(stream) => Ok(Box::pin(stream.map_err(HealthError::from))),
            Err(error) => Err(self
                .refresh_auth_if_needed(error, credential_generation)
                .await),
        }
    }
}

pub(crate) fn is_auth_error(error: &HealthError) -> bool {
    match error {
        HealthError::HttpError(message) => {
            message.contains("HTTP 401") || message.contains("HTTP 403")
        }
        HealthError::BmcError(inner) => is_auth_bmc_source_error(inner.as_ref()),
        _ => false,
    }
}

pub(crate) fn is_auth_bmc_source_error(error: &(dyn std::error::Error + 'static)) -> bool {
    error
        .downcast_ref::<BmcError>()
        .is_some_and(is_auth_bmc_error)
        || error
            .downcast_ref::<HealthError>()
            .is_some_and(is_auth_error)
}

fn is_auth_bmc_error(error: &BmcError) -> bool {
    matches!(
        error,
        BmcError::InvalidResponse { status, .. }
            if *status == http::StatusCode::UNAUTHORIZED || *status == http::StatusCode::FORBIDDEN
    )
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    fn bmc_status_error(status: http::StatusCode) -> BmcError {
        BmcError::InvalidResponse {
            url: Url::parse("https://127.0.0.1/redfish/v1").expect("valid url"),
            status,
            text: String::new(),
        }
    }

    #[test]
    fn detects_auth_bmc_errors() {
        assert!(is_auth_bmc_error(&bmc_status_error(
            http::StatusCode::UNAUTHORIZED
        )));
        assert!(is_auth_bmc_error(&bmc_status_error(
            http::StatusCode::FORBIDDEN
        )));
        assert!(!is_auth_bmc_error(&bmc_status_error(
            http::StatusCode::NOT_FOUND
        )));
    }

    #[test]
    fn detects_auth_health_errors() {
        assert!(is_auth_error(&HealthError::BmcError(Box::new(
            bmc_status_error(http::StatusCode::UNAUTHORIZED),
        ))));
        assert!(is_auth_error(&HealthError::HttpError(
            "request failed with HTTP 403".to_string(),
        )));
        assert!(!is_auth_error(&HealthError::HttpError(
            "request failed with HTTP 404".to_string(),
        )));
    }
}

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

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::WWW_AUTHENTICATE;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum_extra::TypedHeader;
use axum_extra::headers::Authorization;
use axum_extra::headers::authorization::Basic;
use tracing::instrument;

use crate::http::call_router_with_new_request;
use crate::redfish::account_service::AccountServiceState;
use crate::redfish::{account_service, service_root};

const WWW_AUTHENTICATE_VALUE: HeaderValue = HeaderValue::from_static("Basic realm=\"bmc-mock\"");

pub fn append(router: Router, authorizer: Authorizer) -> Router {
    let service_root_path = service_root::resource().odata_id.to_string();
    let service_root_path_with_trailing_slash = format!("{service_root_path}/");
    let account_service_path = account_service::resource().odata_id.to_string();
    let account_service_subtree_path = format!("{account_service_path}/{{*all}}");

    Router::new()
        .route(&service_root_path, any(process_without_auth))
        .route(
            &service_root_path_with_trailing_slash,
            any(process_without_auth),
        )
        .route(&account_service_path, any(process_account_service))
        .route(&account_service_subtree_path, any(process_account_service))
        .route("/{*all}", any(process))
        .with_state(AuthMiddleware {
            inner: router,
            authorizer,
        })
}

#[instrument(skip_all)]
async fn process_without_auth(
    State(mut state): State<AuthMiddleware>,
    request: Request<Body>,
) -> Response {
    state.call_inner_router(request).await
}

#[instrument(skip_all)]
async fn process_account_service(
    State(mut state): State<AuthMiddleware>,
    authorization: Option<TypedHeader<Authorization<Basic>>>,
    request: Request<Body>,
) -> Response {
    match state.authorizer.authorize(authorization.as_ref(), true) {
        AuthorizationResult::Authorized => state.call_inner_router(request).await,
        AuthorizationResult::Unauthorized => {
            tracing::warn!(
                method = request.method().as_str(),
                path = request.uri().path(),
                "Unauthorized request",
            );
            unauthorized_response()
        }
        AuthorizationResult::FactoryDefaultPasswordForbidden => {
            unreachable!("account service authorization does not forbid factory-default passwords")
        }
    }
}

#[instrument(skip_all)]
async fn process(
    State(mut state): State<AuthMiddleware>,
    authorization: Option<TypedHeader<Authorization<Basic>>>,
    request: Request<Body>,
) -> Response {
    match state.authorizer.authorize(authorization.as_ref(), false) {
        AuthorizationResult::Authorized => state.call_inner_router(request).await,
        AuthorizationResult::Unauthorized => {
            tracing::warn!(
                method = request.method().as_str(),
                path = request.uri().path(),
                "Unauthorized request",
            );
            unauthorized_response()
        }
        AuthorizationResult::FactoryDefaultPasswordForbidden => {
            tracing::warn!(
                method = request.method().as_str(),
                path = request.uri().path(),
                "Factory-default password must be changed before accessing this resource",
            );
            StatusCode::FORBIDDEN.into_response()
        }
    }
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(WWW_AUTHENTICATE, WWW_AUTHENTICATE_VALUE)],
    )
        .into_response()
}

#[derive(Clone)]
struct AuthMiddleware {
    inner: Router,
    authorizer: Authorizer,
}

impl AuthMiddleware {
    async fn call_inner_router(&mut self, request: Request<Body>) -> Response {
        call_router_with_new_request(&mut self.inner, request).await
    }
}

#[derive(Clone)]
pub struct Authorizer {
    account_service_state: Arc<AccountServiceState>,
}

impl Authorizer {
    /// Builds the factory-default authorizer for a mock BMC state.
    pub fn new(account_service_state: Arc<AccountServiceState>) -> Self {
        Self {
            account_service_state,
        }
    }

    fn authorize(
        &self,
        authorization: Option<&TypedHeader<Authorization<Basic>>>,
        permit_factory_default: bool,
    ) -> AuthorizationResult {
        let Some(authorization) = authorization else {
            return AuthorizationResult::Unauthorized;
        };
        let actual = &authorization.0.0;
        if self
            .account_service_state
            .is_authorized(actual.username(), actual.password())
        {
            if !permit_factory_default
                && self
                    .account_service_state
                    .is_factory_default_password(actual.username(), actual.password())
            {
                AuthorizationResult::FactoryDefaultPasswordForbidden
            } else {
                AuthorizationResult::Authorized
            }
        } else {
            AuthorizationResult::Unauthorized
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthorizationResult {
    Authorized,
    Unauthorized,
    FactoryDefaultPasswordForbidden,
}

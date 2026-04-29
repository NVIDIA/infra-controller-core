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

//! IMDS-style `GET …/meta-data/identity` (shared between carbide-agent and carbide-fmds).
//!
//! Numeric bounds are in [`forge_dpu_agent_utils::machine_identity::limits`] (also used by
//! carbide-host-support validation).

use std::convert::TryFrom;
use std::time::Duration;

use async_trait::async_trait;
use axum::http::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use forge_dpu_agent_utils::machine_identity::limits::{
    BURST_MAX, BURST_MIN, REQUESTS_PER_SECOND_MAX, REQUESTS_PER_SECOND_MIN, SIGN_TIMEOUT_SECS_MAX,
    SIGN_TIMEOUT_SECS_MIN, WAIT_TIMEOUT_SECS_MAX, WAIT_TIMEOUT_SECS_MIN,
};
use rpc::fmds::FmdsMachineIdentityConfig;
use rpc::forge::MachineIdentityResponse;

/// `meta-data` leaf name for machine identity (`…/meta-data/identity`).
pub const META_DATA_IDENTITY_CATEGORY: &str = "identity";

/// Upstream path appended to `sign-proxy-url` for HTTP pass-through (`{base}/latest/...`).
pub const SIGN_PROXY_UPSTREAM_IMDS_PREFIX: &str = "latest/meta-data/identity";

/// Validated, normalized machine-identity limits (see [*parse, don’t validate*](https://www.rustfinity.com/blog/parse-dont-validate)):
/// values are only obtainable through [`Self::try_from_limits`], [`TryFrom`] from `FmdsMachineIdentityConfig`,
/// or [`Self::fmds_default`] (known-good defaults). Use accessors; fields are private so callers cannot bypass parsing.
///
/// Agent TOML `[machine-identity]` keys use kebab-case; `FmdsMachineIdentityConfig` uses snake_case for the same fields.
///
/// ## What is validated where
///
/// - [`MachineIdentityParams::try_from_limits`] (and [`TryFrom`] / [`Self::try_from_fmds_proto`]):
///   numeric ranges, trim/empty normalization for proxy URL and TLS root CA path, and the rule that a CA
///   path requires a proxy URL.
/// - **Agent `MachineIdentityConfig::validate()`** (carbide-host-support): the above bounds **plus** HTTP(S)
///   scheme checks for `sign-proxy-url`, PEM file readability/parsing for `sign-proxy-tls-root-ca`, etc.
///
/// Call **`try_from_limits`** after agent **`validate()`**, or use **`TryFrom::try_from`** for **`FmdsMachineIdentityConfig`**
/// from gRPC. Callers build governors and HTTP clients from these values without re-running range checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineIdentityParams {
    requests_per_second: u8,
    burst: u8,
    wait_timeout_secs: u8,
    sign_timeout_secs: u8,
    sign_proxy_url: Option<String>,
    sign_proxy_tls_root_ca: Option<String>,
}

impl MachineIdentityParams {
    /// Defaults match `MachineIdentityConfig::default()` in carbide-host-support (agent).
    pub fn fmds_default() -> Self {
        Self {
            requests_per_second: 3,
            burst: 8,
            wait_timeout_secs: 2,
            sign_timeout_secs: 5,
            sign_proxy_url: None,
            sign_proxy_tls_root_ca: None,
        }
    }

    #[inline]
    pub fn requests_per_second(&self) -> u8 {
        self.requests_per_second
    }

    #[inline]
    pub fn burst(&self) -> u8 {
        self.burst
    }

    #[inline]
    pub fn wait_timeout_secs(&self) -> u8 {
        self.wait_timeout_secs
    }

    #[inline]
    pub fn sign_timeout_secs(&self) -> u8 {
        self.sign_timeout_secs
    }

    #[inline]
    pub fn sign_proxy_url(&self) -> Option<&str> {
        self.sign_proxy_url.as_deref()
    }

    #[inline]
    pub fn sign_proxy_tls_root_ca(&self) -> Option<&str> {
        self.sign_proxy_tls_root_ca.as_deref()
    }

    /// Single normalization path: range checks (see [`limits`]), trim,
    /// empty→`None`, and CA path requires proxy URL.
    ///
    /// Call after agent **`MachineIdentityConfig::validate()`** for file-backed config, or from
    /// [`Self::try_from_fmds_proto`] / [`Self::to_fmds_proto`] for the FMDS boundary.
    pub fn try_from_limits(
        requests_per_second: u8,
        burst: u8,
        wait_timeout_secs: u8,
        sign_timeout_secs: u8,
        sign_proxy_url: Option<&str>,
        sign_proxy_tls_root_ca: Option<&str>,
    ) -> Result<Self, String> {
        if !(REQUESTS_PER_SECOND_MIN..=REQUESTS_PER_SECOND_MAX).contains(&requests_per_second) {
            return Err(format!(
                "machine-identity.requests-per-second: must be between {REQUESTS_PER_SECOND_MIN} and {REQUESTS_PER_SECOND_MAX} (inclusive)"
            ));
        }
        if !(BURST_MIN..=BURST_MAX).contains(&burst) {
            return Err(format!(
                "machine-identity.burst: must be between {BURST_MIN} and {BURST_MAX} (inclusive)"
            ));
        }
        if !(WAIT_TIMEOUT_SECS_MIN..=WAIT_TIMEOUT_SECS_MAX).contains(&wait_timeout_secs) {
            return Err(format!(
                "machine-identity.wait-timeout-secs: must be between {WAIT_TIMEOUT_SECS_MIN} and {WAIT_TIMEOUT_SECS_MAX} (inclusive)"
            ));
        }
        if !(SIGN_TIMEOUT_SECS_MIN..=SIGN_TIMEOUT_SECS_MAX).contains(&sign_timeout_secs) {
            return Err(format!(
                "machine-identity.sign-timeout-secs: must be between {SIGN_TIMEOUT_SECS_MIN} and {SIGN_TIMEOUT_SECS_MAX} (inclusive)"
            ));
        }

        let sign_proxy_url = sign_proxy_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let sign_proxy_tls_root_ca = sign_proxy_tls_root_ca
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        if sign_proxy_url.is_none() && sign_proxy_tls_root_ca.is_some() {
            return Err(
                "machine-identity.sign-proxy-tls-root-ca: requires machine-identity.sign-proxy-url"
                    .to_string(),
            );
        }

        Ok(Self {
            requests_per_second,
            burst,
            wait_timeout_secs,
            sign_timeout_secs,
            sign_proxy_url,
            sign_proxy_tls_root_ca,
        })
    }

    /// [`TryFrom::try_from`] — preferred for protobuf (idiomatic “parse” entry point).
    pub fn try_from_fmds_proto(p: &FmdsMachineIdentityConfig) -> Result<Self, String> {
        Self::try_from(p)
    }

    pub fn to_fmds_proto(&self) -> FmdsMachineIdentityConfig {
        FmdsMachineIdentityConfig {
            requests_per_second: u32::from(self.requests_per_second),
            burst: u32::from(self.burst),
            wait_timeout_secs: u32::from(self.wait_timeout_secs),
            sign_timeout_secs: u32::from(self.sign_timeout_secs),
            sign_proxy_url: self.sign_proxy_url.clone(),
            sign_proxy_tls_root_ca: self.sign_proxy_tls_root_ca.clone(),
        }
    }
}

impl TryFrom<&FmdsMachineIdentityConfig> for MachineIdentityParams {
    type Error = String;

    fn try_from(p: &FmdsMachineIdentityConfig) -> Result<Self, Self::Error> {
        let requests_per_second = u8::try_from(p.requests_per_second).map_err(|_| {
            "machine-identity.requests-per-second: does not fit in u8 (proto field requests_per_second)"
                .to_string()
        })?;
        let burst = u8::try_from(p.burst).map_err(|_| {
            "machine-identity.burst: does not fit in u8 (proto field burst)".to_string()
        })?;
        let wait_timeout_secs = u8::try_from(p.wait_timeout_secs).map_err(|_| {
            "machine-identity.wait-timeout-secs: does not fit in u8 (proto field wait_timeout_secs)"
                .to_string()
        })?;
        let sign_timeout_secs = u8::try_from(p.sign_timeout_secs).map_err(|_| {
            "machine-identity.sign-timeout-secs: does not fit in u8 (proto field sign_timeout_secs)"
                .to_string()
        })?;

        Self::try_from_limits(
            requests_per_second,
            burst,
            wait_timeout_secs,
            sign_timeout_secs,
            p.sign_proxy_url.as_deref(),
            p.sign_proxy_tls_root_ca.as_deref(),
        )
    }
}

#[async_trait]
pub trait MetaDataIdentitySigner: Send + Sync {
    /// Rate-limit permit (governor) before signing or proxying.
    async fn wait_identity_permit(&self) -> Result<(), tonic::Status>;

    fn sign_proxy_base(&self) -> Option<String>;

    fn sign_proxy_http_client(&self) -> Option<reqwest::Client>;

    async fn sign_machine_identity(
        &self,
        audiences: Vec<String>,
    ) -> Result<MachineIdentityResponse, tonic::Status>;
}

/// Parses repeated `aud` query parameters (URL-decoded).
pub fn parse_identity_audiences(uri: &Uri) -> Vec<String> {
    let Some(query) = uri.query() else {
        return Vec::new();
    };
    url::form_urlencoded::parse(query.as_bytes())
        .filter(|(k, _)| k == "aud")
        .map(|(_, v)| v.into_owned())
        .collect()
}

pub fn metadata_header_is_true(headers: &HeaderMap) -> bool {
    headers
        .get("metadata")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.eq_ignore_ascii_case("true"))
}

pub fn accept_text_plain(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| {
            a.split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("text/plain"))
        })
}

pub fn map_grpc_status_to_http(status: &tonic::Status) -> StatusCode {
    use tonic::Code;
    match status.code() {
        Code::Ok => StatusCode::OK,
        Code::Cancelled => StatusCode::REQUEST_TIMEOUT,
        Code::Unknown => StatusCode::BAD_GATEWAY,
        Code::InvalidArgument => StatusCode::BAD_REQUEST,
        Code::DeadlineExceeded => StatusCode::GATEWAY_TIMEOUT,
        Code::NotFound => StatusCode::NOT_FOUND,
        Code::AlreadyExists => StatusCode::CONFLICT,
        Code::PermissionDenied => StatusCode::FORBIDDEN,
        Code::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
        Code::FailedPrecondition => StatusCode::BAD_REQUEST,
        Code::Aborted => StatusCode::CONFLICT,
        Code::OutOfRange => StatusCode::BAD_REQUEST,
        Code::Unimplemented => StatusCode::NOT_IMPLEMENTED,
        Code::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        Code::DataLoss => StatusCode::INTERNAL_SERVER_ERROR,
        Code::Unauthenticated => StatusCode::UNAUTHORIZED,
    }
}

pub fn build_sign_proxy_http_client(
    timeout: Duration,
    root_ca_pem_path: Option<&str>,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(timeout);
    if let Some(path) = root_ca_pem_path {
        let pem = std::fs::read(path).map_err(|e| {
            format!("machine-identity.sign-proxy-tls-root-ca: failed to read {path}: {e}")
        })?;
        let certs = reqwest::Certificate::from_pem_bundle(&pem).map_err(|e| {
            format!("machine-identity.sign-proxy-tls-root-ca: invalid PEM in {path}: {e}")
        })?;
        for cert in certs {
            builder = builder.add_root_certificate(cert);
        }
    }
    builder
        .build()
        .map_err(|e| format!("machine-identity.sign-proxy-url: failed to build HTTP client ({e})"))
}

pub fn build_sign_proxy_request_url(base_url: &str, query: Option<&str>) -> Result<String, String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("machine-identity.sign-proxy-url: base URL is empty".to_string());
    }
    let q = query
        .filter(|q| !q.is_empty())
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    Ok(format!("{base}/{SIGN_PROXY_UPSTREAM_IMDS_PREFIX}{q}"))
}

pub async fn forward_sign_proxy_http(
    client: &reqwest::Client,
    base_url: &str,
    request_uri: &Uri,
    headers: &HeaderMap,
) -> Response {
    let upstream_url = match build_sign_proxy_request_url(base_url, request_uri.query()) {
        Ok(u) => u,
        Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
    };

    tracing::debug!(%upstream_url, "forwarding machine identity request to HTTP sign proxy");

    let mut req = client.get(upstream_url);
    if let Some(v) = headers.get("metadata")
        && let Ok(s) = v.to_str()
    {
        req = req.header("Metadata", s);
    }
    if let Some(v) = headers.get(ACCEPT)
        && let Ok(s) = v.to_str()
    {
        req = req.header(ACCEPT, s);
    }

    let upstream = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let code = if e.is_timeout() {
                StatusCode::GATEWAY_TIMEOUT
            } else {
                StatusCode::BAD_GATEWAY
            };
            return (code, e.to_string()).into_response();
        }
    };

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    let content_type = upstream
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| HeaderValue::from_bytes(v.as_bytes()).ok());

    let body_bytes = match upstream.bytes().await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };

    let mut res = Response::builder().status(status);
    if let Some(ct) = content_type {
        res = res.header(CONTENT_TYPE, ct);
    }
    match res.body(axum::body::Body::from(body_bytes)) {
        Ok(r) => r,
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("machine-identity.sign-proxy-url: failed to build HTTP response ({e})"),
        )
            .into_response(),
    }
}

#[derive(serde::Serialize)]
struct IdentityTokenJsonBody {
    access_token: String,
    issued_token_type: String,
    token_type: String,
    expires_in: u32,
}

pub async fn serve_meta_data_identity<S: MetaDataIdentitySigner + ?Sized>(
    signer: &S,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if !metadata_header_is_true(&headers) {
        return (
            StatusCode::BAD_REQUEST,
            "Metadata: true header is required for meta-data/identity\n",
        )
            .into_response();
    }

    if let Err(e) = signer.wait_identity_permit().await {
        let code = map_grpc_status_to_http(&e);
        return (code, e.message().to_string()).into_response();
    }

    if let Some(base) = signer.sign_proxy_base() {
        let Some(client) = signer.sign_proxy_http_client() else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "machine-identity.sign-proxy-url: HTTP client is not configured\n",
            )
                .into_response();
        };
        return forward_sign_proxy_http(&client, &base, &uri, &headers).await;
    }

    let audiences = parse_identity_audiences(&uri);
    let resp = match signer.sign_machine_identity(audiences).await {
        Ok(r) => r,
        Err(e) => {
            let code = map_grpc_status_to_http(&e);
            return (code, e.message().to_string()).into_response();
        }
    };

    let body = IdentityTokenJsonBody {
        access_token: resp.access_token,
        issued_token_type: resp.issued_token_type,
        token_type: resp.token_type,
        expires_in: resp.expires_in_sec,
    };
    let json = match serde_json::to_string(&body) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("machine-identity: failed to serialize identity response ({e})"),
            )
                .into_response();
        }
    };

    let content_type = if accept_text_plain(&headers) {
        "text/plain; charset=utf-8"
    } else {
        "application/json"
    };
    let mut res = (StatusCode::OK, json).into_response();
    res.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
    res
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::routing::get;
    use http_body_util::BodyExt;

    use super::*;

    #[test]
    fn parse_identity_audiences_repeated_and_decoded() {
        let uri: Uri = "http://127.0.0.1/latest/meta-data/identity?aud=spiffe%3A%2F%2Fa&aud=b"
            .parse()
            .unwrap();
        assert_eq!(
            parse_identity_audiences(&uri),
            vec!["spiffe://a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn metadata_header_is_true_accepts_case_insensitive() {
        let mut h = HeaderMap::new();
        assert!(!metadata_header_is_true(&h));
        h.insert("metadata", HeaderValue::from_static("true"));
        assert!(metadata_header_is_true(&h));
        let mut h2 = HeaderMap::new();
        h2.insert("metadata", HeaderValue::from_static("TRUE"));
        assert!(metadata_header_is_true(&h2));
    }

    #[test]
    fn machine_identity_params_try_from_fmds_proto_trims_url() {
        let p = FmdsMachineIdentityConfig {
            requests_per_second: 5,
            burst: 10,
            wait_timeout_secs: 3,
            sign_timeout_secs: 6,
            sign_proxy_url: Some("  https://sign.example  ".to_string()),
            sign_proxy_tls_root_ca: None,
        };
        let params = MachineIdentityParams::try_from(&p).unwrap();
        assert_eq!(params.sign_proxy_url(), Some("https://sign.example"));
    }

    #[test]
    fn machine_identity_params_try_from_trait_agrees_with_try_from_fmds_proto() {
        let p = FmdsMachineIdentityConfig {
            requests_per_second: 5,
            burst: 10,
            wait_timeout_secs: 3,
            sign_timeout_secs: 6,
            sign_proxy_url: None,
            sign_proxy_tls_root_ca: None,
        };
        assert_eq!(
            MachineIdentityParams::try_from(&p).unwrap(),
            MachineIdentityParams::try_from_fmds_proto(&p).unwrap()
        );
    }

    #[test]
    fn try_from_fmds_proto_matches_try_from_limits() {
        let proto = FmdsMachineIdentityConfig {
            requests_per_second: 5,
            burst: 10,
            wait_timeout_secs: 3,
            sign_timeout_secs: 6,
            sign_proxy_url: Some("  https://sign.example  ".to_string()),
            sign_proxy_tls_root_ca: None,
        };
        let a = MachineIdentityParams::try_from_fmds_proto(&proto).unwrap();
        let b = MachineIdentityParams::try_from_limits(
            5,
            10,
            3,
            6,
            Some("  https://sign.example  "),
            None,
        )
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(
            a.to_fmds_proto(),
            FmdsMachineIdentityConfig {
                requests_per_second: 5,
                burst: 10,
                wait_timeout_secs: 3,
                sign_timeout_secs: 6,
                sign_proxy_url: Some("https://sign.example".to_string()),
                sign_proxy_tls_root_ca: None,
            }
        );
    }

    #[test]
    fn accept_text_plain_detects_header() {
        let mut h = HeaderMap::new();
        assert!(!accept_text_plain(&h));
        h.insert(ACCEPT, HeaderValue::from_static("application/json"));
        assert!(!accept_text_plain(&h));
        let mut h2 = HeaderMap::new();
        h2.insert(ACCEPT, HeaderValue::from_static("text/plain"));
        assert!(accept_text_plain(&h2));
    }

    #[test]
    fn build_sign_proxy_request_url_appends_path_and_query() {
        assert_eq!(
            build_sign_proxy_request_url("http://127.0.0.1:9/foo", Some("aud=x")).unwrap(),
            "http://127.0.0.1:9/foo/latest/meta-data/identity?aud=x"
        );
        assert_eq!(
            build_sign_proxy_request_url("http://127.0.0.1:9/foo/", None).unwrap(),
            "http://127.0.0.1:9/foo/latest/meta-data/identity"
        );
    }

    #[tokio::test]
    async fn forward_sign_proxy_http_passes_through() {
        let path = format!("/{}", SIGN_PROXY_UPSTREAM_IMDS_PREFIX);
        let app = Router::new().route(
            path.as_str(),
            get(|| async {
                (
                    StatusCode::CREATED,
                    [(CONTENT_TYPE, "application/special")],
                    "custom-token-body",
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        let base = format!("http://{}", addr);
        let uri: Uri = "http://client/latest/meta-data/identity?aud=test"
            .parse()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("metadata", HeaderValue::from_static("true"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        let res = forward_sign_proxy_http(&client, &base, &uri, &headers).await;
        assert_eq!(res.status(), StatusCode::CREATED);
        assert_eq!(
            res.headers().get(CONTENT_TYPE).unwrap().as_bytes(),
            b"application/special"
        );
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"custom-token-body");
        server.abort();
    }
}

/*
 * SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! SPIFFE `subject_prefix` validation and defaulting against JWT `issuer` trust domain.
//!
//! Character validation is **allowlist-based** (URL-safe ASCII subsets) plus structural checks
//! (`IpAddr`, DNS trust-domain regex, `Url::parse`, path segment regex).

use std::net::IpAddr;

use lazy_static::lazy_static;
use regex::Regex;
use url::Url;

/// Upper bound for stored / configured issuer strings (JWT `iss` is unbounded in theory).
const MAX_ISSUER_BYTES: usize = 2048;
/// Upper bound for `subject_prefix` (SPIFFE ID prefix + optional path).
const MAX_SUBJECT_PREFIX_BYTES: usize = 2048;
/// DNS hostname max length (octets) per RFC 1035; IPv6 textual forms are shorter.
const MAX_TRUST_DOMAIN_BYTES: usize = 253;
/// Reasonable cap on SPIFFE path segments after the trust domain.
const MAX_SPIFFE_PATH_SEGMENTS: usize = 64;
/// Per path segment after `spiffe://<td>/` (generous; DNS labels are ≤63).
const MAX_SPIFFE_PATH_SEGMENT_BYTES: usize = 256;

lazy_static! {
    static ref TRUST_DOMAIN_DNS: Regex = Regex::new(
        r"(?i)^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)*[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$"
    )
    .unwrap();
    static ref PATH_SEGMENT: Regex = Regex::new(r"^[a-zA-Z0-9._-]+$").unwrap();
}

/// URL-style JWT `iss` strings: unreserved + `gen-delims` / `sub-delims` subset we need for
/// `http(s)://…` and `spiffe://…`, without `%` / `#` (no percent-encoding or fragments) or `\`.
/// `@` is allowed so `Url::parse` can detect userinfo (rejected separately).
fn byte_allowed_in_issuer(b: u8) -> bool {
    matches!(
        b,
        b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b':'
            | b'/'
            | b'['
            | b']'
            | b'@'
            | b'!'
            | b'$'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'='
            | b'?'
            | b'&'
    )
}

/// `spiffe://…` subject prefix: same as issuer except `@` (never valid in SPIFFE ID grammar).
fn byte_allowed_in_subject_prefix(b: u8) -> bool {
    b != b'@' && byte_allowed_in_issuer(b)
}

fn validate_issuer_chars(issuer: &str) -> Result<(), String> {
    if !issuer.is_ascii() {
        return Err("issuer must contain only ASCII characters".into());
    }
    for &b in issuer.as_bytes() {
        if !byte_allowed_in_issuer(b) {
            return Err(
                "issuer contains a disallowed character (URL-safe ASCII subset; no percent-encoding or fragment)"
                    .into(),
            );
        }
    }
    Ok(())
}

/// Bare hostname / IP literal form: no `/`, `@`, scheme, etc. (only what can appear in a DNS name or IP string).
fn byte_allowed_bare_issuer(b: u8) -> bool {
    matches!(
        b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b':' | b'[' | b']'
    )
}

fn validate_bare_issuer_chars(issuer: &str) -> Result<(), String> {
    if !issuer.is_ascii() {
        return Err("issuer must contain only ASCII characters".into());
    }
    for &b in issuer.as_bytes() {
        if !byte_allowed_bare_issuer(b) {
            return Err(
                "issuer contains a disallowed character in bare hostname form (use only letters, digits, - . : [ ] for DNS or IP, or use https:// or spiffe://)"
                    .into(),
            );
        }
    }
    Ok(())
}

fn validate_subject_prefix_chars(raw: &str) -> Result<(), String> {
    if !raw.is_ascii() {
        return Err("subject_prefix must contain only ASCII characters".into());
    }
    for &b in raw.as_bytes() {
        if !byte_allowed_in_subject_prefix(b) {
            return Err(
                "subject_prefix contains a disallowed character (use URL-safe ASCII without '@', '%', or '#')"
                    .into(),
            );
        }
    }
    Ok(())
}

fn enforce_max_len(len: usize, max: usize, field: &str) -> Result<(), String> {
    if len > max {
        return Err(format!("{field} exceeds maximum length ({max} bytes)"));
    }
    Ok(())
}

/// Returns bytes after `spiffe://` when the prefix matches ASCII case-insensitively (`SPIFFE://`, etc.).
fn strip_spiffe_issuer_prefix(issuer: &str) -> Option<&str> {
    const PREFIX: &[u8] = b"spiffe://";
    let b = issuer.as_bytes();
    if b.len() >= PREFIX.len() && b[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        Some(&issuer[PREFIX.len()..])
    } else {
        None
    }
}

fn normalize_trust_domain_token(host: &str) -> String {
    if host.parse::<IpAddr>().is_ok() {
        host.to_string()
    } else {
        host.to_ascii_lowercase()
    }
}

fn validate_trust_domain_structure(td: &str) -> Result<(), String> {
    if td.is_empty() {
        return Err("trust domain must be non-empty".into());
    }
    if td.len() > MAX_TRUST_DOMAIN_BYTES {
        return Err(format!(
            "trust domain exceeds maximum length ({MAX_TRUST_DOMAIN_BYTES} bytes)"
        ));
    }
    if td.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    if !TRUST_DOMAIN_DNS.is_match(td) {
        return Err(format!(
            "trust domain {td:?} is not a valid DNS hostname or IP address"
        ));
    }
    Ok(())
}

/// SPIFFE trust domain string derived from `issuer` (`https://` / `http://` URL host, `spiffe://`
/// URI (any ASCII case on the scheme), or bare hostname).
pub(super) fn trust_domain_from_issuer(issuer: &str) -> Result<String, String> {
    let issuer = issuer.trim();
    if issuer.is_empty() {
        return Err("issuer is required".into());
    }
    enforce_max_len(issuer.len(), MAX_ISSUER_BYTES, "issuer")?;

    if let Some(rest) = strip_spiffe_issuer_prefix(issuer) {
        validate_issuer_chars(issuer)?;
        let td = rest
            .split('/')
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "issuer: SPIFFE URI must include a trust domain".to_string())?;
        validate_trust_domain_structure(td)?;
        return Ok(normalize_trust_domain_token(td));
    }

    if issuer.contains("://") {
        validate_issuer_chars(issuer)?;
        let u = Url::parse(issuer).map_err(|e| format!("issuer: invalid URL ({e})"))?;
        match u.scheme() {
            "http" | "https" => {}
            "spiffe" => {
                if !u.username().is_empty() || u.password().is_some() {
                    return Err("issuer: invalid SPIFFE URI (userinfo not allowed)".into());
                }
                let host = u
                    .host_str()
                    .ok_or_else(|| "issuer: SPIFFE URI must include a trust domain".to_string())?;
                validate_trust_domain_structure(host)?;
                return Ok(normalize_trust_domain_token(host));
            }
            other => {
                return Err(format!(
                    "issuer: only http, https, or spiffe URLs are allowed (got {other:?})"
                ));
            }
        }
        if !u.username().is_empty() || u.password().is_some() {
            return Err("issuer: URL must not contain userinfo".into());
        }
        let host = u
            .host_str()
            .ok_or_else(|| "issuer: URL must have a host".to_string())?;
        validate_trust_domain_structure(host)?;
        return Ok(normalize_trust_domain_token(host));
    }

    validate_bare_issuer_chars(issuer)?;
    validate_trust_domain_structure(issuer)?;
    Ok(normalize_trust_domain_token(issuer))
}

fn default_subject_prefix(expected_td: &str) -> String {
    format!("spiffe://{expected_td}/")
}

fn validate_path_segments(path_raw: &str) -> Result<Vec<&str>, String> {
    if path_raw.is_empty() {
        return Ok(Vec::new());
    }
    if path_raw.ends_with('/') {
        return Err(
            "subject_prefix path must not end with '/' (except the root form spiffe://<td>/)"
                .into(),
        );
    }
    let mut out = Vec::new();
    for seg in path_raw.split('/') {
        if seg.is_empty() {
            return Err("subject_prefix path must not contain empty segments".into());
        }
        enforce_max_len(
            seg.len(),
            MAX_SPIFFE_PATH_SEGMENT_BYTES,
            "subject_prefix path segment",
        )?;
        if seg == "." || seg == ".." {
            return Err("subject_prefix path must not use '.' or '..' segments".into());
        }
        if !PATH_SEGMENT.is_match(seg) {
            return Err(format!(
                "subject_prefix path segment {seg:?} must match [a-zA-Z0-9._-]+"
            ));
        }
        out.push(seg);
        if out.len() > MAX_SPIFFE_PATH_SEGMENTS {
            return Err(format!(
                "subject_prefix path must not exceed {MAX_SPIFFE_PATH_SEGMENTS} segments"
            ));
        }
    }
    Ok(out)
}

fn validate_and_canonicalize_subject_prefix(
    raw: &str,
    expected_td: &str,
) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(default_subject_prefix(expected_td));
    }
    enforce_max_len(raw.len(), MAX_SUBJECT_PREFIX_BYTES, "subject_prefix")?;
    validate_subject_prefix_chars(raw)?;
    let body = raw
        .strip_prefix("spiffe://")
        .ok_or_else(|| "subject_prefix must use the spiffe:// scheme".to_string())?;

    let (td_raw, path_raw) = match body.find('/') {
        None => (body, ""),
        Some(0) => return Err("subject_prefix is missing a trust domain after spiffe://".into()),
        Some(i) => (&body[..i], &body[i + 1..]),
    };

    validate_trust_domain_structure(td_raw)?;
    let td_norm = normalize_trust_domain_token(td_raw);
    if td_norm != expected_td {
        return Err(format!(
            "subject_prefix trust domain {td_raw:?} does not match issuer trust domain (expected {expected_td:?})"
        ));
    }

    let segments = validate_path_segments(path_raw)?;
    if segments.is_empty() {
        Ok(default_subject_prefix(expected_td))
    } else {
        Ok(format!("spiffe://{expected_td}/{}", segments.join("/")))
    }
}

/// Resolves optional proto `subject_prefix`: default `spiffe://<expected_td>/` or validated user value.
pub(super) fn resolve_subject_prefix(
    expected_td: &str,
    proto_subject_prefix: Option<&str>,
) -> Result<String, String> {
    match proto_subject_prefix {
        None | Some("") => Ok(default_subject_prefix(expected_td)),
        Some(s) => validate_and_canonicalize_subject_prefix(s, expected_td),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve_identity(issuer: &str, proto: Option<&str>) -> Result<String, String> {
        let td = trust_domain_from_issuer(issuer)?;
        resolve_subject_prefix(&td, proto)
    }

    #[test]
    fn trust_domain_https_issuer() {
        assert_eq!(
            trust_domain_from_issuer("https://Issuer.EXAMPLE/path").unwrap(),
            "issuer.example"
        );
    }

    #[test]
    fn trust_domain_spiffe_issuer() {
        assert_eq!(
            trust_domain_from_issuer("spiffe://Issuer.EXAMPLE/bundle").unwrap(),
            "issuer.example"
        );
    }

    #[test]
    fn trust_domain_spiffe_issuer_scheme_any_case() {
        assert_eq!(
            trust_domain_from_issuer("SPIFFE://Issuer.EXAMPLE/bundle").unwrap(),
            "issuer.example"
        );
        assert_eq!(
            trust_domain_from_issuer("SpIfFe://issuer.example").unwrap(),
            "issuer.example"
        );
    }

    #[test]
    fn resolve_identity_defaults_prefix() {
        assert_eq!(
            resolve_identity("https://my.idp.example", None).unwrap(),
            "spiffe://my.idp.example/"
        );
    }

    #[test]
    fn resolve_identity_spiffe_form_issuer() {
        assert_eq!(
            resolve_identity("spiffe://my.idp.example/ns/x", None).unwrap(),
            "spiffe://my.idp.example/"
        );
    }

    #[test]
    fn explicit_prefix_canonicalizes_td_case() {
        let p =
            resolve_identity("https://issuer.example", Some("spiffe://ISSUER.EXAMPLE/wl")).unwrap();
        assert_eq!(p, "spiffe://issuer.example/wl");
    }

    #[test]
    fn wrong_td_rejected() {
        let err = resolve_identity("https://issuer.example", Some("spiffe://other.example/"))
            .unwrap_err();
        assert!(err.contains("does not match"));
    }

    #[test]
    fn percent_encoding_rejected() {
        let err = resolve_identity(
            "https://issuer.example",
            Some("spiffe://issuer.example/a%2Fb"),
        )
        .unwrap_err();
        assert!(err.contains("disallowed"), "{err}");
    }

    #[test]
    fn https_scheme_subject_prefix_rejected() {
        let err = resolve_identity("https://issuer.example", Some("https://issuer.example/p"))
            .unwrap_err();
        assert!(err.contains("spiffe://"));
    }

    #[test]
    fn https_userinfo_rejected() {
        let err = trust_domain_from_issuer("https://user@issuer.example/").unwrap_err();
        assert!(err.contains("userinfo"), "{err}");
    }

    #[test]
    fn https_password_in_userinfo_rejected() {
        let err = trust_domain_from_issuer("https://user:pass@issuer.example/").unwrap_err();
        assert!(err.contains("userinfo"), "{err}");
    }

    #[test]
    fn non_http_scheme_rejected() {
        let err = trust_domain_from_issuer("ftp://issuer.example/").unwrap_err();
        assert!(err.contains("http"), "{err}");
    }

    #[test]
    fn bare_issuer_must_not_contain_slash() {
        let err = trust_domain_from_issuer("issuer.example/extra").unwrap_err();
        assert!(
            err.contains("bare hostname") || err.contains("disallowed"),
            "{err}"
        );
    }

    #[test]
    fn issuer_backslash_rejected() {
        let err = trust_domain_from_issuer("https://issuer.example\\evil").unwrap_err();
        assert!(err.contains("disallowed"), "{err}");
    }

    #[test]
    fn issuer_too_long_rejected() {
        let long = format!("https://{}.example/", "a".repeat(MAX_ISSUER_BYTES));
        let err = trust_domain_from_issuer(&long).unwrap_err();
        assert!(err.contains("maximum length"), "{err}");
    }

    #[test]
    fn issuer_control_char_rejected() {
        // NUL in the middle — trailing/leading whitespace is trimmed, so avoid only `\n` at the end.
        let err = trust_domain_from_issuer("https://issuer.ex\0ample.com/").unwrap_err();
        assert!(err.contains("disallowed") || err.contains("ASCII"), "{err}");
    }

    #[test]
    fn subject_prefix_backslash_rejected() {
        let err = resolve_identity(
            "https://issuer.example",
            Some("spiffe://issuer.example/a\\b"),
        )
        .unwrap_err();
        assert!(err.contains("disallowed"), "{err}");
    }

    #[test]
    fn subject_prefix_whitespace_rejected() {
        let err = resolve_identity(
            "https://issuer.example",
            Some("spiffe://issuer.example/a b"),
        )
        .unwrap_err();
        assert!(err.contains("disallowed"), "{err}");
    }

    #[test]
    fn dns_trust_domain_too_long_rejected() {
        let label = "a".repeat(63);
        let host = std::iter::repeat_n(label.as_str(), 5)
            .collect::<Vec<_>>()
            .join(".");
        assert!(host.len() > MAX_TRUST_DOMAIN_BYTES);
        let issuer = format!("https://{host}/");
        let err = trust_domain_from_issuer(&issuer).unwrap_err();
        assert!(
            err.contains("maximum length") || err.contains("not a valid DNS"),
            "{err}"
        );
    }

    #[test]
    fn path_segment_too_long_rejected() {
        let seg = "x".repeat(MAX_SPIFFE_PATH_SEGMENT_BYTES + 1);
        let prefix = format!("spiffe://issuer.example/{seg}");
        let err = resolve_identity("https://issuer.example", Some(&prefix)).unwrap_err();
        assert!(err.contains("maximum length"), "{err}");
    }

    #[test]
    fn too_many_path_segments_rejected() {
        let segs = std::iter::repeat_n("w", MAX_SPIFFE_PATH_SEGMENTS + 1)
            .collect::<Vec<_>>()
            .join("/");
        let prefix = format!("spiffe://issuer.example/{segs}");
        let err = resolve_identity("https://issuer.example", Some(&prefix)).unwrap_err();
        assert!(err.contains("segments"), "{err}");
    }
}

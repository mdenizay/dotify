//! Streamable HTTP transport, protected by a bearer token.
//!
//! `POST/GET/DELETE /mcp` needs `Authorization: Bearer <token>`. For
//! clients that cannot send headers (ChatGPT developer mode connectors
//! only offer OAuth or no authentication), `allow_token_in_url` also
//! accepts `/mcp/<token>`: a secret URL, so only share it like a password.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio_util::sync::CancellationToken;

use crate::server::McpServer;

/// Shortest accepted token, in characters.
pub const MIN_TOKEN_LEN: usize = 24;

pub struct HttpOptions {
    pub token: String,
    pub allow_token_in_url: bool,
    /// Extra `Host` header values to accept besides loopback, such as a
    /// tunnel's public hostname.
    pub allowed_hosts: Vec<String>,
    pub cancel: CancellationToken,
}

#[derive(Clone)]
struct Auth {
    token: Arc<str>,
    allow_token_in_url: bool,
}

/// A random 256-bit token as 64 hex characters.
pub fn generate_token() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn validate_token(token: &str) -> Result<(), String> {
    if token.len() < MIN_TOKEN_LEN {
        return Err(format!("the token must be at least {MIN_TOKEN_LEN} characters (try `dotify-mcp token`)"));
    }
    if !token.bytes().all(|b| b.is_ascii_graphic()) || token.contains('/') {
        return Err("the token must be printable ASCII without spaces or '/'".into());
    }
    Ok(())
}

/// Comparison whose time does not depend on where the inputs differ.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Bearer")], "missing or invalid bearer token\n")
        .into_response()
}

async fn check_auth(State(auth): State<Auth>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let ok = if path == "/mcp" {
        request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|t| constant_time_eq(t.trim().as_bytes(), auth.token.as_bytes()))
    } else if let Some(t) = path.strip_prefix("/mcp/") {
        auth.allow_token_in_url && constant_time_eq(t.trim_end_matches('/').as_bytes(), auth.token.as_bytes())
    } else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !ok {
        return unauthorized();
    }
    next.run(request).await
}

/// The HTTP app: the MCP endpoint behind token auth.
pub fn router(server: McpServer, opts: HttpOptions) -> Router {
    let mut hosts: Vec<String> = vec!["localhost".into(), "127.0.0.1".into(), "::1".into()];
    hosts.extend(opts.allowed_hosts);
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(hosts)
        .with_cancellation_token(opts.cancel.child_token());
    let service: StreamableHttpService<McpServer, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(server.clone()), Default::default(), config);
    let auth = Auth { token: opts.token.into(), allow_token_in_url: opts.allow_token_in_url };
    Router::new()
        .route_service("/mcp", service.clone())
        .route_service("/mcp/{token}", service)
        .fallback(|| async { StatusCode::NOT_FOUND })
        .layer(middleware::from_fn_with_state(auth, check_auth))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let t = generate_token().unwrap();
        assert_eq!(t.len(), 64);
        assert!(validate_token(&t).is_ok());
        assert_ne!(t, generate_token().unwrap());
        assert!(validate_token("short").is_err());
        assert!(validate_token(&format!("{t} x")).is_err());
        assert!(validate_token(&format!("{t}/x")).is_err());
    }

    #[test]
    fn compares_exactly() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}

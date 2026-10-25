//! Token auth and host checks on the Streamable HTTP transport.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use dotify_mcp::http::{router, HttpOptions};
use dotify_mcp::{Dotify, McpServer, Options, Sandbox};
use http_body_util::BodyExt;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";
const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#;

fn app(root: &std::path::Path, allow_token_in_url: bool) -> axum::Router {
    let sandbox = Sandbox::new([root.to_path_buf()]).unwrap();
    let dotify = Arc::new(Dotify::new(Options { sandbox, read_only: false }));
    router(
        McpServer::new(dotify),
        HttpOptions {
            token: TOKEN.into(),
            allow_token_in_url,
            allowed_hosts: vec!["dotify.example.com".into()],
            cancel: CancellationToken::new(),
        },
    )
}

fn init(path: &str, host: &str, auth: Option<&str>) -> Request<Body> {
    let mut b = Request::post(path)
        .header("host", host)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(a) = auth {
        b = b.header("authorization", a);
    }
    b.body(Body::from(INIT)).unwrap()
}

async fn status(app: &axum::Router, req: Request<Body>) -> (StatusCode, String) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    // SSE responses stay open: read events until the JSON-RPC result.
    let mut body = res.into_body();
    let mut text = String::new();
    for _ in 0..8 {
        match body.frame().await {
            Some(Ok(frame)) => {
                if let Ok(d) = frame.into_data() {
                    text.push_str(&String::from_utf8_lossy(&d));
                }
                if text.contains("\"result\"") {
                    break;
                }
            }
            _ => break,
        }
    }
    (status, text)
}

#[tokio::test]
async fn requires_the_bearer_token() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path(), false);
    let host = "127.0.0.1:8765";
    assert_eq!(status(&app, init("/mcp", host, None)).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(status(&app, init("/mcp", host, Some("Bearer wrong"))).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(status(&app, init("/mcp", host, Some(TOKEN))).await.0, StatusCode::UNAUTHORIZED);
    let wrong_len = format!("Bearer {TOKEN}0");
    assert_eq!(status(&app, init("/mcp", host, Some(&wrong_len))).await.0, StatusCode::UNAUTHORIZED);
    let (code, body) = status(&app, init("/mcp", host, Some(&format!("Bearer {TOKEN}")))).await;
    assert_eq!(code, StatusCode::OK);
    assert!(body.contains("dotify-mcp"), "{body}");
    // Token in the URL is off unless enabled.
    assert_eq!(status(&app, init(&format!("/mcp/{TOKEN}"), host, None)).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(status(&app, init("/other", host, None)).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn accepts_the_token_in_the_url_when_enabled() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path(), true);
    let host = "dotify.example.com";
    assert_eq!(status(&app, init(&format!("/mcp/{TOKEN}"), host, None)).await.0, StatusCode::OK);
    assert_eq!(status(&app, init("/mcp/not-the-token-at-all-0000000", host, None)).await.0, StatusCode::UNAUTHORIZED);
    // The header form still works and is still required on /mcp.
    assert_eq!(status(&app, init("/mcp", host, None)).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_unknown_hosts() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path(), false);
    let (code, _) = status(&app, init("/mcp", "evil.example.net", Some(&format!("Bearer {TOKEN}")))).await;
    assert!(code.is_client_error(), "{code}");
}

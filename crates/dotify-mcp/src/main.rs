//! `dotify-mcp` – Model Context Protocol server for Dotify projects.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use dotify_mcp::http::{self, HttpOptions};
use dotify_mcp::{Dotify, McpServer, Options, Sandbox};
use rmcp::ServiceExt;
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(name = "dotify-mcp", version, about = "Model Context Protocol server for Dotify projects")]
struct Cli {
    /// Directory the server may read and write; repeat for several. Relative
    /// tool paths resolve against the first one.
    #[arg(long = "root", global = true, env = "DOTIFY_MCP_ROOTS", value_delimiter = ':')]
    roots: Vec<PathBuf>,
    /// Only offer tools that read: no edits, saves or exports.
    #[arg(long, global = true, env = "DOTIFY_MCP_READ_ONLY")]
    read_only: bool,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve over stdin/stdout (Claude Code, Claude Desktop). The default.
    Stdio,
    /// Serve Streamable HTTP at /mcp (ChatGPT developer mode via a tunnel).
    Http {
        /// Address to listen on. Keep it on loopback and expose it through
        /// an HTTPS tunnel.
        #[arg(long, default_value = "127.0.0.1:8765")]
        bind: SocketAddr,
        /// Bearer token clients must send. Prefer the environment variable,
        /// which other users cannot see in the process list.
        #[arg(long, env = "DOTIFY_MCP_TOKEN", hide_env_values = true)]
        token: String,
        /// Also accept the token as the last URL segment, /mcp/<token>, for
        /// clients that cannot send an Authorization header.
        #[arg(long)]
        allow_token_in_url: bool,
        /// Accept this Host header too (for example the tunnel hostname);
        /// repeat for several. Loopback names are always accepted.
        #[arg(long = "allowed-host")]
        allowed_hosts: Vec<String>,
    },
    /// Print a new random token for `http`.
    Token,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn run(cli: Cli) -> Result<()> {
    if let Some(Cmd::Token) = cli.command {
        println!("{}", http::generate_token()?);
        return Ok(());
    }
    if cli.roots.is_empty() {
        return Err("pass at least one --root directory (or set DOTIFY_MCP_ROOTS)".into());
    }
    let sandbox = Sandbox::new(cli.roots)?;
    let dotify = Arc::new(Dotify::new(Options { sandbox, read_only: cli.read_only }));
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async move {
        match cli.command {
            None | Some(Cmd::Stdio) => serve_stdio(dotify).await,
            Some(Cmd::Http { bind, token, allow_token_in_url, allowed_hosts }) => {
                serve_http(dotify, bind, token, allow_token_in_url, allowed_hosts).await
            }
            Some(Cmd::Token) => unreachable!(),
        }
    })
}

async fn serve_stdio(dotify: Arc<Dotify>) -> Result<()> {
    let service = McpServer::new(dotify).serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

async fn serve_http(
    dotify: Arc<Dotify>,
    bind: SocketAddr,
    token: String,
    allow_token_in_url: bool,
    allowed_hosts: Vec<String>,
) -> Result<()> {
    http::validate_token(&token)?;
    if !bind.ip().is_loopback() {
        eprintln!("warning: listening on {bind}, which is reachable from other machines");
    }
    let roots: Vec<String> = dotify.sandbox().roots().iter().map(|r| r.display().to_string()).collect();
    let cancel = CancellationToken::new();
    let app = http::router(
        McpServer::new(dotify.clone()),
        HttpOptions { token, allow_token_in_url, allowed_hosts, cancel: cancel.clone() },
    );
    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!(
        "dotify-mcp listening on http://{}/mcp (roots: {}{})",
        listener.local_addr()?,
        roots.join(", "),
        if dotify.read_only() { ", read-only" } else { "" }
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancel.cancel();
        })
        .await?;
    Ok(())
}

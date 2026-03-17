//! Web server command implementation.
//!
//! This module powers the `ember serve` command which starts
//! an HTTP API server for the Ember AI agent.
//!
//! The server allows Ember to be accessed by:
//! - web frontends
//! - external tools
//! - integrations
//!
//! Examples:
//!
//! Start the server on default port:
//! ```bash
//! ember serve
//! ```
//!
//! Start server on a custom port:
//! ```bash
//! ember serve --port 8080
//! ```
//!
//! Serve a frontend directory:
//! ```bash
//! ember serve --static-dir ./frontend
//! ```

use anyhow::Result;
use clap::Args;
use tracing::info;

/// Serve command arguments
#[derive(Args, Debug)]
pub struct ServeArgs {
    #[arg(
        short,
        long,
        default_value = "3000",
        help = "Port for the HTTP server",
        long_help = "Port for the HTTP server.

Examples:
  ember serve --port 8080
  ember serve --port 5000"
    )]
    pub port: u16,

    #[arg(
        long,
        default_value = "0.0.0.0",
        help = "Host address to bind the server",
        long_help = "Host address to bind the server.

Examples:
  ember serve --host 127.0.0.1
  ember serve --host 0.0.0.0"
    )]
    pub host: String,

    #[arg(
        long,
        help = "Directory containing static frontend files",
        long_help = "Directory containing static frontend files.

Examples:
  ember serve --static-dir ./frontend
  ember serve --static-dir ./dist"
    )]
    pub static_dir: Option<String>,
}

/// Start the Ember web server.
///
/// This launches the HTTP API used by Ember.
/// Optionally serves static frontend files if `--static-dir` is provided.
///
/// The server will listen on the specified host and port.
pub async fn run(args: ServeArgs) -> Result<()> {
    info!(
        host = %args.host,
        port = args.port,
        static_dir = ?args.static_dir,
        "Starting Ember web server"
    );

    let config = ember_web::ServerConfig::new(&args.host, args.port);
    let state = ember_web::AppState::new(config);

    let app = if let Some(static_dir) = &args.static_dir {
        info!(static_dir = %static_dir, "Serving static files");
        ember_web::create_router_with_static(state, static_dir)
    } else {
        ember_web::create_router(state)
    };

    let addr = format!("{}:{}", args.host, args.port);
    info!(address = %addr, "Server listening");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

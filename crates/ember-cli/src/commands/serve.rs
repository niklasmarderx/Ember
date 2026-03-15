//! Web server command implementation.

use anyhow::Result;
use clap::Args;
use tracing::info;

/// Serve command arguments
#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Port to listen on
    #[arg(short, long, default_value = "3000")]
    pub port: u16,

    /// Host to bind to
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Path to static files directory (for frontend)
    #[arg(long)]
    pub static_dir: Option<String>,
}

/// Run the web server
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

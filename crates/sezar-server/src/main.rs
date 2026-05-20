//! `sezar-server` — V1 collector binary.
//!
//! Wires CLI flags into the [`sezar_server::AppState`] and serves
//! the router on a configurable address.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use sezar_server::{router, AppState};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "sezar-server", author, version, about)]
struct Args {
    /// Bind address (e.g. `0.0.0.0:8090`).
    #[arg(long, default_value = "0.0.0.0:8090")]
    listen: String,

    /// Deadline used for org-level posture, RFC 3339. Defaults to the
    /// NSA CNSA 2.0 browser/server-class deadline (2030-01-01).
    #[arg(long)]
    deadline: Option<String>,

    /// Horizon constant for deadline-tension computation (years).
    #[arg(long, default_value_t = 5.0)]
    horizon_years: f32,

    /// Directory holding the internal CA (`ca.crt`, `ca.key`).
    /// Generated on first boot, reloaded thereafter. The key
    /// file is created at mode 0600 on unix.
    #[arg(long, default_value = "/var/lib/sezar/ca")]
    ca_dir: PathBuf,

    /// Admin secret expected in `X-Admin-Token` on
    /// `POST /v1/admin/bootstrap-tokens`. Read from the
    /// environment variable `SEZAR_ADMIN_TOKEN` if not supplied
    /// on the CLI. When neither is set, the admin endpoint is
    /// disabled (returns 503).
    #[arg(long, env = "SEZAR_ADMIN_TOKEN")]
    admin_token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let mut state = AppState::new_in_memory(&args.ca_dir, args.admin_token.clone())?;
    if let Some(d) = args.deadline.as_deref() {
        state.default_deadline = chrono::DateTime::parse_from_rfc3339(d)?
            .with_timezone(&chrono::Utc);
    }
    state.horizon_years = args.horizon_years;

    info!(
        version = env!("CARGO_PKG_VERSION"),
        deadline = %state.default_deadline.to_rfc3339(),
        horizon = state.horizon_years,
        ca_dir = %args.ca_dir.display(),
        admin_enabled = state.admin_token.is_some(),
        "starting sezar-server"
    );

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    info!(addr = %args.listen, "sezar-server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

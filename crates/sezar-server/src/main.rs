//! `sezar-server` — V1 collector binary.
//!
//! Wires CLI flags into the [`sezar_server::AppState`] and serves
//! the router on a configurable address.

use std::net::SocketAddr;
use std::path::PathBuf;

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use sezar_server::{router, router_bootstrap, router_main, store, store_pg, tls, AppState};
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

    /// Turn on the TLS path: mints a CA-signed server cert at
    /// boot, serves the main routes on `--listen` behind an
    /// mTLS listener (client cert required, must be signed by
    /// the internal CA), and serves the bootstrap routes
    /// (`/v1/enrol`, `/v1/admin/bootstrap-tokens`, `/healthz`)
    /// on `--tls-bootstrap-listen` behind a TLS-only listener
    /// (no client cert). When this flag is off the server runs
    /// a single plain-HTTP listener on `--listen` (the legacy
    /// mode, used by the dev smoke + the in-process tests).
    #[arg(long, default_value_t = false)]
    tls: bool,

    /// Bootstrap-listener bind address when `--tls` is on.
    /// Defaults to the same host as `--listen` on port 8443.
    #[arg(long, default_value = "0.0.0.0:8443")]
    tls_bootstrap_listen: String,

    /// Additional Subject-Alternative-Names for the auto-minted
    /// server cert. `127.0.0.1`, `::1`, and `localhost` are
    /// always included; add the public DNS name or extra IPs
    /// here so curl --cacert verifies cleanly.
    #[arg(long = "tls-san", num_args = 0..)]
    tls_sans: Vec<String>,

    /// Postgres connection URL (e.g.
    /// `postgres://sezar:sezar@127.0.0.1:5432/sezar`). When set,
    /// the server persists events to Postgres and runs the
    /// bundled migrations on first boot. Without this flag the
    /// server uses an in-memory store (V1 default, suitable for
    /// the dev smoke and the acceptance script). Reads
    /// `SEZAR_DATABASE_URL` from the environment if not
    /// supplied on the CLI.
    #[arg(long, env = "SEZAR_DATABASE_URL")]
    database_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let store: Arc<dyn store::EventStore> = match args.database_url.as_deref() {
        Some(url) => Arc::new(
            store_pg::PgEventStore::connect(url)
                .await
                .context("init Postgres event store")?,
        ),
        None => Arc::new(store::InMemoryEventStore::new()),
    };
    let backend = if args.database_url.is_some() {
        "postgres"
    } else {
        "in-memory"
    };

    let mut state = AppState::with_store(store, &args.ca_dir, args.admin_token.clone())?;
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
        tls = args.tls,
        store = backend,
        "starting sezar-server"
    );

    if args.tls {
        run_tls(args, state).await
    } else {
        run_plain(args, state).await
    }
}

async fn run_plain(args: Args, state: AppState) -> Result<()> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    info!(addr = %args.listen, "sezar-server listening (plain HTTP)");
    // `into_make_service_with_connect_info` exposes the peer
    // address to the rate-limit middleware on the bootstrap routes.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn run_tls(args: Args, state: AppState) -> Result<()> {
    tls::install_default_crypto_provider();

    // Compose the SAN list: callers add public names / extra
    // IPs; we always include local loopbacks so an operator
    // doing `curl --cacert ca.crt https://127.0.0.1:8090/...`
    // gets a clean verification path.
    let mut sans: Vec<String> = vec![
        "localhost".into(),
        "127.0.0.1".into(),
        "::1".into(),
    ];
    for extra in &args.tls_sans {
        if !sans.iter().any(|s| s == extra) {
            sans.push(extra.clone());
        }
    }

    let server_cn = sans
        .iter()
        .find(|s| !s.parse::<std::net::IpAddr>().is_ok())
        .cloned()
        .unwrap_or_else(|| "sezar-server".into());

    let server_cert = state
        .ca
        .sign_server_cert(&server_cn, &sans, 365)
        .context("mint server cert")?;
    info!(cn = %server_cn, sans = ?sans, "minted server cert");

    let mtls_addr: SocketAddr = args
        .listen
        .parse()
        .with_context(|| format!("parse --listen as SocketAddr: {}", args.listen))?;
    let bootstrap_addr: SocketAddr = args
        .tls_bootstrap_listen
        .parse()
        .with_context(|| {
            format!(
                "parse --tls-bootstrap-listen as SocketAddr: {}",
                args.tls_bootstrap_listen
            )
        })?;

    let mtls_config =
        tls::build_mtls_config(&server_cert.cert_pem, &server_cert.key_pem, &server_cert.ca_cert_pem)?;
    let bootstrap_config =
        tls::build_bootstrap_config(&server_cert.cert_pem, &server_cert.key_pem)?;

    let main_app = router_main(state.clone());
    let bootstrap_app = router_bootstrap(state);

    info!(addr = %mtls_addr, "sezar-server mTLS main listener");
    info!(addr = %bootstrap_addr, "sezar-server TLS bootstrap listener");

    let mtls = axum_server::bind_rustls(
        mtls_addr,
        axum_server::tls_rustls::RustlsConfig::from_config(mtls_config),
    )
    .serve(main_app.into_make_service());
    let bootstrap = axum_server::bind_rustls(
        bootstrap_addr,
        axum_server::tls_rustls::RustlsConfig::from_config(bootstrap_config),
    )
    // Bootstrap routes are rate-limited per client; expose the peer
    // address so the limiter keys on the real source.
    .serve(bootstrap_app.into_make_service_with_connect_info::<std::net::SocketAddr>());

    // Both listeners run forever; if either dies we report it
    // and bring the whole server down so operators notice.
    tokio::select! {
        r = mtls => r.context("mTLS listener exited")?,
        r = bootstrap => r.context("bootstrap TLS listener exited")?,
    }
    Ok(())
}

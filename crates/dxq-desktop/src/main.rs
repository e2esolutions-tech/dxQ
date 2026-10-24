//! `dxq-desktop` — native (egui/eframe) monitor for the dxQ collector.
//!
//! A pure-Rust desktop client that polls the collector's REST API and
//! renders the live crypto-posture: org-level `q`, the asset
//! inventory, the BLOCKED set, and the `org_q` trend. It observes
//! only — no ingestion, no mutation — the same read surface the web
//! dashboard uses.
//!
//! ```text
//! cargo run -- --collector http://127.0.0.1:8090
//! # or: DXQ_COLLECTOR=https://dxq.internal:8443 cargo run
//! ```

mod app;
mod client;

use clap::Parser;

use app::DesktopApp;

#[derive(Parser, Debug)]
#[command(name = "dxq-desktop", version, about)]
struct Args {
    /// Base URL of the dxQ collector REST API.
    #[arg(long, env = "DXQ_COLLECTOR", default_value = "http://127.0.0.1:8090")]
    collector: String,

    /// Client certificate (PEM) for mTLS collectors. Requires
    /// --client-key. Signed by the collector's internal CA.
    #[arg(long, env = "DXQ_CLIENT_CERT")]
    client_cert: Option<std::path::PathBuf>,

    /// Client private key (PEM) for mTLS. Requires --client-cert.
    #[arg(long, env = "DXQ_CLIENT_KEY")]
    client_key: Option<std::path::PathBuf>,

    /// CA certificate (PEM) to trust the collector's server cert
    /// (the internal CA for a self-hosted collector).
    #[arg(long, env = "DXQ_CA_CERT")]
    ca_cert: Option<std::path::PathBuf>,
}

fn main() -> eframe::Result<()> {
    let args = Args::parse();

    // reqwest here uses rustls without a bundled provider; install the
    // `ring` provider as the process default so HTTPS collectors work.
    // Idempotent — ignore the error if one is already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([960.0, 680.0])
            .with_min_inner_size([640.0, 420.0])
            .with_title("dxQ — the dimension of quantum readiness"),
        ..Default::default()
    };

    let collector = args.collector;
    let tls = client::TlsConfig {
        client_cert: args.client_cert,
        client_key: args.client_key,
        ca_cert: args.ca_cert,
    };
    let secured = tls.is_mtls();
    eframe::run_native(
        "dxq-desktop",
        options,
        Box::new(move |cc| {
            // Spawn the background poller; it repaints egui when fresh
            // data arrives so the UI stays live while idle.
            let ctx = cc.egui_ctx.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            client::spawn_poller(collector.clone(), tls, tx, move || ctx.request_repaint());
            Ok(Box::new(DesktopApp::new(collector, secured, rx)))
        }),
    )
}

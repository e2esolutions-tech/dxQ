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
    eframe::run_native(
        "dxq-desktop",
        options,
        Box::new(move |cc| {
            // Spawn the background poller; it repaints egui when fresh
            // data arrives so the UI stays live while idle.
            let ctx = cc.egui_ctx.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            client::spawn_poller(collector.clone(), tx, move || ctx.request_repaint());
            Ok(Box::new(DesktopApp::new(collector, rx)))
        }),
    )
}

//! `sezar-net` — Phase 1 CLI.
//!
//! Subcommands:
//!
//! - `from-zgrab` — read `zgrab2 tls` JSON output (one record per
//!   line OR a wrapping JSON array) and emit one
//!   `crypto_inventory_event` per record, either to stdout (NDJSON)
//!   or to a downstream collector URL.
//! - `parse-handshake` — read raw TLS handshake bytes (hex on
//!   stdin or a file path) and pretty-print the parsed summary plus
//!   the resolved primitives. Handy for debugging captures.
//! - `pq-probe` — open a single TLS 1.3 handshake per host
//!   advertising `X25519MLKEM768`, record the negotiated kex group
//!   plus cert sig algo, emit NDJSON. Phase-1.5 PQ-readiness probe.
//!
//! Phase 2 (eBPF live mode) ships as `sezar-net live`.

use std::io::{BufRead, Read};
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use sezar_net::{pq_probe, tls, zgrab};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "sezar-net", author, version, about)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Read zgrab2 JSON output and emit crypto_inventory_event records.
    FromZgrab {
        /// Input path. `-` (default) reads stdin. NDJSON or a JSON array.
        #[arg(long, default_value = "-")]
        input: String,
        /// Optional downstream collector URL. When set, POST each
        /// event; otherwise print NDJSON to stdout.
        #[arg(long)]
        collector: Option<String>,
    },
    /// Parse raw TLS handshake bytes (hex) and dump the summary.
    ParseHandshake {
        /// Hex-encoded handshake bytes (or `-` to read stdin).
        #[arg(long)]
        hex: String,
    },
    /// PQ-capable TLS probe. One handshake per host advertising
    /// `X25519MLKEM768`. Emits NDJSON to stdout.
    PqProbe {
        /// Path to hosts file (one hostname per line). `-` for stdin.
        #[arg(long, default_value = "-")]
        hosts: String,
        /// Port (default 443).
        #[arg(long, default_value_t = 443)]
        port: u16,
        /// Per-host timeout in seconds.
        #[arg(long, default_value_t = 5)]
        timeout_s: u64,
        /// Rate cap — pause this many milliseconds between hosts.
        #[arg(long, default_value_t = 1000)]
        rate_delay_ms: u64,
    },
}

fn main() -> anyhow::Result<()> {
    // Send tracing output to stderr so subcommands that emit NDJSON
    // on stdout (e.g. `pq-probe`, `from-zgrab`) stay greppable.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    match args.cmd {
        Cmd::FromZgrab { input, collector } => run_from_zgrab(input, collector),
        Cmd::ParseHandshake { hex } => run_parse_handshake(hex),
        Cmd::PqProbe {
            hosts,
            port,
            timeout_s,
            rate_delay_ms,
        } => run_pq_probe(hosts, port, timeout_s, rate_delay_ms),
    }
}

fn run_pq_probe(
    hosts_path: String,
    port: u16,
    timeout_s: u64,
    rate_delay_ms: u64,
) -> anyhow::Result<()> {
    let raw = read_input(&hosts_path)?;
    let hosts: Vec<String> = raw
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    info!(count = hosts.len(), "pq probe loaded host list");

    // We use a current-thread tokio runtime: every host probe is
    // sequential by design (rate cap + ethics) so the multi-thread
    // pool would add nothing.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let timeout = Duration::from_secs(timeout_s);
    let delay = Duration::from_millis(rate_delay_ms);

    for (i, host) in hosts.iter().enumerate() {
        let result = rt.block_on(pq_probe::probe(host, port, timeout));
        if result.ok {
            info!(
                "[{:3}/{}] {host} {} {} kex={} pq={}",
                i + 1,
                hosts.len(),
                result.protocol_version.as_deref().unwrap_or("?"),
                result.cipher_suite.as_deref().unwrap_or("?"),
                result.kex_group.as_deref().unwrap_or("?"),
                result.kex_pq,
            );
        } else {
            warn!(
                "[{:3}/{}] {host} FAIL: {}",
                i + 1,
                hosts.len(),
                result.error.as_deref().unwrap_or("unknown")
            );
        }
        println!("{}", serde_json::to_string(&result)?);
        if i + 1 < hosts.len() {
            std::thread::sleep(delay);
        }
    }
    Ok(())
}

fn run_from_zgrab(input: String, collector: Option<String>) -> anyhow::Result<()> {
    let raw = read_input(&input)?;
    let records = parse_zgrab_payload(&raw)?;
    info!(count = records.len(), "zgrab records loaded");

    // We use blocking reqwest only when the operator opts in to a
    // downstream collector; the common stdout path stays sync.
    let client = collector
        .as_deref()
        .map(|_| reqwest::blocking::Client::builder().build())
        .transpose()
        .map_err(|e| anyhow::anyhow!("client build: {e}"))?;

    for rec in records {
        let ev = zgrab::event_from_zgrab(&rec);
        if let Some(url) = collector.as_deref() {
            let resp = client.as_ref().unwrap().post(url).json(&ev).send()?;
            if !resp.status().is_success() {
                warn!(status = %resp.status(), "downstream rejected event");
            }
        } else {
            println!("{}", serde_json::to_string(&ev)?);
        }
    }
    Ok(())
}

/// Accept either NDJSON (one record per line) or a single JSON array.
fn parse_zgrab_payload(raw: &str) -> anyhow::Result<Vec<zgrab::ZgrabRecord>> {
    let trimmed = raw.trim_start();
    if trimmed.starts_with('[') {
        let v: Vec<zgrab::ZgrabRecord> = serde_json::from_str(trimmed)?;
        return Ok(v);
    }
    let mut out = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<zgrab::ZgrabRecord>(line) {
            Ok(r) => out.push(r),
            Err(e) => {
                error!(line = i + 1, error = %e, "skipping malformed record");
            }
        }
    }
    Ok(out)
}

fn run_parse_handshake(hex_arg: String) -> anyhow::Result<()> {
    let raw_hex = if hex_arg == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else if let Ok(meta) = std::fs::metadata(&hex_arg) {
        if meta.is_file() {
            std::fs::read_to_string(&hex_arg)?
        } else {
            hex_arg
        }
    } else {
        hex_arg
    };
    let clean: String = raw_hex.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = hex::decode(clean.trim_start_matches("0x"))?;
    let summary = tls::parse_handshake(&bytes)?;
    let prims = tls::primitives_from_summary(&summary);
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "summary": summary,
        "primitives": prims,
    }))?);
    Ok(())
}

fn read_input(path: &str) -> anyhow::Result<String> {
    if path == "-" {
        let mut buf = String::new();
        let stdin = std::io::stdin();
        let mut handle = stdin.lock();
        let mut tmp = String::new();
        while handle.read_line(&mut tmp)? > 0 {
            buf.push_str(&tmp);
            tmp.clear();
        }
        Ok(buf)
    } else {
        Ok(std::fs::read_to_string(PathBuf::from(path))?)
    }
}

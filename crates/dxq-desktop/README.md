# dxq-desktop

A native, pure-Rust desktop monitor for the dxQ collector — built on
[egui/eframe](https://github.com/emilk/egui). It polls the collector's
read-only REST API and renders the live crypto-posture:

- **Posture** — org-level quantum-readiness score `q`, asset count,
  BLOCKED count, and the migration deadline.
- **Inventory** — every observed asset with its `q`, host, PQ
  primitives, and BLOCKED flag.
- **BLOCKED** — the assets whose crypto can't be rotated in place
  (agility `Locked`/`Frozen`).
- **Trend** — `org_q` over time with a BLOCKED-count overlay.
- **Recommendations** — per-asset PQ migration advice
  (`replaces → replacement`, cost class, rationale, caveats).
- **Compat matrix** — TLS-stack × algorithm support grid, colour-coded
  by status.

It is an **observer**: it only reads (`/v1/posture`, `/v1/inventory`,
`/v1/blocked`, `/v1/posture/history`, `/v1/recommendations`,
`/v1/agility/compat`). It never ingests or mutates.

## Build & run

```bash
cd crates/dxq-desktop
cargo run -- --collector http://127.0.0.1:8090
# or point it anywhere:
DXQ_COLLECTOR=https://dxq.internal:8443 cargo run
```

Default collector: `http://127.0.0.1:8090`. HTTPS collectors with a
publicly-trusted server certificate work out of the box (rustls +
webpki roots, `ring` provider).

### mTLS collectors

The collector's main `/v1` surface sits behind an mTLS listener. Point
the client at a per-agent certificate (signed by the collector's
internal CA) plus the CA cert to verify the server:

```bash
cargo run -- \
  --collector https://dxq.internal:8443 \
  --client-cert /etc/dxq/agent.crt \
  --client-key  /etc/dxq/agent.key \
  --ca-cert     /etc/dxq/ca.crt
# env equivalents: DXQ_CLIENT_CERT / DXQ_CLIENT_KEY / DXQ_CA_CERT
```

The header shows an **mTLS** badge when a client identity is in use.

## Why it is its own workspace

This crate is intentionally **not** a member of the parent Cargo
workspace. The egui/eframe/winit/glow stack needs X11/Wayland + GL
development libraries to link and a display to run, neither of which a
headless CI runner has. Keeping it a separate workspace lets
`cargo build --workspace` and CI stay clean; build the desktop client
explicitly from this directory.

On Debian/Ubuntu the link-time libraries are:

```bash
sudo apt-get install -y \
    libgl1-mesa-dev libx11-dev libxkbcommon-dev libwayland-dev
```

The networking and data layers are unit-tested (`cargo test`); the
GUI itself requires a display to run.

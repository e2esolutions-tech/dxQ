# ree0xQ Dashboard

Minimal Vite + React + TypeScript + Tailwind dashboard against the
`ree0xq-server` REST API.

## Pages

| Route          | Endpoint           | Purpose                                                                              |
|----------------|--------------------|--------------------------------------------------------------------------------------|
| `/posture`     | `GET /v1/posture`  | Org-level $q$, deadline countdown, total / BLOCKED counts.                           |
| `/inventory`   | `GET /v1/inventory`| Per-asset latest event sorted by $q$ descending; filter by kind and BLOCKED flag.   |
| `/blocked`     | `GET /v1/blocked`  | Just the BLOCKED assets — agility $\le$ Locked.                                      |
| `/qkd`         | `GET /v1/qkd/links`| ETSI 014 KME health, QBER, key rate.                                                 |

## Development

```bash
cd web
npm install
npm run dev
```

Vite dev-server runs on http://127.0.0.1:5173/ and proxies `/v1/*` to
`ree0xq-server` on http://127.0.0.1:8090/.

## Production build

```bash
npm run build
# Output: dist/
```

`dist/` is a static bundle; in production we expect `ree0xq-server` to
serve it (the binary picks up a `--web-dir` flag in a later milestone).

## TypeScript schema sync

The wire types in [`src/types/ree0xq.ts`](./src/types/ree0xq.ts) are
hand-mirrored from `ree0xq-core`'s `CryptoInventoryEvent` v1.1 schema.
When `ts-rs` integration lands (the `ts-types` feature on
`ree0xq-core`), this file will be auto-generated. Until then, any
change to the Rust schema requires a corresponding edit here — the
v1.1 invariant (additive only) means the file rarely needs to change
in practice.

## End-to-end demo

See [`../scripts/demo.sh`](../scripts/demo.sh) — boots the KME
emulator, the ree0xq-qkd collector, ree0xq-server, runs ree0xq-net once
over a small zgrab fixture, and prints the dashboard URL.

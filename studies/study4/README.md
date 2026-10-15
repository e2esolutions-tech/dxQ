# Study 4 — Longitudinal PQ-adoption measurement (in progress)

Turns Study 1's single snapshot (Tranco 6G8PX, 2026-05-13,
43.8% hybrid PQ-KEX) into a time series, and adds the SSH
surface via `ree0xq-net ssh` (SEZ-30).

## Method

- Quarterly rescan of the current Tranco top-1k with the same
  probe pair as Study 1 (classical baseline + PQ-capable probe).
- Each epoch stored under `epochs/YYYY-QQ/` (raw NDJSON + summary).
- Per-host diffing against the frozen Study 1 baseline:
  `classical->hybrid`, `hybrid->classical` (regressions), hosts
  dropped from / new to the list are tracked separately.
- SSH: top-1k hosts with tcp/22 open, banner + KEXINIT offer
  recorded (no authentication attempted; connection closed after
  the first server KEXINIT).

## Outputs

- `timeseries.csv` — epoch, n_ok, hybrid_count, hybrid_pct (TLS),
  ssh_n, ssh_pq_count.
- `plots/adoption-curve.pdf`.

## Status

Scaffolding only — first epoch lands with SEZ-31. The Study 1
baseline under `../study1/` is frozen and never rewritten.

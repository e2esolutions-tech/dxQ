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

## Tooling (SEZ-31)

- `rescan.sh <epoch-label> <hosts.txt>` — run one epoch: PQ-probe
  the host list with `ree0xq-net pq-probe`, store raw NDJSON under
  `epochs/<label>/`, diff against the frozen 6G8PX baseline, write
  `summary.json`.
- `diff_epoch.py <epoch.ndjson>` — per-host transitions vs the
  baseline (adopted / regressed / stable), on hosts responsive in
  BOTH snapshots so the rate has a stable denominator.
- `build_timeseries.py` — merge the baseline (epoch `2026-Q2`) and
  every `epochs/*/summary.json` into `timeseries.csv`.
- `plot_curve.py` — render `plots/adoption-curve.{png,pdf}`;
  degrades to a single point when only the baseline exists.

The baseline epoch is Study 1 itself (Tranco 6G8PX, 2026-05-13,
43.8% hybrid PQ-KEX over 724 responsive hosts). `diff_epoch.py`
run against the baseline reports zero transitions — the rig's
self-consistency check.

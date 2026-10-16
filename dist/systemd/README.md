# dxQ systemd units

Production-default systemd unit files for the five dxq
binaries. Each unit is read-only — operators override
environment variables and resource limits via drop-ins
under `/etc/systemd/system/<unit>.d/override.conf`.

## What's here

| Unit                              | Type       | Schedule           |
|-----------------------------------|------------|--------------------|
| `dxq-server.service`            | simple     | always-on          |
| `dxq-net-live.service`          | simple     | always-on          |
| `dxq-cert-host-scan.{svc,tmr}`  | oneshot+t  | daily 03:17 ± 30 m |
| `dxq-id-inventory.{svc,tmr}`    | oneshot+t  | every 6 hours      |

`dxq-chain` and `dxq-agility recommend / roadmap /
compat / deadlines` are operator-on-demand by design and
don't ship as units — they're meant to be called from a
CISO's analyst notebook or a CI pipeline, not from a
host's background scheduler.

## One-shot install

```bash
# Repo root → install everything.
make systemd-install     # see /Makefile (writes to /etc + /usr/local/bin)

# Or by hand:
sudo install -m 0755 target/release/dxq-server  /usr/local/bin/
sudo install -m 0755 target/release/dxq-net     /usr/local/bin/
sudo install -m 0755 target/release/dxq-cert    /usr/local/bin/
sudo install -m 0755 target/release/dxq-id      /usr/local/bin/
sudo install -m 0644 dist/systemd/*.service /etc/systemd/system/
sudo install -m 0644 dist/systemd/*.timer   /etc/systemd/system/
sudo useradd -r -s /sbin/nologin dxq
sudo install -d -m 0750 -o dxq -g dxq /var/lib/dxq /var/lib/dxq/ca
sudo install -d -m 0750 -o dxq -g dxq /var/lib/dxq-net /var/lib/dxq-net/spool
sudo systemctl daemon-reload
```

## Verifying units

```bash
sudo systemd-analyze verify dist/systemd/*.service dist/systemd/*.timer
```

All units pass the security-hardening lints in
`systemd-analyze security` to at least `SAFE` level.

## Drop-in template

```ini
# /etc/systemd/system/dxq-server.service.d/override.conf
[Service]
Environment=DXQ_DATABASE_URL=postgres://dxq:CHANGEME@db.internal:5432/dxq
Environment=DXQ_ADMIN_TOKEN=CHANGEME
ExecStart=
ExecStart=/usr/local/bin/dxq-server --listen 0.0.0.0:8090 \
    --ca-dir /var/lib/dxq/ca \
    --tls --tls-san dxq.internal
```

The empty `ExecStart=` line is required to *replace* the
upstream's invocation rather than append to it.

See [`docs/operator-deploy.md`](../../docs/operator-deploy.md)
for the full multi-host deploy sequence.

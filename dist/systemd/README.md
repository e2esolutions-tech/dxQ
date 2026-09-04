# ree0xQ systemd units

Production-default systemd unit files for the five ree0xq
binaries. Each unit is read-only — operators override
environment variables and resource limits via drop-ins
under `/etc/systemd/system/<unit>.d/override.conf`.

## What's here

| Unit                              | Type       | Schedule           |
|-----------------------------------|------------|--------------------|
| `ree0xq-server.service`            | simple     | always-on          |
| `ree0xq-net-live.service`          | simple     | always-on          |
| `ree0xq-cert-host-scan.{svc,tmr}`  | oneshot+t  | daily 03:17 ± 30 m |
| `ree0xq-id-inventory.{svc,tmr}`    | oneshot+t  | every 6 hours      |

`ree0xq-chain` and `ree0xq-agility recommend / roadmap /
compat / deadlines` are operator-on-demand by design and
don't ship as units — they're meant to be called from a
CISO's analyst notebook or a CI pipeline, not from a
host's background scheduler.

## One-shot install

```bash
# Repo root → install everything.
make systemd-install     # see /Makefile (writes to /etc + /usr/local/bin)

# Or by hand:
sudo install -m 0755 target/release/ree0xq-server  /usr/local/bin/
sudo install -m 0755 target/release/ree0xq-net     /usr/local/bin/
sudo install -m 0755 target/release/ree0xq-cert    /usr/local/bin/
sudo install -m 0755 target/release/ree0xq-id      /usr/local/bin/
sudo install -m 0644 dist/systemd/*.service /etc/systemd/system/
sudo install -m 0644 dist/systemd/*.timer   /etc/systemd/system/
sudo useradd -r -s /sbin/nologin ree0xq
sudo install -d -m 0750 -o ree0xq -g ree0xq /var/lib/ree0xq /var/lib/ree0xq/ca
sudo install -d -m 0750 -o ree0xq -g ree0xq /var/lib/ree0xq-net /var/lib/ree0xq-net/spool
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
# /etc/systemd/system/ree0xq-server.service.d/override.conf
[Service]
Environment=REE0XQ_DATABASE_URL=postgres://ree0xq:CHANGEME@db.internal:5432/ree0xq
Environment=REE0XQ_ADMIN_TOKEN=CHANGEME
ExecStart=
ExecStart=/usr/local/bin/ree0xq-server --listen 0.0.0.0:8090 \
    --ca-dir /var/lib/ree0xq/ca \
    --tls --tls-san ree0xq.internal
```

The empty `ExecStart=` line is required to *replace* the
upstream's invocation rather than append to it.

See [`docs/operator-deploy.md`](../../docs/operator-deploy.md)
for the full multi-host deploy sequence.

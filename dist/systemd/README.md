# Sezar systemd units

Production-default systemd unit files for the five sezar
binaries. Each unit is read-only — operators override
environment variables and resource limits via drop-ins
under `/etc/systemd/system/<unit>.d/override.conf`.

## What's here

| Unit                              | Type       | Schedule           |
|-----------------------------------|------------|--------------------|
| `sezar-server.service`            | simple     | always-on          |
| `sezar-net-live.service`          | simple     | always-on          |
| `sezar-cert-host-scan.{svc,tmr}`  | oneshot+t  | daily 03:17 ± 30 m |
| `sezar-id-inventory.{svc,tmr}`    | oneshot+t  | every 6 hours      |

`sezar-chain` and `sezar-agility recommend / roadmap /
compat / deadlines` are operator-on-demand by design and
don't ship as units — they're meant to be called from a
CISO's analyst notebook or a CI pipeline, not from a
host's background scheduler.

## One-shot install

```bash
# Repo root → install everything.
make systemd-install     # see /Makefile (writes to /etc + /usr/local/bin)

# Or by hand:
sudo install -m 0755 target/release/sezar-server  /usr/local/bin/
sudo install -m 0755 target/release/sezar-net     /usr/local/bin/
sudo install -m 0755 target/release/sezar-cert    /usr/local/bin/
sudo install -m 0755 target/release/sezar-id      /usr/local/bin/
sudo install -m 0644 dist/systemd/*.service /etc/systemd/system/
sudo install -m 0644 dist/systemd/*.timer   /etc/systemd/system/
sudo useradd -r -s /sbin/nologin sezar
sudo install -d -m 0750 -o sezar -g sezar /var/lib/sezar /var/lib/sezar/ca
sudo install -d -m 0750 -o sezar -g sezar /var/lib/sezar-net /var/lib/sezar-net/spool
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
# /etc/systemd/system/sezar-server.service.d/override.conf
[Service]
Environment=SEZAR_DATABASE_URL=postgres://sezar:CHANGEME@db.internal:5432/sezar
Environment=SEZAR_ADMIN_TOKEN=CHANGEME
ExecStart=
ExecStart=/usr/local/bin/sezar-server --listen 0.0.0.0:8090 \
    --ca-dir /var/lib/sezar/ca \
    --tls --tls-san sezar.internal
```

The empty `ExecStart=` line is required to *replace* the
upstream's invocation rather than append to it.

See [`docs/operator-deploy.md`](../../docs/operator-deploy.md)
for the full multi-host deploy sequence.

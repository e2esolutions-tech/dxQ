# ree0xQ — multi-host operator deployment

Stand up ree0xQ across a fleet. Three host roles:

- **collector host** — runs `ree0xq-server` + Postgres.
  One per region or one per tenant; typically the only
  host that's externally reachable.
- **agent hosts** — run `ree0xq-net` (always-on network
  observer) + `ree0xq-cert` + `ree0xq-id` (timer-driven
  inventory scans). Many; one per system you care to
  measure.
- **analyst workstations** — run `ree0xq-chain` /
  `ree0xq-agility recommend / roadmap / compat /
  deadlines` interactively. Typically the CISO's laptop;
  one per analyst.

Topology:

```
                 ┌─────────────────────┐
                 │  collector host     │
                 │                     │
                 │  ree0xq-server  ─── Postgres
                 │      ▲              │
                 └──────┼──────────────┘
                        │ mTLS (8090) + bootstrap (8443)
        ┌───────────────┼──────────────────┐
        │               │                  │
   ┌────┴───────┐  ┌────┴───────┐  ┌──────┴─────────┐
   │ agent host │  │ agent host │  │ agent host …   │
   │ ree0xq-net  │  │ ree0xq-net  │  │ ree0xq-net      │
   │ ree0xq-cert │  │ ree0xq-cert │  │ ree0xq-cert     │
   │ ree0xq-id   │  │ ree0xq-id   │  │ ree0xq-id       │
   └────────────┘  └────────────┘  └────────────────┘
```

The `analyst-workstation` role is omitted from the
diagram because it only consumes the collector REST API —
no agent install, no inbound connections.

## 0. Prerequisites

| Where           | What                                                                   |
|-----------------|------------------------------------------------------------------------|
| Collector host  | Linux ≥ kernel 5.8 (matches ree0xq-net Phase 2.2 require), Postgres 15+, network reach from every agent. |
| Agent hosts     | Linux, `libpcap` runtime, kernel ≥ 5.8 for Phase 2.2 (Phase 2.0 pcap-file works on older). |
| Analyst host    | Standard Linux + `cargo`-built binaries.                               |

## 1. Bring up the collector

```bash
# As root on the collector host.
useradd -r -s /sbin/nologin ree0xq
install -d -m 0750 -o ree0xq -g ree0xq /var/lib/ree0xq /var/lib/ree0xq/ca

# Install the binary + systemd unit.
install -m 0755 target/release/ree0xq-server  /usr/local/bin/
install -m 0644 dist/systemd/ree0xq-server.service /etc/systemd/system/

# Configure DB + admin token.
mkdir -p /etc/systemd/system/ree0xq-server.service.d
cat > /etc/systemd/system/ree0xq-server.service.d/override.conf <<EOF
[Service]
Environment=REE0XQ_DATABASE_URL=postgres://ree0xq:CHANGEME@127.0.0.1:5432/ree0xq
Environment=REE0XQ_ADMIN_TOKEN=$(openssl rand -hex 32)
ExecStart=
ExecStart=/usr/local/bin/ree0xq-server --listen 0.0.0.0:8090 \
    --ca-dir /var/lib/ree0xq/ca \
    --tls --tls-bootstrap-listen 0.0.0.0:8443 \
    --tls-san ree0xq.internal
EOF

systemctl daemon-reload
systemctl enable --now ree0xq-server

# Confirm.
curl -fsS --cacert /var/lib/ree0xq/ca/ca.crt https://ree0xq.internal:8443/healthz
```

The collector boots with a CA-signed server cert; agents
will pin against the CA at enrolment time.

## 2. Mint a bootstrap token per agent

On the collector host, issue a one-time token per agent
identity:

```bash
# Save REE0XQ_ADMIN_TOKEN from the override above somewhere
# safe; you'll need it for every token mint.
ADMIN=$(systemctl show -p Environment ree0xq-server | tr ' ' '\n' | grep REE0XQ_ADMIN_TOKEN | cut -d= -f2)

curl -sS --cacert /var/lib/ree0xq/ca/ca.crt \
    https://ree0xq.internal:8443/v1/admin/bootstrap-tokens \
    -H "X-Admin-Token: $ADMIN" \
    -d '{"agent_id":"agent-host-01"}'
# → {"token":"<uuid>","agent_id":"agent-host-01","expires_at":"..."}
```

Each token is single-use and TTL-bound (default 24 h).
Hand the token to the agent host over an authenticated
channel (SSH session, Ansible vault, …) — the token
itself is the bootstrap secret.

## 3. Bring up an agent host

```bash
# As root on the agent.
useradd -r -s /sbin/nologin ree0xq
install -d -m 0750 -o ree0xq -g ree0xq /var/lib/ree0xq-net /var/lib/ree0xq-net/spool /etc/ree0xq

install -m 0755 target/release/ree0xq-net  /usr/local/bin/
install -m 0755 target/release/ree0xq-cert /usr/local/bin/
install -m 0755 target/release/ree0xq-id   /usr/local/bin/
install -m 0644 dist/systemd/*.service /etc/systemd/system/
install -m 0644 dist/systemd/*.timer   /etc/systemd/system/

# Trust the collector's CA.
scp collector:/var/lib/ree0xq/ca/ca.crt /etc/ree0xq/ca.crt

# Redeem the bootstrap token for a per-agent cert.
TOKEN='<paste from step 2>'
curl -sS --cacert /etc/ree0xq/ca.crt \
    https://ree0xq.internal:8443/v1/enrol \
    -H "X-Bootstrap-Token: $TOKEN" \
    -d '{"agent_id":"agent-host-01"}' \
    | tee /etc/ree0xq/enrol.json
jq -r .cert_pem  < /etc/ree0xq/enrol.json > /etc/ree0xq/agent.crt
jq -r .key_pem   < /etc/ree0xq/enrol.json > /etc/ree0xq/agent.key
chmod 0640 /etc/ree0xq/agent.{crt,key}
chown root:ree0xq /etc/ree0xq/agent.{crt,key}

# Configure the live-capture unit.
mkdir -p /etc/systemd/system/ree0xq-net-live.service.d
cat > /etc/systemd/system/ree0xq-net-live.service.d/override.conf <<EOF
[Service]
Environment=REE0XQ_NET_IFACE=eth0
Environment=REE0XQ_NET_COLLECTOR=https://ree0xq.internal:8090/v1/events
Environment=REE0XQ_NET_SPOOL=/var/lib/ree0xq-net/spool
EOF

# Configure the cert host-scan timer.
mkdir -p /etc/systemd/system/ree0xq-cert-host-scan.service.d
cat > /etc/systemd/system/ree0xq-cert-host-scan.service.d/override.conf <<EOF
[Service]
Environment=REE0XQ_CERT_COLLECTOR=https://ree0xq.internal:8090/v1/events
EOF

# Configure the HSM inventory timer (optional; only on
# hosts with HSMs the operator catalogued).
if [ -f /etc/ree0xq/hsm-inventory.json ]; then
    mkdir -p /etc/systemd/system/ree0xq-id-inventory.service.d
    cat > /etc/systemd/system/ree0xq-id-inventory.service.d/override.conf <<EOF
[Service]
Environment=REE0XQ_ID_INVENTORY=/etc/ree0xq/hsm-inventory.json
Environment=REE0XQ_ID_COLLECTOR=https://ree0xq.internal:8090/v1/events
EOF
fi

systemctl daemon-reload
systemctl enable --now ree0xq-net-live
systemctl enable --now ree0xq-cert-host-scan.timer
[ -f /etc/ree0xq/hsm-inventory.json ] && \
    systemctl enable --now ree0xq-id-inventory.timer

# Confirm — should show the agent's first events
# arriving within a few seconds of any TLS traffic.
curl -sS --cacert /etc/ree0xq/ca.crt \
    --cert /etc/ree0xq/agent.crt --key /etc/ree0xq/agent.key \
    https://ree0xq.internal:8090/v1/inventory | jq '.count'
```

## 4. Analyst-workstation usage

No daemons, no install — just the binaries:

```bash
# Pull the V5 recommendations for the whole fleet.
ree0xq-agility recommend --inventory https://ree0xq.internal:8090/v1/inventory

# Project an org_q trajectory for a candidate plan.
ree0xq-agility roadmap --inventory https://ree0xq.internal:8090/v1/inventory \
    --plan migration-q1-2027.json

# Check whether OpenSSL-3.x ships ML-DSA-65.
ree0xq-agility compat --stack openssl-3.x --algo ML-DSA-65

# What PQ deadlines hit our jurisdiction in the next 12 months?
ree0xq-agility deadlines --jurisdiction US --horizon-days 365
```

## 5. Day-2 operations

- **Cert rotation.** Re-mint a bootstrap token on the
  collector, run step 3's enrolment block on the agent,
  reload `ree0xq-net-live`. The new cert is in place
  before the old one expires.
- **Adding an agent.** Steps 2–3, no collector change.
- **Removing an agent.** Stop and disable the unit on the
  agent; the per-asset events stop arriving and roll out
  of the latest-per-asset map naturally as the cert
  expires.
- **Cycling the CA.** Out of scope for V1. Plan: bring up
  a second `ree0xq-server` on a new CA, enrol all agents
  against it, decommission the old one.

## 6. Verifying the hardening

```bash
# Each unit should pass systemd-analyze security with
# at least a SAFE rating.
systemd-analyze security ree0xq-server.service
systemd-analyze security ree0xq-net-live.service
```

The shipped units drop every capability except
`CAP_NET_RAW` (ree0xq-net-live), use the `ree0xq` system
user, restrict the writable filesystem to
`/var/lib/ree0xq*`, and filter syscalls to
`@system-service` minus `@privileged @resources`.
Operators tightening further can layer
`ReadOnlyPaths=/etc/ree0xq/agent.key` and similar in the
drop-in.

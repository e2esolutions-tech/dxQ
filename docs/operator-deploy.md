# Sezar — multi-host operator deployment

Stand up Sezar across a fleet. Three host roles:

- **collector host** — runs `sezar-server` + Postgres.
  One per region or one per tenant; typically the only
  host that's externally reachable.
- **agent hosts** — run `sezar-net` (always-on network
  observer) + `sezar-cert` + `sezar-id` (timer-driven
  inventory scans). Many; one per system you care to
  measure.
- **analyst workstations** — run `sezar-chain` /
  `sezar-agility recommend / roadmap / compat /
  deadlines` interactively. Typically the CISO's laptop;
  one per analyst.

Topology:

```
                 ┌─────────────────────┐
                 │  collector host     │
                 │                     │
                 │  sezar-server  ─── Postgres
                 │      ▲              │
                 └──────┼──────────────┘
                        │ mTLS (8090) + bootstrap (8443)
        ┌───────────────┼──────────────────┐
        │               │                  │
   ┌────┴───────┐  ┌────┴───────┐  ┌──────┴─────────┐
   │ agent host │  │ agent host │  │ agent host …   │
   │ sezar-net  │  │ sezar-net  │  │ sezar-net      │
   │ sezar-cert │  │ sezar-cert │  │ sezar-cert     │
   │ sezar-id   │  │ sezar-id   │  │ sezar-id       │
   └────────────┘  └────────────┘  └────────────────┘
```

The `analyst-workstation` role is omitted from the
diagram because it only consumes the collector REST API —
no agent install, no inbound connections.

## 0. Prerequisites

| Where           | What                                                                   |
|-----------------|------------------------------------------------------------------------|
| Collector host  | Linux ≥ kernel 5.8 (matches sezar-net Phase 2.2 require), Postgres 15+, network reach from every agent. |
| Agent hosts     | Linux, `libpcap` runtime, kernel ≥ 5.8 for Phase 2.2 (Phase 2.0 pcap-file works on older). |
| Analyst host    | Standard Linux + `cargo`-built binaries.                               |

## 1. Bring up the collector

```bash
# As root on the collector host.
useradd -r -s /sbin/nologin sezar
install -d -m 0750 -o sezar -g sezar /var/lib/sezar /var/lib/sezar/ca

# Install the binary + systemd unit.
install -m 0755 target/release/sezar-server  /usr/local/bin/
install -m 0644 dist/systemd/sezar-server.service /etc/systemd/system/

# Configure DB + admin token.
mkdir -p /etc/systemd/system/sezar-server.service.d
cat > /etc/systemd/system/sezar-server.service.d/override.conf <<EOF
[Service]
Environment=SEZAR_DATABASE_URL=postgres://sezar:CHANGEME@127.0.0.1:5432/sezar
Environment=SEZAR_ADMIN_TOKEN=$(openssl rand -hex 32)
ExecStart=
ExecStart=/usr/local/bin/sezar-server --listen 0.0.0.0:8090 \
    --ca-dir /var/lib/sezar/ca \
    --tls --tls-bootstrap-listen 0.0.0.0:8443 \
    --tls-san sezar.internal
EOF

systemctl daemon-reload
systemctl enable --now sezar-server

# Confirm.
curl -fsS --cacert /var/lib/sezar/ca/ca.crt https://sezar.internal:8443/healthz
```

The collector boots with a CA-signed server cert; agents
will pin against the CA at enrolment time.

## 2. Mint a bootstrap token per agent

On the collector host, issue a one-time token per agent
identity:

```bash
# Save SEZAR_ADMIN_TOKEN from the override above somewhere
# safe; you'll need it for every token mint.
ADMIN=$(systemctl show -p Environment sezar-server | tr ' ' '\n' | grep SEZAR_ADMIN_TOKEN | cut -d= -f2)

curl -sS --cacert /var/lib/sezar/ca/ca.crt \
    https://sezar.internal:8443/v1/admin/bootstrap-tokens \
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
useradd -r -s /sbin/nologin sezar
install -d -m 0750 -o sezar -g sezar /var/lib/sezar-net /var/lib/sezar-net/spool /etc/sezar

install -m 0755 target/release/sezar-net  /usr/local/bin/
install -m 0755 target/release/sezar-cert /usr/local/bin/
install -m 0755 target/release/sezar-id   /usr/local/bin/
install -m 0644 dist/systemd/*.service /etc/systemd/system/
install -m 0644 dist/systemd/*.timer   /etc/systemd/system/

# Trust the collector's CA.
scp collector:/var/lib/sezar/ca/ca.crt /etc/sezar/ca.crt

# Redeem the bootstrap token for a per-agent cert.
TOKEN='<paste from step 2>'
curl -sS --cacert /etc/sezar/ca.crt \
    https://sezar.internal:8443/v1/enrol \
    -H "X-Bootstrap-Token: $TOKEN" \
    -d '{"agent_id":"agent-host-01"}' \
    | tee /etc/sezar/enrol.json
jq -r .cert_pem  < /etc/sezar/enrol.json > /etc/sezar/agent.crt
jq -r .key_pem   < /etc/sezar/enrol.json > /etc/sezar/agent.key
chmod 0640 /etc/sezar/agent.{crt,key}
chown root:sezar /etc/sezar/agent.{crt,key}

# Configure the live-capture unit.
mkdir -p /etc/systemd/system/sezar-net-live.service.d
cat > /etc/systemd/system/sezar-net-live.service.d/override.conf <<EOF
[Service]
Environment=SEZAR_NET_IFACE=eth0
Environment=SEZAR_NET_COLLECTOR=https://sezar.internal:8090/v1/events
Environment=SEZAR_NET_SPOOL=/var/lib/sezar-net/spool
EOF

# Configure the cert host-scan timer.
mkdir -p /etc/systemd/system/sezar-cert-host-scan.service.d
cat > /etc/systemd/system/sezar-cert-host-scan.service.d/override.conf <<EOF
[Service]
Environment=SEZAR_CERT_COLLECTOR=https://sezar.internal:8090/v1/events
EOF

# Configure the HSM inventory timer (optional; only on
# hosts with HSMs the operator catalogued).
if [ -f /etc/sezar/hsm-inventory.json ]; then
    mkdir -p /etc/systemd/system/sezar-id-inventory.service.d
    cat > /etc/systemd/system/sezar-id-inventory.service.d/override.conf <<EOF
[Service]
Environment=SEZAR_ID_INVENTORY=/etc/sezar/hsm-inventory.json
Environment=SEZAR_ID_COLLECTOR=https://sezar.internal:8090/v1/events
EOF
fi

systemctl daemon-reload
systemctl enable --now sezar-net-live
systemctl enable --now sezar-cert-host-scan.timer
[ -f /etc/sezar/hsm-inventory.json ] && \
    systemctl enable --now sezar-id-inventory.timer

# Confirm — should show the agent's first events
# arriving within a few seconds of any TLS traffic.
curl -sS --cacert /etc/sezar/ca.crt \
    --cert /etc/sezar/agent.crt --key /etc/sezar/agent.key \
    https://sezar.internal:8090/v1/inventory | jq '.count'
```

## 4. Analyst-workstation usage

No daemons, no install — just the binaries:

```bash
# Pull the V5 recommendations for the whole fleet.
sezar-agility recommend --inventory https://sezar.internal:8090/v1/inventory

# Project an org_q trajectory for a candidate plan.
sezar-agility roadmap --inventory https://sezar.internal:8090/v1/inventory \
    --plan migration-q1-2027.json

# Check whether OpenSSL-3.x ships ML-DSA-65.
sezar-agility compat --stack openssl-3.x --algo ML-DSA-65

# What PQ deadlines hit our jurisdiction in the next 12 months?
sezar-agility deadlines --jurisdiction US --horizon-days 365
```

## 5. Day-2 operations

- **Cert rotation.** Re-mint a bootstrap token on the
  collector, run step 3's enrolment block on the agent,
  reload `sezar-net-live`. The new cert is in place
  before the old one expires.
- **Adding an agent.** Steps 2–3, no collector change.
- **Removing an agent.** Stop and disable the unit on the
  agent; the per-asset events stop arriving and roll out
  of the latest-per-asset map naturally as the cert
  expires.
- **Cycling the CA.** Out of scope for V1. Plan: bring up
  a second `sezar-server` on a new CA, enrol all agents
  against it, decommission the old one.

## 6. Verifying the hardening

```bash
# Each unit should pass systemd-analyze security with
# at least a SAFE rating.
systemd-analyze security sezar-server.service
systemd-analyze security sezar-net-live.service
```

The shipped units drop every capability except
`CAP_NET_RAW` (sezar-net-live), use the `sezar` system
user, restrict the writable filesystem to
`/var/lib/sezar*`, and filter syscalls to
`@system-service` minus `@privileged @resources`.
Operators tightening further can layer
`ReadOnlyPaths=/etc/sezar/agent.key` and similar in the
drop-in.

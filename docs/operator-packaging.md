# ree0xQ — Packaging (`.deb` + `.rpm`)

Every shipping ree0xQ binary ships as both a Debian `.deb`
and a Red-Hat `.rpm`. The intent: drop the right package on
an operator's host, `systemctl enable --now` the matching
unit, and walk away.

## Build host requirements

```bash
cargo install cargo-deb
cargo install cargo-generate-rpm
```

That's it — no Docker, no `fpm`, no Ruby. `cargo-deb` runs
on any Linux (the `dpkg-shlibdeps` warning on Fedora is
benign; the resulting `.deb` still installs cleanly on
Debian/Ubuntu). `cargo-generate-rpm` is pure Rust and
needs neither `rpm` nor `rpmbuild` on the build host.

## What lands

| Crate          | Binary           | Systemd unit              | Notes |
|----------------|------------------|---------------------------|-------|
| `ree0xq-server` | `ree0xq-server`   | `ree0xq-server.service`    | Long-running collector + REST API + dashboard backend |
| `ree0xq-net`    | `ree0xq-net`      | `ree0xq-net-live.service`  | Long-running passive TLS observer |
| `ree0xq-cert`   | `ree0xq-cert`     | `ree0xq-cert-host-scan.{service,timer}` | Periodic host filesystem cert scan |
| `ree0xq-id`     | `ree0xq-id`       | `ree0xq-id-inventory.{service,timer}`   | Periodic HSM / KMS / smart-card inventory |
| `ree0xq-chain`  | `ree0xq-chain`    | (none — ad-hoc CLI)        | Blockchain address-inventory scanner |
| `ree0xq-agility`| `ree0xq-agility`  | (none — ad-hoc CLI)        | V5 PQ-migration recommendations CLI |

Every binary lands under `/usr/local/bin/`. Units land
under `/usr/lib/systemd/system/` on RPM hosts and
`/lib/systemd/system/` on `.deb` hosts (Debian / Ubuntu
convention). Neither package enables the units — operators
do that explicitly after dropping in any required env
vars.

## Build commands

One-shot — every crate, both formats:

```bash
make packages                  # release + .deb + .rpm
```

Per-format:

```bash
make packages-deb              # cargo build --release first, then cargo deb -p <each>
make packages-rpm              # cargo build --release first, then cargo generate-rpm -p crates/<each>
```

Per-crate (if you're iterating on one):

```bash
cargo build --release -p ree0xq-server
cargo deb           -p ree0xq-server --no-build
cargo generate-rpm  -p crates/ree0xq-server
```

Output paths:

- `target/debian/<crate>_<version>-1_amd64.deb`
- `target/generate-rpm/<crate>-<version>-1.x86_64.rpm`

Both directories are git-ignored via the top-level
`target/` exclusion.

## Versioning

The workspace uses `0.1.0-dev` while V1 is in flight. Deb
accepts that string verbatim; RPM rejects `-` in versions
so `[package.metadata.generate-rpm].version` is set to
`0.1.0~dev` per crate. The `~` ranks `0.1.0~dev < 0.1.0`
in RPM's comparator — when V1 cuts to `0.1.0`, an in-place
upgrade is correct.

## Install — Debian / Ubuntu

```bash
sudo apt install ./ree0xq-server_0.1.0-dev-1_amd64.deb \
                 ./ree0xq-net_0.1.0-dev-1_amd64.deb \
                 ./ree0xq-cert_0.1.0-dev-1_amd64.deb \
                 ./ree0xq-id_0.1.0-dev-1_amd64.deb

# State dirs (ree0xq-server)
sudo useradd -r -s /sbin/nologin ree0xq
sudo install -d -m 0750 -o ree0xq -g ree0xq /var/lib/ree0xq /var/lib/ree0xq/ca

# Drop in env (collector URL, admin token, db url)
sudo systemctl edit ree0xq-server

# Bring it up
sudo systemctl enable --now ree0xq-server
```

## Install — Fedora / RHEL / Rocky

```bash
sudo dnf install ./ree0xq-server-0.1.0~dev-1.x86_64.rpm \
                 ./ree0xq-net-0.1.0~dev-1.x86_64.rpm \
                 ./ree0xq-cert-0.1.0~dev-1.x86_64.rpm \
                 ./ree0xq-id-0.1.0~dev-1.x86_64.rpm

# State dirs and daemon-reload are handled by the package's
# %post script.

sudo systemctl edit ree0xq-server      # drop in env
sudo systemctl enable --now ree0xq-server
```

## Uninstall

Both `.deb` and `.rpm` pre-uninstall scripts stop and
disable the matching units. State directories
(`/var/lib/ree0xq*`) and the `ree0xq` system user are left
alone — clean them up manually with:

```bash
sudo rm -rf /var/lib/ree0xq /var/lib/ree0xq-net
sudo userdel ree0xq
```

## Sanity-check a built package

```bash
# RPM
rpm -qpi target/generate-rpm/ree0xq-server-0.1.0~dev-1.x86_64.rpm
rpm -qpl target/generate-rpm/ree0xq-server-0.1.0~dev-1.x86_64.rpm

# Deb (needs dpkg on the inspecting host)
dpkg -I target/debian/ree0xq-server_0.1.0-dev-1_amd64.deb
dpkg -c target/debian/ree0xq-server_0.1.0-dev-1_amd64.deb
```

## Known limitations

- `cargo deb` runs `dpkg-shlibdeps` for automatic library-
  dependency resolution. On non-Debian build hosts that
  binary is absent and the dependencies list is left as
  `$auto`; the `.deb` still installs because
  `glibc`/`openssl` are satisfied on every Debian release.
  Build the `.deb` on a Debian/Ubuntu CI runner if you need
  the exact shared-library deps resolved.

- The `ree0xq-net-ebpf` crate's BPF object is not packaged
  yet — it requires a specific nightly toolchain and the
  packaging pipeline keeps the stable toolchain dependency.
  Operators who need the eBPF live mode build the BPF
  object out-of-band per `crates/ree0xq-net-ebpf/README.md`.

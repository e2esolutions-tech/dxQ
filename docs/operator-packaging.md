# Sezar — Packaging (`.deb` + `.rpm`)

Every shipping Sezar binary ships as both a Debian `.deb`
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
| `sezar-server` | `sezar-server`   | `sezar-server.service`    | Long-running collector + REST API + dashboard backend |
| `sezar-net`    | `sezar-net`      | `sezar-net-live.service`  | Long-running passive TLS observer |
| `sezar-cert`   | `sezar-cert`     | `sezar-cert-host-scan.{service,timer}` | Periodic host filesystem cert scan |
| `sezar-id`     | `sezar-id`       | `sezar-id-inventory.{service,timer}`   | Periodic HSM / KMS / smart-card inventory |
| `sezar-chain`  | `sezar-chain`    | (none — ad-hoc CLI)        | Blockchain address-inventory scanner |
| `sezar-agility`| `sezar-agility`  | (none — ad-hoc CLI)        | V5 PQ-migration recommendations CLI |

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
cargo build --release -p sezar-server
cargo deb           -p sezar-server --no-build
cargo generate-rpm  -p crates/sezar-server
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
sudo apt install ./sezar-server_0.1.0-dev-1_amd64.deb \
                 ./sezar-net_0.1.0-dev-1_amd64.deb \
                 ./sezar-cert_0.1.0-dev-1_amd64.deb \
                 ./sezar-id_0.1.0-dev-1_amd64.deb

# State dirs (sezar-server)
sudo useradd -r -s /sbin/nologin sezar
sudo install -d -m 0750 -o sezar -g sezar /var/lib/sezar /var/lib/sezar/ca

# Drop in env (collector URL, admin token, db url)
sudo systemctl edit sezar-server

# Bring it up
sudo systemctl enable --now sezar-server
```

## Install — Fedora / RHEL / Rocky

```bash
sudo dnf install ./sezar-server-0.1.0~dev-1.x86_64.rpm \
                 ./sezar-net-0.1.0~dev-1.x86_64.rpm \
                 ./sezar-cert-0.1.0~dev-1.x86_64.rpm \
                 ./sezar-id-0.1.0~dev-1.x86_64.rpm

# State dirs and daemon-reload are handled by the package's
# %post script.

sudo systemctl edit sezar-server      # drop in env
sudo systemctl enable --now sezar-server
```

## Uninstall

Both `.deb` and `.rpm` pre-uninstall scripts stop and
disable the matching units. State directories
(`/var/lib/sezar*`) and the `sezar` system user are left
alone — clean them up manually with:

```bash
sudo rm -rf /var/lib/sezar /var/lib/sezar-net
sudo userdel sezar
```

## Sanity-check a built package

```bash
# RPM
rpm -qpi target/generate-rpm/sezar-server-0.1.0~dev-1.x86_64.rpm
rpm -qpl target/generate-rpm/sezar-server-0.1.0~dev-1.x86_64.rpm

# Deb (needs dpkg on the inspecting host)
dpkg -I target/debian/sezar-server_0.1.0-dev-1_amd64.deb
dpkg -c target/debian/sezar-server_0.1.0-dev-1_amd64.deb
```

## Known limitations

- `cargo deb` runs `dpkg-shlibdeps` for automatic library-
  dependency resolution. On non-Debian build hosts that
  binary is absent and the dependencies list is left as
  `$auto`; the `.deb` still installs because
  `glibc`/`openssl` are satisfied on every Debian release.
  Build the `.deb` on a Debian/Ubuntu CI runner if you need
  the exact shared-library deps resolved.

- The `sezar-net-ebpf` crate's BPF object is not packaged
  yet — it requires a specific nightly toolchain and the
  packaging pipeline keeps the stable toolchain dependency.
  Operators who need the eBPF live mode build the BPF
  object out-of-band per `crates/sezar-net-ebpf/README.md`.

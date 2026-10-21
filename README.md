# nvml-exporter

A small Prometheus exporter for one NVIDIA GPU. It reads NVML (`libnvidia-ml.so.1`, shipped
with the driver and loaded at runtime) directly and serves Prometheus text on its own HTTP port.
One process, one thread, no runtime beyond libc and the driver's NVML: a 1.6 MB binary that
runs in about 15 MB of RSS on a Tesla V100, most of it libnvidia-ml.

It exists to replace NVIDIA DCGM where all that is needed is temperatures (including HBM
memory temperature), power, clocks, utilization and throttle reasons, polled about once a
second. A metric the GPU does not support is omitted, never reported as 0; without a driver
the exporter keeps running and serves `nvml_up 0`.

## Install

From the apt repository at pkg.jshkol.com:

```
curl -fsSL https://pkg.jshkol.com/jshkol-archive-keyring.asc \
  | gpg --dearmor \
  | sudo tee /usr/share/keyrings/jshkol-archive-keyring.gpg > /dev/null
echo 'deb [signed-by=/usr/share/keyrings/jshkol-archive-keyring.gpg] https://pkg.jshkol.com stable main' \
  | sudo tee /etc/apt/sources.list.d/shkolnik.list
sudo apt update
sudo apt install nvml-exporter
```

The repository indexes formal releases only (GitHub releases with a `SHA256SUMS` asset), so
the package appears there after the first formal release. Each release also carries the deb
and the bare binary as assets. The rolling `dev` prerelease is never indexed; download its
fixed-name `nvml-exporter-dev_amd64.deb` and install it with `dpkg -i`.

The package does not depend on the driver. Installing it enables and starts
`nvml-exporter.service`, which is safe because the shipped config listens on loopback only.

## Configuration

`/etc/nvml-exporter/config.toml` (a conffile) has three keys:

```toml
listen       = "127.0.0.1:9835"   # address:port; the package ships this loopback default
readers      = []                 # IPv4/IPv6 addresses or CIDRs allowed besides loopback
collect_pcie = false              # PCIe throughput costs a 20 ms NVML sampling call per scrape
```

To let one host on the LAN scrape it, bind the GPU host's own address and allow the reader:

```toml
listen  = "192.0.2.10:9835"
readers = ["192.0.2.1"]
```

Loopback is always allowed. Validate a change with `nvml-exporter check` before
`systemctl restart nvml-exporter`. Log level comes from `NVML_EXPORTER_LOG` (default `info`).

## Endpoints

- `GET /metrics`: the Prometheus exposition.
- `GET /` and `GET /health`: one line, `nvml-exporter X.Y.Z, nvml_up 1|0`.
- Anything else is `404`, a non-GET `405`, a malformed or oversized request `400`. A peer that
  is neither loopback nor in `readers` is disconnected right after accept and counted in
  `nvml_exporter_rejected_connections_total`.

## Commands

```
nvml-exporter serve [--config PATH]   # the daemon (default subcommand)
nvml-exporter once  [--config PATH]   # one scrape to stdout; exit 1 if NVML is unavailable
nvml-exporter check [--config PATH]   # parse the config, print listen, readers, collect_pcie
nvml-exporter --version
```

`--config` defaults to `/etc/nvml-exporter/config.toml`. `once` runs the same collector as
`serve` without a socket, which makes it the quickest check that NVML works on a host.

## Metrics

All series are prefixed `nvml_` and per-GPU series carry `gpu="<index>"`. The catalog (name,
type, whether per GPU, help text) is in [docs/metrics.md](docs/metrics.md); the NVML source of
each series is in [docs/design.md](docs/design.md) §4.4, and what the V100 reports is in
[docs/spike-log.md](docs/spike-log.md).

## Security posture

- Read-only by construction: the binary never calls an NVML setter. `clippy.toml` disallows
  every `set_`/`reset_`/`clear_` method of `nvml_wrapper::Device`, and a test checks the list.
- Access is an IP allow-list (`readers` plus loopback); there is no authentication and no
  TLS. Expose it only on a trusted segment.
- The systemd unit runs under `DynamicUser` with `video` as a supplementary group, a closed
  device policy limited to the NVIDIA nodes, `ProtectSystem=strict`, no capabilities,
  `MemoryDenyWriteExecute` and related hardening (`packaging/deb/nvml-exporter.service`).

## Building and releasing

```
cargo test --locked
cargo install --locked cargo-deb@3.8.0
cargo deb                     # target/debian/nvml-exporter_<version>-1_amd64.deb
```

`rust-toolchain.toml` pins the toolchain. CI builds in `debian:trixie` and runs fmt, clippy
and tests; pull requests also build the deb.

Every push to `main` replaces the rolling `dev` prerelease (deb version
`X.Y.Z~dev<run>+g<sha7>`). To cut a formal release, bump `version` in `Cargo.toml`, run
`cargo check` so `Cargo.lock` follows, commit and push, wait for CI, then run
`scripts/tag-release.sh` (`--dry-run` to preview). The release workflow tests, builds,
attests, publishes the GitHub release with `SHA256SUMS`, and asks pkg.jshkol.com to propose
the release for indexing.

## License

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.

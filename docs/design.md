# nvml-exporter: a small NVML-to-Prometheus exporter (design)

Date: 2026-10-20. Author: agent, from a design discussion with James the same day. Status:
**approved by James 2026-10-20** (every call the author made in this document stands; see §11 for
the rulings). The implementation plan is `docs/superpowers/plans/2026-10-20-nvml-exporter.md`.
This copy is the canonical one; the original lives in the research repo
`<research-repo>/docs/superpowers/specs/2026-10-20-nvml-exporter-design.md`.

## 1. Intent

A single Rust binary that reads one NVIDIA GPU through NVML (`libnvidia-ml.so.1`, shipped with
the driver already on the host) and serves Prometheus text on its own HTTP port. It replaces
NVIDIA DCGM in the the operator's config repo metrics path.

Why: DCGM measured ~175 MB on disk and ~520 MB RSS (hostengine ~450 MB, exporter ~73 MB) for
what is, for us, a dozen NVML reads per second. Its extras (profiling metrics, `dcgmi diag`,
job stats, policies) are not needed. It also put NVIDIA's apt repo and a pin on the VM to keep
Debian's driver safe, and ran a 4.8.4 exporter on a 4.7.0 core that nobody supports. Evidence, in the research repo `<research-repo>`:
`docs/home-deployment/2026-10-20-metrics-diagnostics/README.md` and
`docs/research/2026-10-20-nvml-exporter-survey.md` (no maintained Rust or C NVML exporter
exists; the survey's design findings are folded in below).

Consumers, in order of importance:

1. **The fan controller on reader-host** (future, the operator's config repo): reads GPU temperature, HBM memory
   temperature and power at ~1 Hz over the network from the hypervisor, and drives `pwm8` of
   the fan controller on reader-host. Its fallback rule: no fresh sample within N seconds means hand the fan
   back to a safe duty. It needs a reliable "GPU gone" versus "exporter dead" distinction.
2. **The automated thermal test harness** (research repo): logs the same fields plus clocks,
   throttle reasons and energy beside fan duty and tach.
3. **A future central Prometheus**: scrapes the same endpoint for history.
4. **A human with curl** on the VM or reader-host.

## 2. Goals, non-goals, anti-goals

Goals:

- Correct, fresh values for the thermal and power fields at 1 Hz, with sub-millisecond NVML
  reads done at scrape time (no cache, no background sampler).
- Tiny footprint: one process, no runtime besides libc and the driver's NVML, a few MB RSS,
  a binary in the hundreds of KB.
- Honest failure: a field the GPU does not support is omitted, never reported as 0; an absent
  GPU or driver yields `nvml_up 0` and the process keeps serving and retrying.
- Read-only by construction: the binary never calls an NVML setter (power limit, clocks,
  persistence, compute mode, ECC...). Enforced by a lint, see §7.
- Boring delivery: Debian package from CI, published through pkg.jshkol.com, installed by a
  the operator's config repo role at a pinned version.

Non-goals (not in v1, may come later): multiple GPUs beyond "enumerate all and label by
index" (v1 handles N devices trivially, but is tested on one); TLS; authentication beyond a
reader allow-list; a textfile-collector mode; MIG; NVLink or NvSwitch; profiling metrics
(DCGM's DCP); process-level GPU accounting; a vsock face (hypervisor has no native vsock option
and `args` needs `root@pam`, rejected 2026-10-20).

Anti-goals (would make it a failure even if it works): any write to the GPU; a dependency on
NVIDIA's apt repo; forking `nvidia-smi` per scrape (it re-initializes NVML each time and
pays the persistence-mode re-init when persistence is off); async runtimes or an HTTP
framework for one GET route; `no_std` or static musl builds (the whole program wraps a
glibc-linked `.so` loaded with `dlopen`).

## 3. Decisions already made (2026-10-20, James)

| # | Decision | Alternative rejected and why |
|---|---|---|
| D1 | Own Rust exporter on NVML via `nvml-wrapper` 0.13 | Adopt an OSS exporter: none in Rust/C is maintained and complete; the Go ones are heavy or fork nvidia-smi |
| D2 | Separate HTTP endpoint on its own port | node_exporter textfile: would force exposing 9100 on the VM; a separate port keeps node_exporter loopback-only on every host |
| D3 | Listen on the VM's reader-facing address, not 0.0.0.0 | Bind-all plus nftables: the socket should not exist on other interfaces at all |
| D4 | Reader allow-list inside the binary, checked on the accepted peer address; loopback always allowed | nftables role from the spike: dropped, PR 3 as designed is dead |
| D5 | No TLS | Traffic stays on reader-host's bridge (passthrough pins the VM to reader-host); content is read-only numbers |
| D6 | Plain glibc dynamic build for Debian 13 (`x86_64-unknown-linux-gnu`) | musl static: cannot `dlopen` the driver's glibc `.so` |
| D7 | Separate repo `shkolnik/nvml-exporter`, own CI and releases, deb through pkg.jshkol.com, the operator's config repo pins the version | Embedding in the operator's config repo: wrong repo shape for a compiled, versioned tool; the operator's config repo has no CI and must stay "pins versions, builds nothing" |
| D8 | Collect on scrape | Background sampler: unnecessary for sub-ms reads; collect-on-scrape makes freshness equal scrape time |
| D9 | DCGM parked: the operator's config repo PR #15 is a draft and is deleted when this lands; the `nvidia-dcgm` role is removed from the VM play in the same the operator's config repo change | Keep both: nothing uses DCGM |

## 4. Interface

### 4.1 Command line

```
nvml-exporter serve [--config /etc/nvml-exporter/config.toml]   # the daemon (default subcommand)
nvml-exporter once  [--config ...]                               # one scrape to stdout, exit 0; exit 1 if NVML is unavailable
nvml-exporter check [--config ...]                               # parse config, print the resolved listen address and readers, exit
nvml-exporter --version                                          # "nvml-exporter X.Y.Z"
```

`once` is the diagnostic and the Ansible verify primitive: it exercises the same collector as
`serve` with no socket. `check` lets a config change be validated before a restart.

### 4.2 Config file, TOML

```toml
# /etc/nvml-exporter/config.toml
listen       = "127.0.0.1:9835"   # address:port; the package ships this loopback default
readers      = []                 # IPv4/IPv6 addresses or CIDRs allowed besides loopback
collect_pcie = false              # PCIe throughput costs a 20 ms NVML sampling call per scrape
```

The shipped conffile carries all three keys with comments, so later additions are the only
conffile changes an upgrade can prompt about. `check` prints all three.

the operator's config repo sets `listen` to the VM's address on the segment reader-host reaches it over (today
192.0.2.10/24 by DHCP; reader-host is 192.0.2.1) and `readers = ["192.0.2.1"]`. A changed
DHCP lease makes the bind fail at start; the unit restarts until the address exists (see §6),
and the the operator's config repo side should give the VM a reservation. Port 9835 is the Prometheus
registry's allocation for an NVIDIA GPU exporter and the shipped default; it is overridable
per host through `listen` (James's one request on this spec), and the operator's config repo's
`nvml_exporter_listen` carries that override.

### 4.3 HTTP

- `GET /metrics`: `200 text/plain; version=0.0.4`, the exposition.
- `GET /` and `GET /health`: `200`, one line (`nvml-exporter X.Y.Z, nvml_up 1|0`). `/health`
  is for a human; the controller reads `/metrics`.
- Anything else: `404`; non-GET: `405`; a malformed request line or a request larger than the
  bound below: `400`. A peer not in `readers` and not loopback: the
  connection is closed immediately after accept, before reading, and counted in
  `nvml_exporter_rejected_connections_total`.
- HTTP/1.0 semantics: read the request line and headers with a bounded reader (8 KiB, 2 s
  read timeout), respond, close. One connection at a time, served from a single thread.
  The collector is cheap enough that serialization is a feature: it also serializes NVML
  calls.

### 4.4 Metrics

Prefix `nvml_`. Every per-GPU series carries `gpu="<index>"`; identity lives in one info
series so the rest stays low-label. Units in the name per Prometheus convention; base units
(celsius, watts, joules, hertz, bytes, ratio 0-1).

Always present (no `gpu` label):

| Name | Type | Meaning |
|---|---|---|
| `nvml_up` | gauge | 1 when NVML is loaded and initialized, else 0 |
| `nvml_device_count` | gauge | devices NVML reports (0 when `nvml_up` is 0) |
| `nvml_driver_info{version}` | gauge 1 | driver version string; omitted when `nvml_up` is 0 |
| `nvml_exporter_build_info{version}` | gauge 1 | the binary's version |
| `nvml_exporter_scrape_duration_seconds` | gauge | wall time of this collection |
| `nvml_exporter_scrape_errors_total{metric}` | counter | NVML errors by metric name, excluding `NotSupported` (which is silent omission) |
| `nvml_exporter_rejected_connections_total` | counter | peers refused by the allow-list; present in `serve`'s `/metrics` only, `once` has no listener and omits it |

Per GPU (`gpu` label), omitted individually on `NotSupported`:

| Name | NVML source | Note |
|---|---|---|
| `nvml_gpu_info{gpu,uuid,name,pci_bus_id,vbios_version}` | device identity | always 1 |
| `nvml_gpu_temperature_celsius` | `temperature(Gpu)` | |
| `nvml_memory_temperature_celsius` | `field_values_for([MEMORY_TEMP])` (field 82) | HBM; the fan controller's primary input. Expected on the V100 (DCGM read 52 °C from the same field) but unproven in Rust until the spike |
| `nvml_power_watts` | `power_usage()` / 1000 | refreshes every 100 ms on this card (E3) |
| `nvml_power_limit_watts` | `enforced_power_limit()` / 1000 | the cap in force (150/175/250 W in our runs) |
| `nvml_energy_joules_total` | `total_energy_consumption()` / 1000 | counter; the harness differentiates it |
| `nvml_sm_clock_hertz`, `nvml_memory_clock_hertz` | `clock_info(Graphics|Memory)` × 1e6 | |
| `nvml_utilization_gpu_ratio`, `nvml_utilization_memory_ratio` | `utilization_rates()` / 100 | |
| `nvml_memory_used_bytes`, `nvml_memory_total_bytes` | `memory_info()` | |
| `nvml_throttle_reason{reason}` | `current_throttle_reasons()` bitmask | one 0/1 series per known bit: `gpu_idle`, `applications_clocks_setting`, `sw_power_cap`, `hw_slowdown`, `sync_boost`, `sw_thermal_slowdown`, `hw_thermal_slowdown`, `hw_power_brake_slowdown`, `display_clock_setting`; all bits emitted on every scrape so absence of a reason is a 0, not a gap |
| `nvml_pcie_throughput_bytes_per_second{direction}` | `pcie_throughput(Send|Receive)` × 1000 | 20 ms sampling call inside NVML; **off by default** via `collect_pcie = false` because it adds latency to every scrape |
| `nvml_fan_speed_ratio` | `fan_speed(0)` / 100 | omitted on the V100 (no fan, NotSupported); present for cards that have one |
| `nvml_persistence_mode` | `is_in_persistent_mode()` | 0/1; informs the open persistenced question |

Not collected in v1 (listed so nobody adds them silently): ECC counters, retired pages,
process lists, encoder/decoder utilization, NVLink, MIG, clock events beyond throttle reasons.

## 5. Behavior

- **Startup:** parse config; bind the listener (fail fast with a clear message on bind
  errors, exit 1: systemd restarts); try `Nvml::init()` once. Failure to init is not fatal:
  serve `nvml_up 0` and retry init on each scrape, with the error logged once per state
  change, not per scrape.
- **Scrape:** `init` if not initialized; `device_count`; for each device, read every catalog
  entry in a fixed order; format. `NotSupported` omits the series silently; any other NVML
  error omits the series, increments `scrape_errors_total{metric}`, and logs at debug. If a
  device handle call itself fails (GPU lost, driver reload), drop to `nvml_up 0`, shut NVML
  down and re-init on the next scrape.
- **Logging:** to stderr (journal). Default level info: startup line with version, listen
  address, reader list, driver version and device names; state changes (NVML up/down).
  `RUST_LOG`-style level from `NVML_EXPORTER_LOG` env, default `info`. No per-scrape log lines
  at info.
- **Signals:** SIGTERM/SIGINT exit 0 after the in-flight response. `Type=simple` is enough;
  no sd_notify.

## 6. Packaging and service

Package `nvml-exporter` (cargo-deb, `[package.metadata.deb]` like cfab): binary in
`/usr/bin`, config in `/etc/nvml-exporter/config.toml` as a conffile with the loopback
default, unit `nvml-exporter.service`, README to `/usr/share/doc`. `Depends` from cargo-deb's
`$auto` only (`libc6`, plus `libgcc-s1` if the binary links it); NVML is a runtime `dlopen`, so the package does not depend on the
driver: on a host without it the service runs and serves `nvml_up 0`, which is the intended
diagnostic. `Recommends: libnvidia-ml1`? No: it would pull a driver onto a host that has none.
Not even Recommends.

Unit (shipped in `packaging/deb/nvml-exporter.service`):

```
[Unit]
Description=NVML Prometheus exporter
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=0

[Service]
ExecStart=/usr/bin/nvml-exporter serve --config /etc/nvml-exporter/config.toml
Restart=on-failure
RestartSec=5
DynamicUser=yes
SupplementaryGroups=video
DeviceAllow=/dev/nvidiactl rw
DeviceAllow=/dev/nvidia0 rw
DeviceAllow=char-nvidia rw
DevicePolicy=closed
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
NoNewPrivileges=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
RestrictNamespaces=yes
LockPersonality=yes
MemoryDenyWriteExecute=yes
SystemCallArchitectures=native
CapabilityBoundingSet=
AmbientCapabilities=

[Install]
WantedBy=multi-user.target
```

Install enables and starts it (cargo-deb `enable = true`, `start = true`,
`restart-after-upgrade = true`): safe because the shipped config is loopback-only. The exact
device-node and group requirements of `libnvidia-ml` under `DynamicUser` are unverified; the
spike confirms them on the VM and the plan adjusts the unit (first fallback: a static
`nvml-exporter` system user in `video`; last resort: drop `MemoryDenyWriteExecute`, which the
CUDA/NVML libraries sometimes need).

## 7. Repository

Path `$HOME/workspace/nvml-exporter`, branch `main`, local until James creates
`github.com/shkolnik/nvml-exporter` and adds the agent as collaborator; then
`git remote add origin git@github.com:shkolnik/nvml-exporter.git` and push. Commits by
`shkolnik-beep <shkolnik-beep>` via `git -c`, no attribution trailers. License
MIT OR Apache-2.0 like cfab, copyright James Shkolnik.

Layout, modeled on cfab (single crate, flat `src/`) with beep-browser's pinned toolchain:

```
Cargo.toml  Cargo.lock  rust-toolchain.toml  rustfmt.toml  clippy.toml  LICENSE-MIT  LICENSE-APACHE
README.md   docs/design.md (this spec, copied)  docs/metrics.md (generated from the catalog by a test)
src/main.rs        CLI (clap derive), subcommands, signal handling
src/config.rs      TOML config, allow-list parsing (IpAddr | CIDR), validation
src/collector.rs   NVML init/retry, device enumeration, the catalog, collect() -> Vec<Sample>
src/catalog.rs     the fixed metric table: name, help, type, unit, reader fn
src/expo.rs        Prometheus text formatting from Vec<Sample> (no NVML dependency)
src/http.rs        TcpListener loop, bounded request reader, allow-list check, responses
tests/expo.rs      golden exposition from a fake sample set; parses back with prometheus-parse
tests/http.rs      serve on 127.0.0.1:0 with a fake collector: routes, 404/405, allow-list, bounded reader
tests/live.rs      #[ignore] tests that need a GPU: `once` output has the thermal fields
packaging/deb/nvml-exporter.service  packaging/deb/config.toml
scripts/tag-release.sh (cfab's, name swapped)
.github/workflows/ci.yml  .github/workflows/release.yml  .github/dependabot.yml
```

Toolchain and lints:

- `rust-toolchain.toml`: a pinned stable channel (the current stable at bootstrap, e.g.
  `1.98.0`) with `clippy` and `rustfmt`, beep-browser style, so CI and local agree; bumps are
  one-line diffs. `edition = "2024"`, `rust-version` matching.
- `rustfmt.toml`: `max_width = 100`, `newline_style = "Unix"`.
- `[lints.rust] unsafe_code = "forbid"` (nvml-wrapper hides the FFI). `[lints.clippy]`:
  `all = warn`, `pedantic = warn` with the usual allows decided by the builder, plus cfab's
  `wildcard_enum_match_arm`, `dbg_macro`, `todo`, `unimplemented`.
- `clippy.toml` **`disallowed-methods`** listing every `nvml_wrapper::Device` method whose
  name starts with `set_`, `reset_` or `clear_` at the pinned crate version (power limit,
  application and locked clocks, persistence, compute mode, ECC, fan speed, GPU operation
  mode, MIG mode, accounting, affinity, API restriction, NVLink counters and the rest; the
  builder enumerates them from the crate source and the list is complete, not illustrative) so
  `cargo clippy -D warnings` fails on any write path. A unit test greps `src/` for `set_`
  calls on a `Device` as a second belt.
- Dependencies, deliberately few: `nvml-wrapper = "0.13"`, `clap` (derive), `serde` +
  `toml`, `ipnet` (CIDR), `log` + a tiny stderr logger (`env_logger` is fine). No `tiny_http`:
  the HTTP side is ~100 lines on `std::net` and that keeps the dependency tree auditable.
  Dev: `prometheus-parse`.
- `[profile.release]`: `opt-level = "s"`, `lto = true`, `codegen-units = 1`,
  `panic = "abort"`, `strip = true`.

CI (`ci.yml`, on PR and push to main), one job in `container: debian:trixie` so the glibc and
toolchain match the VM exactly (rustup installed in the job; cargo cache via
`actions/cache`): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --locked` (the live tests are `#[ignore]`), `cargo deb` (PRs only) with the deb
uploaded as an artifact and `dpkg-deb -I` printed. Plus `cargo deny`? No: dependabot weekly
for cargo and github-actions covers it at this size.

Release (`release.yml`, modeled on cfab without the container image): push to main builds a
rolling `dev` prerelease (`nvml-exporter-dev_amd64.deb`, fixed name, deb version
`X.Y.Z~dev<run>+g<sha7>`); a `v*` tag that equals `v<Cargo.toml version>` builds the formal
release: `cargo test --locked`, `cargo deb`, `dist/` with the deb and the bare binary,
`actions/attest-build-provenance`, `SHA256SUMS`, `gh release create --verify-tag
--generate-notes`, then `gh workflow run check-releases.yml -R shkolnik/pkg --ref main` with
`PKG_TRIGGER_TOKEN` (continue-on-error, as in cfab). Build in the same `debian:trixie`
container as CI. Repo settings James sets: the `PKG_TRIGGER_TOKEN` secret (the existing
fine-grained PAT scoped to Actions:write on pkg), workflow permissions as cfab's workflow-level
block, and the first `pool/nvml-exporter/<tag>/<file>.redirect` PR in pkg by hand (onboarding
is a trust decision).

## 7a. Scope of the first plan and ordering

The plan that follows this spec covers the `nvml-exporter` repo only: bootstrap, the binary,
packaging, CI and release workflows, the spike on the card, and the first formal release.
§9 (the operator's config repo integration) is a later plan and PR; its one ruling for James is listed in §11.

Ordering while the GitHub repo does not exist yet: all work commits locally. The spike does
not wait for CI: this workstation runs Debian 13 (glibc 2.41, the VM's exact libc) with Rust
1.98 and can build the deb with `cargo deb` locally; James carries it to the VM with scp and
installs it with `dpkg -i`. CI and the release workflow are proven once James creates the
remote. The spike runs after the collector and `once` exist (a thin vertical slice: config,
catalog, collector, exposition, `once`) and before the HTTP server is finished, so NVML-side
facts the spike changes are cheap to absorb. The collector sits behind a trait from the first
commit, both for the HTTP tests' fake collector and for this reason.

Facts verified 2026-10-20 in `nvml-wrapper` 0.13.0 source: `Nvml::init()` loads
`libnvidia-ml.so.1` (`src/lib.rs:162`; the doc comment saying `.so` is stale), which is the
soname Debian's `libnvidia-ml1` ships, so no explicit path is needed. `Device` has 33
`set_`/`reset_`/`clear_` methods; the `clippy.toml` list names each.

pkg.jshkol.com expectations, verified in `pkg/scripts/check-releases.sh`: only formal GitHub
releases (drafts and prereleases excluded), a `SHA256SUMS` asset whose `.deb` lines become
one `.redirect` each; the rolling `dev` prerelease is never indexed. the operator's config repo therefore
installs formal versions only; dev debs are carried by hand for spikes.

## 8. Proof on the card, before any the operator's config repo change

The first release is gated on a spike on the VM, James operating:

1. The dev-channel deb from the rolling release (or a locally built one carried by James):
   `dpkg -i`, then `nvml-exporter once`. Expected: `nvml_up 1`, `nvml_device_count 1`,
   `nvml_gpu_info` naming the Tesla V100-PCIE-32GB, `nvml_memory_temperature_celsius` present
   and within 1 °C of `nvidia-smi -q -d TEMPERATURE`'s memory line, power within 1 W of
   `nvidia-smi`, energy rising between two calls, throttle reasons showing `gpu_idle 1` or
   `sw_power_cap 1` under the 150 W cap with a load.
2. `systemctl status nvml-exporter` healthy under the hardened unit; if NVML fails under
   `DynamicUser`, the journal says so and §6's fallbacks apply.
3. Footprint: `systemctl show -p MemoryCurrent`, binary size, `ldd` (expect libc, libgcc_s,
   libm only; `libnvidia-ml` is dlopened).
4. With `listen` on the VM address and `readers = ["192.0.2.1"]`: curl from reader-host succeeds,
   curl from the sandbox VM (192.0.2.53) is refused, `127.0.0.1:9835` has no socket (there is
   no second listener, §11.4), and curl from the VM to its own LAN address is refused too (the
   peer is the VM's address, not loopback). `once` is the local diagnostic.

## 9. the operator's config repo integration (its own PR, after the first formal release)

Role `nvml-exporter` under the `nvidia_` naming? No: the tool is `nvml-exporter`, so the role
is `nvml-exporter` and the switch `nvml_exporter: true` with settings `nvml_exporter_version`
(pinned deb version), `nvml_exporter_listen` (defaults to
`ansible_facts['default_ipv4']['address']:9835`) and `nvml_exporter_readers`
(`["192.0.2.1"]` for gpu-host). The role needs pkg.jshkol.com configured on the VM (today only
the pve role does that, in `30_apt.yaml`; the common pieces move to a shared role or the VM
play gets the same tasks). Verify: `nvml-exporter once` on the host, assert the thermal
fields, assert `ss` shows the port on the configured address only, curl from the controller
address is left to James. The same PR removes `nvidia-dcgm` from the VM play and inventory,
adds `nvml-exporter.service` to the gpu-host systemd unit list, and deletes PR #15's branch.

## 10. Left to the builder

- Exact clippy allow-list under `pedantic`; module boundaries inside the layout above.
- The `Sample` type and how the catalog expresses "per-device" vs "global" entries.
- The stderr logger crate; the exact bounded-reader implementation.
- The exact CIDR semantics (`ipnet`) and whether `readers` also accepts hostnames (lean: no,
  addresses only, resolution belongs to Ansible).
- The trixie-container CI details (rustup bootstrap, cache keys).
- Whether `docs/metrics.md` is generated by a test that fails when stale, or hand-written
  and checked by a test (lean: generated).

## 11. Rulings by James (2026-10-20)

James accepted every call in this document. The four questions it had left open:

1. Port: 9835 stays the default; it must be overridable, which `listen` already provides
   (§4.2). Nothing in the binary may assume 9835.
2. Metric prefix: `nvml_`.
3. §9's apt setup (shared role versus copying the pkg.jshkol.com tasks into the VM play):
   deferred to the the operator's config repo integration plan; not this repo's concern.
4. No second loopback socket. `once` covers local diagnostics; a host that wants curl on its
   own LAN address adds that address to `readers`.

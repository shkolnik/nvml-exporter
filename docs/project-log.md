# Project log

Append-only. Newest entry last. Format and rules in `AGENTS.md` §3.

## 2026-10-21T00:28Z — Task 1 complete: crate scaffold, pins, lints, setter ban
Commit: see this commit. Tests: 2 passed. Notes: toolchain pinned to 1.98.0; nvml-wrapper resolved to 0.13.0; the 33-name setter list matches device.rs exactly. Clippy group lints use priority -1 so the individual allows apply.

## 2026-10-21T00:31Z — Task 2 complete: catalog and exposition
Commit: see this commit. Tests: 8 passed (4 expo, 2 catalog, 2 existing). Notes: CATALOG holds 23 entries in §4.4 order; 19 help strings written from §4.4's meaning/note columns. render() panics on a name not in CATALOG and emits HELP/TYPE only for metrics with samples. docs/metrics.md is generated (UPDATE_DOCS=1 cargo test --test catalog).

## 2026-10-21T00:33Z — Task 3 complete: config and allow-list
Commit: see this commit. Tests: 5 passed (tests/config.rs). Notes: bare reader addresses become /32 or /128 so entries() Displays 192.0.2.1/32; allows() canonicalizes the peer so ::ffff:a.b.c.d matches IPv4 entries and loopback; hostnames in readers are rejected. ConfigError::Io carries the path.

## 2026-10-21T00:37Z — Task 4 complete: NVML collector
Commit: see this commit. Tests: 3 new passed (2 unit in src/collector.rs, 1 tests/collector.rs on the no-driver path); 16 total. Notes: crate APIs matched the brief (SampleValue has exactly F64/U32/U64/I64; FieldValueSample.value is a Result; NVML_FI_DEV_MEMORY_TEMP = 82). Decisions not in the brief: nvml_driver_info and the uuid/name/pci_bus_id/vbios_version reads go through read() (errors counted under nvml_driver_info and nvml_gpu_info); utilization and memory_info errors are counted under the _gpu_ratio and _used_bytes names; the throttle label key is reason (design §4.4). Now that the lib names nvml_wrapper paths, clippy prints a non-fatal config warning that Device::set_driver_model (Windows-only) is unreachable; it stays in clippy.toml so the ban list remains complete. The tests module allows clippy::float_cmp for the brief's exact conversion asserts. Review fix: state-change logging moved to a LogState (Unknown/Up/Down) observed after each scrape, so init-ok-then-device-failure no longer flaps info/warn every scrape; set_driver_model in clippy.toml got allow-invalid = true.

## 2026-10-21T00:44Z — Task 5 complete: CLI once/check, packaging, local deb
Commit: see this commit. Tests: 4 new passed (tests/cli.rs) plus 3 ignored in tests/live.rs (live_once_reports_up_and_thermals and the two collector placeholders from Task 4); 21 passed total. Notes: main returns ExitCode (output flushed before return) rather than calling process::exit. Deb target/debian/nvml-exporter_0.1.0-1_amd64.deb (cargo-deb 3.8.0): Depends: libc6 (>= 2.34), no nvidia package; conffiles /etc/nvml-exporter/config.toml; postinst enables via deb-systemd-helper and starts/restarts via deb-systemd-invoke. Binary 1582632 bytes stripped; ldd: libc, libgcc_s, ld-linux (no libm). libgcc_s is linked but cargo-deb's $auto omitted libgcc-s1 (Protected/required on Debian, so harmless). lintian not installed here (no sudo): not run.

## 2026-10-21T00:54Z — Task 6 complete: spike on the V100
Commit: see this commit. Tests: 21 passed, 3 ignored (unchanged count; live test gained a no-fan-error assert). Notes: spike run by the agent on the GPU host, a one-time James-authorized exception to AGENTS.md §3 (host rebuilt afterwards). All §8.1–8.3 expectations met: temps 36/38 °C exact vs nvidia-smi, power 27.326 vs 27.33 W, energy rising, gpu_idle 1, SM clock = nvidia-smi SM, once 0.018 s. Hardened unit verified unchanged via systemd-run with every [Service] property (nvml_up 1, 14.7 MB peak, no fallback). Finding: fanless V100 returns InvalidArgument from fan_speed(0), counted as a scrape error; fix gates the read on num_fans(). No load run (James approves loads); sw_power_cap check deferred. Allow-list test deferred to Task 7 step 6. Details in docs/spike-log.md.

## 2026-10-21T00:57Z — Task 7 complete: HTTP server, serve, signals
Commit: see this commit. Tests: 8 new passed (tests/http.rs); 29 passed, 3 ignored total; tests/http.rs 3 consecutive runs green. Notes: a request whose headers exceed max_request is 400 even when complete. After each response the server shuts down its write side and drains unread input for at most 100 ms total and 4 KiB so the close is orderly (unread input at close makes the kernel send RST, which can destroy the response). Review fix: timeouts are per connection, not per read(): the request read has an overall deadline of read_timeout from the start of handle, and the response write a second budget of read_timeout, so a trickling peer cannot hold the single slot. Accept errors Interrupted/ConnectionAborted are skipped; any other ends run with exit 1. Smoke tested here: serve answers /health, a second instance fails with "cannot bind", SIGTERM exits 0 after "shutting down". Second spike round passed on the VM (docs/spike-log.md, Round 2): hardened unit, loopback and LAN modes; the reader host served, 192.0.2.53 and the self-LAN address refused; 15.7 MB peak. The num_fans gate is confirmed (no scrape error counted). The VM config was restored to the shipped default.

## 2026-10-21T01:08Z — Task 8 complete: CI, release workflow, dependabot, tag script, README
Commit: see this commit. Tests: 29 passed, 3 ignored (live). Notes: ci.yml runs fmt/clippy/test in debian:trixie (rustup with stable default, pinned 1.98.0 via `rustup toolchain install`), PRs also build the deb, print `dpkg-deb -I` and upload it. release.yml splits cfab's single job into `build` (trixie container: test, meta, cargo deb, dist/) and `publish` (runner: attest, formal release + SHA256SUMS + pkg dispatch, or rolling `dev` prerelease by delete+create as cfab does). Action pins follow the brief, which matches cfab: checkout/upload/download-artifact @v7, cache @v5, attest-build-provenance @v4. Formal binary asset named `nvml-exporter-linux-x86_64` (brief and cfab). Both workflows are unverified until the first push; the formal path also needs the PKG_TRIGGER_TOKEN secret.

## 2026-10-21T01:22Z — Task 9 complete: remote, CI green, first formal release
Commit: see this commit. Tests: 29 passed, 3 ignored. Notes: James tagged v0.1.0 on 6a3f6e3 on 2026-10-21; the release run succeeded with assets nvml-exporter_0.1.0-1_amd64.deb, nvml-exporter-linux-x86_64 and SHA256SUMS, and the build-provenance attestation verifies with `gh attestation verify`.
The PKG_TRIGGER_TOKEN dispatch ran check-releases in shkolnik/pkg successfully, but nvml-exporter is not yet under pkg's pool/: the first pool/nvml-exporter/v0.1.0/nvml-exporter_0.1.0-1_amd64.deb.redirect is James's hand step (design §7).
The CI-built dev deb was verified on the V100: identical behavior, 15.7 MB peak.
Changes land by direct push to main, one commit per reviewed task.
The final whole-branch review passed ("ship as-is") with one non-blocking Important finding: GpuLost on a metric read keeps nvml_up 1. 0.1.1 carries that fix, the §5 startup scrape so the journal shows driver and devices, and a README metrics-note correction.

## 2026-10-21T01:23Z — 0.1.1: final-review fixes
Commit: see this commit. Tests: 30 passed, 3 ignored. Notes: (1) GpuLost or Uninitialized from any per-metric read now takes the device-failure path (shutdown, nvml_up 0, one warn, re-init next scrape) and counts nothing; classify() splits NvmlError into Silent/Count/Fatal (unit-tested), and reads are now closures so no NVML call is made after a fatal one. Unverifiable without a real GPU loss.
(2) serve runs one scrape after bind, so the journal shows "NVML up: driver …, devices […]" (or the down warning) right after the listening line, per design §5; seen here as the down warning, the up line is unverified until the card.
(3) README metrics note now says what docs/metrics.md holds and points to design §4.4 and docs/spike-log.md. Version 0.1.1; James tags v0.1.1.

## 2026-10-21T01:37Z — Scrub: home-network and personal details removed; history rewritten
Commit: see this commit. Tests: 30 passed, 3 ignored. Notes: James's rule, now binding (AGENTS.md §3): no home or home-network detail (addresses, hostnames, VM ids, topology, lab operational notes) and no unnecessary personal information in the repo. By James's instruction, earlier entries in this log and docs/spike-log.md were redacted in place, the one exception to append-only; every measurement and conclusion is kept. RFC 5737 addresses stand in for the lab's (192.0.2.10 the GPU host, 192.0.2.1 the reader, 192.0.2.53 the refused peer); test CIDR fixtures moved to 198.51.100.0/24 and 203.0.113.0/24. CI now fails on any private-range address in the tree. The removed lab facts live in the maintainer's private research repo. The git history was rewritten with the same replacement map.

## 2026-10-21T02:05Z — v0.1.1 released; history rewrite pushed; v0.1.0 removed
Commit: see this commit. Notes: history rewritten with git filter-repo (text and messages) from the scrub
map and force-pushed to main with James's authorization; CI and release green on the new head. James tagged
v0.1.1 (deb sha e6aa22b8…74d8, attestation verifies); pkg PR 24 merged and the redirect is served. The
v0.1.0 release and tag deleted on GitHub. James decided no GitHub support purge of orphaned commits is
needed. Agent-side work for this phase is complete; James enables branch protection next.

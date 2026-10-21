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

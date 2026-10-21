# nvml-exporter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Debian-packaged Rust binary that reads one NVIDIA GPU through NVML and serves Prometheus text on its own port, proven on James's V100 and published as the first formal release.

**Architecture:** One crate, flat `src/`: a static metric catalog, a `Collector` trait with an NVML implementation (init once, retry per scrape, never a setter), a pure Prometheus text formatter, a single-threaded HTTP/1.0 server on `std::net` with an in-code reader allow-list, and a clap CLI (`serve`, `once`, `check`). cargo-deb builds the package with a hardened systemd unit; GitHub Actions build in `debian:trixie` and publish releases that pkg.jshkol.com indexes.

**Tech Stack:** Rust (pinned stable 1.98.0, edition 2024), `nvml-wrapper` 0.13, `clap` 4 (derive), `serde` 1 + `toml` 1, `ipnet` 2, `log` 0.4 + `env_logger` 0.11, `ctrlc` 3; dev `prometheus-parse` 0.2; cargo-deb 3.8.0; GitHub Actions.

**Spec:** `docs/design.md` (approved by James 2026-10-20; §11 has the rulings). Section numbers below (§4.2, §6 ...) refer to it.

## Global Constraints

- `rust-toolchain.toml` pins `channel = "1.98.0"` with `clippy` and `rustfmt`; `edition = "2024"`, `rust-version = "1.98"`.
- `[lints.rust] unsafe_code = "forbid"`; `[lints.clippy]` `all = "warn"`, `pedantic = "warn"`, `wildcard_enum_match_arm`, `dbg_macro`, `todo`, `unimplemented` = `"warn"`. CI runs `cargo clippy --all-targets -- -D warnings`, so every warning is a failure.
- The binary never calls an NVML setter: `clippy.toml` `disallowed-methods` lists all 33 `nvml_wrapper::Device` `set_`/`reset_`/`clear_` methods (Task 1 has the list) and a test greps `src/`.
- Dependencies are exactly the Tech Stack list; nothing async, no HTTP framework, no musl target.
- Metric prefix `nvml_`; units in names (celsius, watts, joules, hertz, bytes, ratio); counters end in `_total`; per-GPU series carry `gpu="<index>"`; `NotSupported` omits a series silently and never emits 0.
- Config keys exactly `listen` (default `"127.0.0.1:9835"`), `readers` (default `[]`), `collect_pcie` (default `false`); unknown keys are an error. Loopback peers are always allowed. 9835 is a default only; nothing assumes it.
- HTTP: `GET /metrics` → `200 text/plain; version=0.0.4`; `GET /` and `GET /health` → `200` with body `nvml-exporter <version>, nvml_up <0|1>\n`; other paths `404`; other methods `405`; request read bounded to 8 KiB and 2 s; one connection at a time; rejected peers closed after accept, before reading, and counted in `nvml_exporter_rejected_connections_total`.
- `once` exits 1 when NVML is unavailable; `serve` exits 1 on bind failure; SIGTERM/SIGINT exit 0 after the in-flight response.
- Logging to stderr, level from `NVML_EXPORTER_LOG` (default `info`); state changes only, no per-scrape lines at info.
- Package `nvml-exporter`: `/usr/bin/nvml-exporter`, conffile `/etc/nvml-exporter/config.toml` (loopback default), unit `nvml-exporter.service` as in §6, `Depends: libc6` only (cargo-deb `$auto`), enable + start on install, restart after upgrade.
- `[profile.release]`: `opt-level = "s"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.
- Git: identity `shkolnik-beep <shkolnik-beep>` via `git -c`; no AI attribution trailers of any kind; commits on local `main`; no remote until James says it exists (Task 9).
- James operates the VM and reader-host; the plan gives commands and reads pasted output. Never run anything against those hosts.
- Before every commit: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked`.

## Review Focus

Inputs the spec implies but does not spell out; each is pinned by a test in the task named.

1. A request target with a query string or an absolute URI (`GET /metrics?x=1`, `GET http://host/metrics`) must still serve `/metrics`: Prometheus and curl both send these. (Task 7, `http_matches_path_ignoring_query_and_authority`.)
2. A peer arriving as an IPv4-mapped IPv6 address (`::ffff:192.0.2.1`) when `listen` is an IPv6 wildcard must match an IPv4 `readers` entry; otherwise the allow-list silently blocks the fan controller. (Task 3, `allows_ipv4_mapped_peer`.)
3. A client that connects and sends nothing holds the only thread; the 2 s read timeout must close it and the next client must be served. (Task 7, `http_idle_client_is_dropped_after_timeout`.)
4. A label value containing `"`, `\` or a newline (a VBIOS or device name string) must be escaped per the exposition format or the whole scrape is unparseable. (Task 2, `expo_escapes_label_values`.)
5. NVML failing on one metric mid-scrape (any error other than `NotSupported`) must omit that series, count it under `nvml_exporter_scrape_errors_total{metric}` and leave `nvml_up 1`, while a failing device handle drops to `nvml_up 0` and re-initializes next scrape. (Task 4, `collector_reads_other_errors_count_and_omit` and `collector_device_failure_drops_to_down`.)

---

## File map

| Path | Responsibility |
|---|---|
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml` | crate, pins, lints, setter ban |
| `src/main.rs` | clap CLI, logger setup, subcommand dispatch, signal flag |
| `src/catalog.rs` | `MetricDef`, `Kind`, `Sample`, the static `CATALOG`, `render_docs()` |
| `src/expo.rs` | `render(&[Sample]) -> String` Prometheus text, no NVML dependency |
| `src/config.rs` | `Config`, TOML parsing, `AllowList` |
| `src/collector.rs` | `Collector` trait, `Scrape`, `NvmlCollector`, pure conversion helpers |
| `src/http.rs` | `Server`, bounded request reader, routing, `handle()` |
| `src/lib.rs` | `pub mod` for the five modules so integration tests can use them |
| `tests/common/mod.rs` | `FakeCollector` |
| `tests/expo.rs`, `tests/config.rs`, `tests/http.rs`, `tests/catalog.rs`, `tests/no_setters.rs`, `tests/live.rs` | integration tests; `live.rs` is `#[ignore]` |
| `docs/metrics.md` | generated catalog table, checked by `tests/catalog.rs` |
| `packaging/deb/nvml-exporter.service`, `packaging/deb/config.toml` | unit and conffile |
| `scripts/tag-release.sh` | cfab's script with the crate name swapped |
| `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/dependabot.yml` | CI and release |
| `docs/spike-log.md` | the spike's commands, James's paste-backs, conclusions |

---

### Task 1: Crate scaffold, pins, lints, setter ban

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `src/main.rs`, `src/lib.rs`
- Test: `tests/no_setters.rs`

**Interfaces:**
- Produces: a crate `nvml-exporter` (lib target `nvml_exporter` + bin `nvml-exporter`) that builds, lints clean, and whose `--version` prints `nvml-exporter 0.1.0`.

- [ ] **Step 1: Write `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`**

`rust-toolchain.toml`: `[toolchain] channel = "1.98.0"`, `components = ["clippy", "rustfmt"]`.
`rustfmt.toml`: `max_width = 100`, `newline_style = "Unix"`.
`clippy.toml`: `disallowed-methods` with one entry per name below, each as `{ path = "nvml_wrapper::Device::<name>", reason = "nvml-exporter is read-only; it never calls an NVML setter" }`. The 33 names, enumerated 2026-10-20 from nvml-wrapper 0.13.0 `src/device.rs`:

```
clear_accounting_pids clear_cpu_affinity clear_ecc_error_counts
reset_applications_clocks reset_gpu_locked_clocks reset_mem_locked_clocks
set_accounting set_api_restricted set_applications_clocks set_auto_boosted_clocks
set_auto_boosted_clocks_default set_clock_offset set_compute_mode
set_confidential_compute_state set_cpu_affinity set_default_fan_speed set_drain
set_driver_model set_ecc set_fan_control_policy set_fan_speed set_gpc_clock_vf_offset
set_gpm_streaming_enabled set_gpu_locked_clocks set_gpu_op_mode set_mem_clock_vf_offset
set_mem_locked_clocks set_mig_mode set_persistent set_power_management_limit
set_power_mizer_mode set_temperature_threshold set_vgpu_scheduler_state
```

- [ ] **Step 2: Write `Cargo.toml`**

`[package]`: `name = "nvml-exporter"`, `version = "0.1.0"`, `edition = "2024"`, `rust-version = "1.98"`, `description = "Small Prometheus exporter for NVIDIA GPUs over NVML"`, `license = "MIT OR Apache-2.0"`, `repository = "https://github.com/shkolnik/nvml-exporter"`. `[lib] name = "nvml_exporter"`, `path = "src/lib.rs"`; `[[bin]] name = "nvml-exporter"`, `path = "src/main.rs"`. Dependencies and dev-dependencies from the Tech Stack with the caret versions given there (`clap` features `["derive"]`, `serde` features `["derive"]`, `ctrlc` features `["termination"]`). Lints and release profile from Global Constraints; under `[lints.clippy]` add allows only for pedantic lints that fire on idiomatic code in this crate (`module_name_repetitions`, `missing_errors_doc`, `missing_panics_doc`, `must_use_candidate` are the expected set; add others only with a one-line reason). Leave `[package.metadata.deb]` for Task 5.

- [ ] **Step 3: Write minimal `src/lib.rs` and `src/main.rs`**

`src/lib.rs`: empty for now except a crate doc line. `src/main.rs`: clap `#[derive(Parser)] #[command(name = "nvml-exporter", version)] struct Cli { #[arg(long, global = true, default_value = "/etc/nvml-exporter/config.toml")] config: PathBuf, #[command(subcommand)] cmd: Option<Cmd> }` with `enum Cmd { Serve, Once, Check }`; `main` parses and, for now, prints the chosen subcommand name (default `Serve` when none) and exits 0. Task 5 fills it in.

- [ ] **Step 4: Write the failing test `tests/no_setters.rs`**

```rust
const SETTERS: &[&str] = &[ /* the 33 names from Step 1 */ ];

#[test]
fn clippy_toml_disallows_every_setter() {
    let toml = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/clippy.toml")).unwrap();
    for name in SETTERS {
        assert!(toml.contains(&format!("nvml_wrapper::Device::{name}\"")), "clippy.toml lacks {name}");
    }
    assert_eq!(SETTERS.len(), 33);
}

#[test]
fn sources_never_call_a_setter() {
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for name in SETTERS {
            assert!(!text.contains(&format!(".{name}(")), "{} calls {name}", path.display());
        }
    }
}
```

- [ ] **Step 5: Build, lint, test**

Run: `cargo build && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked`
Expected: all green; `cargo run -- --version` prints `nvml-exporter 0.1.0`. Rustup downloads 1.98.0 on first use if `rustup toolchain list` lacks it (it is installed on this sandbox).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml clippy.toml src tests
git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit -m "Crate scaffold: pinned toolchain, lints, NVML setter ban"
```

---

### Task 2: Catalog and exposition

**Files:**
- Create: `src/catalog.rs`, `src/expo.rs`, `docs/metrics.md`
- Modify: `src/lib.rs` (add `pub mod catalog; pub mod expo;`)
- Test: `tests/expo.rs`, `tests/catalog.rs`

**Interfaces:**
- Produces, in `catalog.rs`:
  ```rust
  pub enum Kind { Gauge, Counter }
  pub struct MetricDef { pub name: &'static str, pub help: &'static str, pub kind: Kind, pub per_gpu: bool }
  pub static CATALOG: &[MetricDef];                       // the §4.4 tables, in that order
  pub fn find(name: &str) -> Option<&'static MetricDef>;
  pub struct Sample { pub name: &'static str, pub labels: Vec<(&'static str, String)>, pub value: f64 }
  impl Sample { pub fn global(name: &'static str, value: f64) -> Self;
                pub fn gpu(name: &'static str, gpu: u32, value: f64) -> Self;   // labels = [("gpu", index)]
                pub fn with_label(self, key: &'static str, value: impl Into<String>) -> Self; }
  pub fn render_docs() -> String;                          // the docs/metrics.md body
  ```
- Produces, in `expo.rs`: `pub fn render(samples: &[Sample]) -> String`.

- [ ] **Step 1: Write the failing tests `tests/expo.rs`**

```rust
use nvml_exporter::{catalog::Sample, expo::render};

#[test]
fn expo_golden_small_scrape() {
    let samples = vec![
        Sample::global("nvml_up", 1.0),
        Sample::gpu("nvml_gpu_temperature_celsius", 0, 36.0),
        Sample::gpu("nvml_energy_joules_total", 0, 12345.678),
        Sample::global("nvml_driver_info", 1.0).with_label("version", "550.163.01"),
    ];
    let out = render(&samples);
    let expected = "\
# HELP nvml_up 1 when NVML is loaded and initialized, else 0\n\
# TYPE nvml_up gauge\n\
nvml_up 1\n\
# HELP nvml_driver_info Driver version string; omitted when nvml_up is 0\n\
# TYPE nvml_driver_info gauge\n\
nvml_driver_info{version=\"550.163.01\"} 1\n\
# HELP nvml_gpu_temperature_celsius GPU die temperature\n\
# TYPE nvml_gpu_temperature_celsius gauge\n\
nvml_gpu_temperature_celsius{gpu=\"0\"} 36\n\
# HELP nvml_energy_joules_total Total energy consumed since the driver loaded\n\
# TYPE nvml_energy_joules_total counter\n\
nvml_energy_joules_total{gpu=\"0\"} 12345.678\n";
    assert_eq!(out, expected);   // catalog order, not sample order
}

#[test]
fn expo_escapes_label_values() {
    let s = Sample::global("nvml_gpu_info", 1.0).with_label("name", "a\"b\\c\nd");
    assert!(render(&[s]).contains(r#"name="a\"b\\c\nd""#));
}

#[test]
fn expo_parses_back_with_prometheus_parse() {
    let samples = vec![Sample::global("nvml_up", 0.0), Sample::global("nvml_device_count", 0.0)];
    let text = render(&samples);
    let scrape = prometheus_parse::Scrape::parse(text.lines().map(|l| Ok(l.to_owned()))).unwrap();
    assert_eq!(scrape.samples.len(), 2);
}

#[test]
#[should_panic(expected = "not in CATALOG")]
fn expo_rejects_unknown_metric() {
    render(&[Sample::global("nvml_bogus", 1.0)]);
}
```

The HELP strings in the golden test are the ones `CATALOG` must carry; the implementer writes the rest of the catalog's help strings in the same register (one short sentence, no trailing period).

- [ ] **Step 2: Write the failing tests `tests/catalog.rs`**

```rust
use nvml_exporter::catalog::{CATALOG, Kind, render_docs};

#[test]
fn catalog_names_are_unique_prefixed_and_counters_end_in_total() {
    let mut seen = std::collections::HashSet::new();
    for m in CATALOG {
        assert!(m.name.starts_with("nvml_"), "{}", m.name);
        assert!(seen.insert(m.name), "duplicate {}", m.name);
        if matches!(m.kind, Kind::Counter) { assert!(m.name.ends_with("_total"), "{}", m.name); }
    }
    assert_eq!(CATALOG.len(), 23);
}

#[test]
fn docs_metrics_md_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/metrics.md");
    let want = render_docs();
    if std::env::var_os("UPDATE_DOCS").is_some() { std::fs::write(path, &want).unwrap(); }
    let have = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(have, want, "docs/metrics.md is stale: run UPDATE_DOCS=1 cargo test --test catalog");
}
```

The 23 entries: the 7 global metrics of §4.4 and the 16 per-GPU metrics (`nvml_gpu_info`, gpu temperature, memory temperature, power, power limit, energy, sm clock, memory clock, utilization gpu, utilization memory, memory used, memory total, throttle reason, pcie throughput, fan speed, persistence mode).

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --test expo --test catalog`
Expected: compile errors (`catalog`/`expo` modules missing).

- [ ] **Step 4: Implement `src/catalog.rs` and `src/expo.rs`**

`CATALOG` is a `static` array literal in §4.4 order with `per_gpu` set for the second table. `render()` groups samples by `name` in `CATALOG` order (a sample whose name is not in the catalog panics with a message containing `not in CATALOG`: a programming error, not a runtime condition), writes `# HELP` and `# TYPE` once per metric that has at least one sample, then one line per sample with labels in insertion order, values via `f64` `Display` (prints `36`, `0.5`, `12345.678`), label values escaped (`\` → `\\`, `"` → `\"`, newline → `\n`), help text escaped (`\` → `\\`, newline → `\n`). `render_docs()` returns a Markdown table (`| Name | Type | Per GPU | Help |`) preceded by one heading line `# Metrics` and one sentence saying the file is generated by `tests/catalog.rs`.

- [ ] **Step 5: Generate the docs and run the tests**

Run: `UPDATE_DOCS=1 cargo test --test catalog && cargo test --test expo --test catalog`
Expected: PASS; `docs/metrics.md` exists with 23 rows.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
git add src/catalog.rs src/expo.rs src/lib.rs tests/expo.rs tests/catalog.rs docs/metrics.md
git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit -m "Metric catalog and Prometheus text exposition"
```

---

### Task 3: Config and allow-list

**Files:**
- Create: `src/config.rs`
- Modify: `src/lib.rs` (add `pub mod config;`)
- Test: `tests/config.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Config { pub listen: SocketAddr, pub readers: AllowList, pub collect_pcie: bool }
  pub struct AllowList(Vec<ipnet::IpNet>);
  impl AllowList { pub fn parse(entries: &[String]) -> Result<Self, ConfigError>;
                   pub fn allows(&self, peer: IpAddr) -> bool;      // loopback always true; peer.to_canonical() before matching
                   pub fn entries(&self) -> &[IpNet]; }
  pub enum ConfigError { Io { path: PathBuf, source: io::Error }, Toml(toml::de::Error), Reader { entry: String, reason: String } }  // impl Display + Error
  pub fn parse(text: &str) -> Result<Config, ConfigError>;
  pub fn load(path: &Path) -> Result<Config, ConfigError>;
  ```

- [ ] **Step 1: Write the failing tests `tests/config.rs`**

```rust
use nvml_exporter::config::{parse, AllowList, ConfigError};
use std::net::IpAddr;

fn ip(s: &str) -> IpAddr { s.parse().unwrap() }

#[test]
fn config_empty_text_gives_defaults() {
    let c = parse("").unwrap();
    assert_eq!(c.listen.to_string(), "127.0.0.1:9835");
    assert!(c.readers.entries().is_empty());
    assert!(!c.collect_pcie);
}

#[test]
fn config_parses_all_keys() {
    let c = parse("listen = \"192.0.2.10:9835\"\nreaders = [\"192.0.2.1\", \"203.0.113.0/24\"]\ncollect_pcie = true\n").unwrap();
    assert_eq!(c.listen.to_string(), "192.0.2.10:9835");
    assert_eq!(c.readers.entries().len(), 2);
    assert!(c.collect_pcie);
}

#[test]
fn config_rejects_unknown_key_and_bad_reader() {
    assert!(matches!(parse("port = 9835\n"), Err(ConfigError::Toml(_))));
    match parse("readers = [\"reader-host\"]\n") { Err(ConfigError::Reader { entry, .. }) => assert_eq!(entry, "reader-host"), other => panic!("{other:?}") }
}

#[test]
fn allows_loopback_always_and_listed_peers() {
    let a = AllowList::parse(&["192.0.2.1".into(), "198.51.100.0/24".into()]).unwrap();
    assert!(a.allows(ip("127.0.0.1")));
    assert!(a.allows(ip("::1")));
    assert!(a.allows(ip("192.0.2.1")));
    assert!(a.allows(ip("198.51.100.7")));
    assert!(!a.allows(ip("192.0.2.53")));
    assert!(!a.allows(ip("203.0.113.1")));
    assert!(!AllowList::parse(&[]).unwrap().allows(ip("192.0.2.1")));
}

#[test]
fn allows_ipv4_mapped_peer() {
    let a = AllowList::parse(&["192.0.2.1".into()]).unwrap();
    assert!(a.allows(ip("::ffff:192.0.2.1")));
    assert!(a.allows(ip("::ffff:127.0.0.1")));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test config`
Expected: compile error, `config` module missing.

- [ ] **Step 3: Implement `src/config.rs`**

Deserialize into a private `#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Raw { listen: Option<String>, readers: Option<Vec<String>>, collect_pcie: Option<bool> }`, then build `Config`. A reader entry is parsed as `IpAddr` first (wrapped with `IpNet::from`), then as `IpNet`; failure of both is `ConfigError::Reader`. A `listen` that does not parse as `SocketAddr` is reported as `ConfigError::Reader { entry: <the value>, reason: "listen must be address:port" }`. `ConfigError` needs `Debug` (the test uses `{other:?}`).

- [ ] **Step 4: Run the tests**

Run: `cargo test --test config`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
git add src/config.rs src/lib.rs tests/config.rs
git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit -m "Config file and reader allow-list"
```

---

### Task 4: Collector

**Files:**
- Create: `src/collector.rs`, `tests/common/mod.rs`
- Modify: `src/lib.rs` (add `pub mod collector;`)
- Test: unit tests inside `src/collector.rs` (pure helpers; the NVML object cannot be faked), `tests/collector.rs`

**Interfaces:**
- Consumes: `catalog::Sample`, `catalog::CATALOG` names.
- Produces:
  ```rust
  pub struct Scrape { pub up: bool, pub samples: Vec<Sample> }   // samples include nvml_up, device_count, build_info, scrape_duration, scrape_errors_total
  pub trait Collector { fn collect(&mut self) -> Scrape; }
  pub struct NvmlCollector { /* Option<Nvml>, collect_pcie, error counts, logged state */ }
  impl NvmlCollector { pub fn new(collect_pcie: bool) -> Self; }   // does not init; first collect() does
  impl Collector for NvmlCollector { ... }
  // pure helpers, pub for tests:
  pub fn throttle_samples(gpu: u32, reasons: nvml_wrapper::bitmasks::device::ThrottleReasons) -> Vec<Sample>;  // 9 samples, fixed order, 0/1
  pub fn milli_to_unit(v: u32) -> f64;      // mW → W, mJ → J
  pub fn mhz_to_hz(v: u32) -> f64;
  pub fn percent_to_ratio(v: u32) -> f64;
  pub const THROTTLE_REASONS: &[(&str, ThrottleReasons)];  // gpu_idle, applications_clocks_setting, sw_power_cap, hw_slowdown, sync_boost, sw_thermal_slowdown, hw_thermal_slowdown, hw_power_brake_slowdown, display_clock_setting
  ```
- `tests/common/mod.rs`: `pub struct FakeCollector { pub up: bool, pub samples: Vec<Sample>, pub calls: Arc<AtomicUsize> }` implementing `Collector` by returning a clone of `samples` plus an `nvml_up` sample and incrementing `calls`.

- [ ] **Step 1: Write the failing unit tests in `src/collector.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use nvml_wrapper::bitmasks::device::ThrottleReasons;

    #[test]
    fn throttle_bits_become_nine_zero_one_series() {
        let s = throttle_samples(0, ThrottleReasons::SW_POWER_CAP | ThrottleReasons::HW_SLOWDOWN);
        assert_eq!(s.len(), 9);
        let on: Vec<&str> = s.iter().filter(|x| x.value == 1.0).map(|x| x.labels[1].1.as_str()).collect();
        assert_eq!(on, ["sw_power_cap", "hw_slowdown"]);
        assert!(s.iter().all(|x| x.name == "nvml_throttle_reason" && x.labels[0] == ("gpu", "0".into())));
        assert!(throttle_samples(0, ThrottleReasons::empty()).iter().all(|x| x.value == 0.0));
    }

    #[test]
    fn unit_conversions() {
        assert_eq!(milli_to_unit(150_000), 150.0);
        assert_eq!(mhz_to_hz(1380), 1_380_000_000.0);
        assert_eq!(percent_to_ratio(37), 0.37);
    }
}
```

- [ ] **Step 2: Write the failing test `tests/collector.rs`**

```rust
use nvml_exporter::collector::{Collector, NvmlCollector};

#[test]
fn collector_without_driver_reports_down_and_keeps_serving() {
    // This sandbox has no libnvidia-ml.so.1; on a host that has one this test is skipped.
    let mut c = NvmlCollector::new(false);
    let s1 = c.collect();
    if s1.up { eprintln!("driver present, skipping"); return; }
    let names: Vec<&str> = s1.samples.iter().map(|x| x.name).collect();
    assert!(names.contains(&"nvml_up") && names.contains(&"nvml_device_count") && names.contains(&"nvml_exporter_build_info") && names.contains(&"nvml_exporter_scrape_duration_seconds"));
    assert!(!names.contains(&"nvml_driver_info"));
    let up = s1.samples.iter().find(|x| x.name == "nvml_up").unwrap().value;
    let count = s1.samples.iter().find(|x| x.name == "nvml_device_count").unwrap().value;
    assert_eq!((up, count), (0.0, 0.0));
    let s2 = c.collect();      // retries init, still down, does not panic
    assert!(!s2.up);
}
```

Review Focus 5 (`collector_reads_other_errors_count_and_omit`, `collector_device_failure_drops_to_down`) cannot be exercised without a GPU; it is pinned instead by the structure in Step 4 (one `read()` helper every metric goes through, one device-level error path) and verified on the card in Task 6 step 4 by reading the error counter after a driver reload. Name the two `#[ignore]` tests in `tests/live.rs` (Task 5) after them so the gap is visible in `cargo test -- --ignored --list`.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --test collector --lib`
Expected: compile errors (`collector` missing).

- [ ] **Step 4: Implement `src/collector.rs`**

`collect()`: start an `Instant`; if `nvml` is `None`, try `Nvml::init()`; on error log at `warn` only if the previous state was up or this is the first attempt (keep a `logged_down: bool`), emit `nvml_up 0`, `nvml_device_count 0`, build info, duration, and the error counters, return. On success log at `info` the driver version and each device's name, once per transition. With NVML up: `device_count()`; for each index `device_by_index(i)`; if either errors, log `warn`, set `nvml = None` (dropping the handle; `Nvml::shutdown` consumes it, call it and ignore its result), emit the down scrape. Per device, read in §4.4 order through one helper:

```rust
fn read<T>(&mut self, metric: &'static str, r: Result<T, NvmlError>) -> Option<T>
// Ok → Some; Err(NotSupported) → None silently; other Err → None, errors[metric] += 1, log at debug
```

Sources, from nvml-wrapper 0.13 (`Device` methods): `name()`, `uuid()`, `pci_info().bus_id`, `vbios_version()` → `nvml_gpu_info` labels (an individual failure leaves that label empty rather than dropping the info series); `temperature(TemperatureSensor::Gpu)`; `field_values_for(&[FieldId(nvml_wrapper::sys_exports::field_id::NVML_FI_DEV_MEMORY_TEMP)])` whose outer `Result` and inner `Vec<Result<FieldValueSample, _>>` and `sample.value: Result<SampleValue, _>` all go through `read`, with `SampleValue::U32`/`U64`/`I64`/`F64` converted to `f64` (match all four; no wildcard arm); `power_usage()` and `enforced_power_limit()` through `milli_to_unit`; `total_energy_consumption()` (u64 mJ) `/ 1000.0`; `clock_info(Clock::SM)` and `clock_info(Clock::Memory)` through `mhz_to_hz` (§4.4 says `Graphics|Memory`; `Clock::SM` is the builder's call here because the metric is named `nvml_sm_clock_hertz` and it is the clock `nvidia-smi` reports as SM; the spike compares it with `nvidia-smi -q -d CLOCK`); `utilization_rates()` `.gpu`/`.memory` through `percent_to_ratio`; `memory_info()` `.used`/`.total`; `current_throttle_reasons()` through `throttle_samples`; when `collect_pcie`, `pcie_throughput(PcieUtilCounter::Send|Receive)` (KB/s) `* 1000.0` with label `direction` = `tx`/`rx`; `fan_speed(0)` through `percent_to_ratio`; `is_in_persistent_mode()` as 0/1. Then `nvml_exporter_scrape_duration_seconds` and one `nvml_exporter_scrape_errors_total{metric}` per metric that has ever errored (counters persist for the process lifetime). `nvml_exporter_build_info{version}` uses `env!("CARGO_PKG_VERSION")`.

- [ ] **Step 5: Run the tests**

Run: `cargo test --test collector --lib`
Expected: PASS (the integration test takes the no-driver path on this sandbox).

- [ ] **Step 6: Write `tests/common/mod.rs` (`FakeCollector`) and check it compiles**

Run: `cargo test --locked`
Expected: PASS; an unused-module warning from `common` in crates that do not use it yet is avoided by adding `#![allow(dead_code)]` at the top of `common/mod.rs`.

- [ ] **Step 7: Lint and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
git add src/collector.rs src/lib.rs tests/collector.rs tests/common
git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit -m "NVML collector: init once, retry per scrape, omit unsupported fields"
```

---

### Task 5: CLI (`once`, `check`), packaging, local deb

**Files:**
- Modify: `src/main.rs`, `Cargo.toml` (deb metadata)
- Create: `packaging/deb/nvml-exporter.service`, `packaging/deb/config.toml`, `tests/live.rs`
- Test: `tests/cli.rs`

**Interfaces:**
- Consumes: `config::load`, `collector::{NvmlCollector, Collector}`, `expo::render`.
- Produces: the binary's `once` and `check` behavior, and `serve` as a stub that prints `serve: not implemented until the HTTP server lands` to stderr and exits 2 (replaced in Task 7). A deb at `target/debian/nvml-exporter_0.1.0-1_amd64.deb`.

- [ ] **Step 1: Write the failing tests `tests/cli.rs`**

```rust
use std::process::Command;
fn bin() -> Command { Command::new(env!("CARGO_BIN_EXE_nvml-exporter")) }

#[test]
fn cli_version() {
    let out = bin().arg("--version").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), concat!("nvml-exporter ", env!("CARGO_PKG_VERSION")));
}

#[test]
fn cli_check_prints_resolved_config() {
    let dir = std::env::temp_dir().join(format!("nvml-exporter-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("config.toml");
    std::fs::write(&cfg, "listen = \"127.0.0.1:19835\"\nreaders = [\"192.0.2.1\"]\n").unwrap();
    let out = bin().args(["check", "--config"]).arg(&cfg).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("listen = 127.0.0.1:19835") && text.contains("readers = [192.0.2.1/32]") && text.contains("collect_pcie = false"));
}

#[test]
fn cli_check_missing_config_fails_with_path() {
    let out = bin().args(["check", "--config", "/nonexistent/config.toml"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("/nonexistent/config.toml"));
}

#[test]
fn cli_once_without_driver_prints_down_and_exits_1() {
    let dir = std::env::temp_dir().join(format!("nvml-exporter-once-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("config.toml");
    std::fs::write(&cfg, "").unwrap();
    let out = bin().args(["once", "--config"]).arg(&cfg).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    if text.contains("nvml_up 1") { return; }   // driver present on this host
    assert_eq!(out.status.code(), Some(1));
    assert!(text.contains("nvml_up 0\n"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test cli`
Expected: `cli_check_*` and `cli_once_*` fail (subcommands print their name only).

- [ ] **Step 3: Implement the subcommands in `src/main.rs`**

`main` initializes `env_logger` from `Env::new().filter_or("NVML_EXPORTER_LOG", "info")` with `format_timestamp(None)`, loads the config (error → message with the path on stderr, exit 1), then: `check` prints the three lines `listen = <addr>`, `readers = [<net>, ...]`, `collect_pcie = <bool>` and exits 0; `once` builds `NvmlCollector::new(cfg.collect_pcie)`, collects once, prints `render(&scrape.samples)` to stdout and exits `if scrape.up { 0 } else { 1 }`; `serve` is the stub described above. Exit codes via `std::process::exit` after all output is flushed.

- [ ] **Step 4: Run the tests**

Run: `cargo test --test cli`
Expected: PASS.

- [ ] **Step 5: Write `tests/live.rs`**

Three `#[ignore]` tests that run the binary's `once` with an empty config and assert: `live_once_reports_up_and_thermals` (`nvml_up 1`, `nvml_device_count 1`, lines starting with `nvml_gpu_info{`, `nvml_gpu_temperature_celsius{`, `nvml_memory_temperature_celsius{`, `nvml_power_watts{`, `nvml_power_limit_watts{`, `nvml_energy_joules_total{`, nine `nvml_throttle_reason{` lines, no `nvml_fan_speed_ratio` line on a V100); `collector_reads_other_errors_count_and_omit` and `collector_device_failure_drops_to_down` as named placeholders that assert the scrape parses and carry a doc comment saying they are verified by hand in `docs/spike-log.md` (Task 6).

- [ ] **Step 6: Write the unit and conffile, and the deb metadata**

`packaging/deb/nvml-exporter.service`: exactly §6's unit. `packaging/deb/config.toml`: the §4.2 example with its comments, `listen = "127.0.0.1:9835"`, `readers = []`, `collect_pcie = false`. In `Cargo.toml`:

```toml
[package.metadata.deb]
maintainer = "James Shkolnik <jshkolnik@gmail.com>"
copyright = "2026 James Shkolnik"
extended-description = "Reads one NVIDIA GPU through NVML (libnvidia-ml.so.1 from the installed driver, loaded at runtime) and serves Prometheus text on its own port. Read-only by construction; without a driver it serves nvml_up 0."
section = "net"
priority = "optional"
depends = "$auto"
assets = [
    ["target/release/nvml-exporter", "usr/bin/", "755"],
    ["README.md", "usr/share/doc/nvml-exporter/", "644"],
    ["docs/metrics.md", "usr/share/doc/nvml-exporter/", "644"],
    ["packaging/deb/config.toml", "etc/nvml-exporter/config.toml", "644"],
]
conf-files = ["/etc/nvml-exporter/config.toml"]
maintainer-scripts = "packaging/deb"

[package.metadata.deb.systemd-units]
unit-scripts = "packaging/deb"
unit-name = "nvml-exporter"
enable = true
start = true
restart-after-upgrade = true
```

- [ ] **Step 7: Install cargo-deb and build the package**

Run: `cargo +stable install --locked cargo-deb@3.8.0 && cargo deb && dpkg-deb -I target/debian/nvml-exporter_0.1.0-1_amd64.deb && dpkg-deb -c target/debian/nvml-exporter_0.1.0-1_amd64.deb && dpkg-deb -e target/debian/nvml-exporter_0.1.0-1_amd64.deb /tmp/nvml-ctl && cat /tmp/nvml-ctl/conffiles /tmp/nvml-ctl/postinst && ldd target/release/nvml-exporter && ls -l target/release/nvml-exporter`
Expected: `Depends: libc6 (>= ...)` and `libgcc-s1` at most, no nvidia package; conffiles lists `/etc/nvml-exporter/config.toml`; postinst contains `systemctl enable` and `start` fragments; `ldd` shows libc, libgcc_s, libm only; binary under 2 MB stripped.

- [ ] **Step 8: Lint and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
git add src/main.rs Cargo.toml Cargo.lock packaging tests/cli.rs tests/live.rs
git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit -m "CLI once/check, Debian packaging with hardened unit"
```

---

### Task 6: Spike on the V100 (James operates)

**Files:**
- Create: `docs/spike-log.md`
- Modify: `packaging/deb/nvml-exporter.service` and `docs/design.md` §6 if the hardening needs a fallback; this plan's Amendments section.

**Interfaces:**
- Consumes: the deb from Task 5.
- Produces: verified NVML facts on the card; the unit as it will ship.

This task stops for James. Give the commands below, wait for the pasted output, record it in `docs/spike-log.md` (command, output excerpt, conclusion), and only then continue.

- [ ] **Step 1: Hand over the deb**

Tell James the path `target/debian/nvml-exporter_0.1.0-1_amd64.deb` on this sandbox and ask them to copy it to the VM (`scp` from their laptop through the sandbox, or whichever path they prefer) and run, on the VM:

```bash
sudo dpkg -i nvml-exporter_0.1.0-1_amd64.deb
nvml-exporter once; echo "exit $?"
nvidia-smi -q -d TEMPERATURE,POWER | grep -E 'GPU Current Temp|Memory Current Temp|Power Draw|Current Power Limit'
sleep 2; nvml-exporter once | grep -E 'nvml_energy|nvml_power_watts'
```

Expected (§8.1): `nvml_up 1`, `nvml_device_count 1`, `nvml_gpu_info` naming `Tesla V100-PCIE-32GB`, `nvml_memory_temperature_celsius` within 1 °C of the `Memory Current Temp` line, power within 1 W, energy higher on the second call, `nvml_throttle_reason{reason="gpu_idle"} 1` on an idle card.

- [ ] **Step 2: Service under the hardened unit**

```bash
systemctl status nvml-exporter --no-pager
journalctl -u nvml-exporter -b --no-pager | tail -20
curl -s http://127.0.0.1:9835/health
```

`serve` is still a stub at this point, so the unit will be `failed` with exit 2 and the journal shows the stub's message: that proves the install enabled and started it. The hardening is tested now anyway: replace the stub check with `sudo systemd-run --unit nvml-spike -p DynamicUser=yes -p SupplementaryGroups=video -p DeviceAllow=/dev/nvidiactl -p DeviceAllow=/dev/nvidia0 -p DeviceAllow=char-nvidia -p DevicePolicy=closed -p ProtectSystem=strict -p MemoryDenyWriteExecute=yes -p NoNewPrivileges=yes --wait --pipe nvml-exporter once | head -3`. Expected: `nvml_up 1`. If `nvml_up 0`: read `journalctl -u nvml-spike`, then retry without `MemoryDenyWriteExecute`, then with a static user; record which combination works and change the unit (and §6) accordingly.

- [ ] **Step 3: Footprint**

```bash
ls -l /usr/bin/nvml-exporter; ldd /usr/bin/nvml-exporter
/usr/bin/time -v nvml-exporter once 2>&1 >/dev/null | grep -E 'Maximum resident|Elapsed'
```

Expected: a few MB RSS, well under 100 ms.

- [ ] **Step 4: Error paths (Review Focus 5)**

Ask James for, with the exporter running later in Task 7's second spike round, `sudo systemctl restart nvidia-persistenced` and a scrape before and after; the `nvml_exporter_scrape_errors_total` and `nvml_up` lines tell whether the collector handled it. Record the outcome; if the driver cannot be made to fail safely on the card, say so in the log and leave the placeholders in `tests/live.rs` as documentation.

- [ ] **Step 5: Record and commit**

Write `docs/spike-log.md` with the date, the deb version, each command and its outcome, and the decisions taken (unit changes, any catalog correction, the `Clock::SM` versus `Graphics` reading if `nvidia-smi` disagrees). Update §6 of `docs/design.md` if the unit changed. Add an Amendments entry per ruling.

```bash
git add docs/spike-log.md docs/design.md packaging docs/superpowers/plans/2026-10-20-nvml-exporter.md
git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit -m "Spike on the V100: NVML fields and unit hardening verified"
```

---

### Task 7: HTTP server, `serve`, signals

**Files:**
- Create: `src/http.rs`
- Modify: `src/main.rs` (replace the `serve` stub), `src/lib.rs` (add `pub mod http;`)
- Test: `tests/http.rs`

**Interfaces:**
- Consumes: `config::AllowList`, `collector::Collector`, `expo::render`, `catalog::Sample`.
- Produces:
  ```rust
  pub struct Options { pub read_timeout: Duration, pub max_request: usize }   // Default: 2 s, 8192
  impl Default for Options { ... }
  pub struct Server<'a> { /* listener, allow, collector, version, options, rejected: u64 */ }
  impl<'a> Server<'a> {
      pub fn bind(listen: SocketAddr, allow: &'a AllowList, collector: &'a mut dyn Collector, version: &'a str, options: Options) -> io::Result<Self>;
      pub fn local_addr(&self) -> SocketAddr;
      pub fn run(&mut self, shutdown: &AtomicBool) -> io::Result<()>;   // returns when shutdown is set; polls accept with 50 ms sleeps
      pub fn handle(&mut self, stream: TcpStream, peer: IpAddr) -> Outcome;   // one connection; pub for tests
  }
  pub enum Outcome { Rejected, Served(u16), BadRequest }
  pub fn parse_request_line(line: &str) -> Option<(&str, &str)>;   // (method, path) with query and authority stripped
  ```
- `main`'s `serve`: `Server::bind` (error → `cannot bind <addr>: <err>` on stderr, exit 1), log the startup line (`version`, listen address, readers, `collect_pcie`), install `ctrlc::set_handler` setting an `Arc<AtomicBool>`, `run`, log `shutting down`, exit 0.

- [ ] **Step 1: Write the failing tests `tests/http.rs`**

```rust
mod common;
use common::FakeCollector;
use nvml_exporter::{catalog::Sample, config::AllowList, http::{parse_request_line, Options, Outcome, Server}};
use std::{io::{Read, Write}, net::{IpAddr, TcpListener, TcpStream}, sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, Arc}, thread, time::Duration};

/// A connected pair: the server-side stream (as if accepted by the Server) and the client.
fn pair() -> (TcpStream, TcpStream) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    (l.accept().unwrap().0, client)
}

fn request(addr: std::net::SocketAddr, raw: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(raw.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

fn spawn(allow: AllowList, options: Options) -> (std::net::SocketAddr, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let h = thread::spawn(move || {
        let mut c = FakeCollector { up: true, samples: vec![Sample::gpu("nvml_gpu_temperature_celsius", 0, 36.0)], calls: Arc::new(AtomicUsize::new(0)) };
        let mut srv = Server::bind("127.0.0.1:0".parse().unwrap(), &allow, &mut c, "0.1.0", options).unwrap();
        tx.send(srv.local_addr()).unwrap();
        srv.run(&stop2).unwrap();
    });
    (rx.recv().unwrap(), stop, h)
}

#[test]
fn http_routes_and_status_codes() {
    let (addr, stop, h) = spawn(AllowList::parse(&[]).unwrap(), Options::default());
    let m = request(addr, "GET /metrics HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(m.starts_with("HTTP/1.0 200 OK\r\n") && m.contains("Content-Type: text/plain; version=0.0.4") && m.contains("\r\n\r\n# HELP nvml_up") && m.contains("nvml_gpu_temperature_celsius{gpu=\"0\"} 36\n"));
    for path in ["/", "/health"] {
        let r = request(addr, &format!("GET {path} HTTP/1.0\r\n\r\n"));
        assert!(r.starts_with("HTTP/1.0 200 OK\r\n") && r.ends_with("\r\n\r\nnvml-exporter 0.1.0, nvml_up 1\n"), "{path}: {r}");
    }
    assert!(request(addr, "GET /nope HTTP/1.0\r\n\r\n").starts_with("HTTP/1.0 404 Not Found\r\n"));
    assert!(request(addr, "POST /metrics HTTP/1.0\r\n\r\n").starts_with("HTTP/1.0 405 Method Not Allowed\r\n"));
    assert!(request(addr, "garbage\r\n\r\n").starts_with("HTTP/1.0 400 Bad Request\r\n"));
    stop.store(true, Ordering::SeqCst); h.join().unwrap();
}

#[test]
fn http_matches_path_ignoring_query_and_authority() {
    assert_eq!(parse_request_line("GET /metrics?x=1 HTTP/1.1"), Some(("GET", "/metrics")));
    assert_eq!(parse_request_line("GET http://198.51.100.1:9835/metrics HTTP/1.1"), Some(("GET", "/metrics")));
    assert_eq!(parse_request_line("GET /metrics"), None);
    assert_eq!(parse_request_line(""), None);
}

#[test]
fn http_idle_client_is_dropped_after_timeout() {
    let (addr, stop, h) = spawn(AllowList::parse(&[]).unwrap(), Options { read_timeout: Duration::from_millis(200), max_request: 8192 });
    let mut idle = TcpStream::connect(addr).unwrap();      // sends nothing
    let started = std::time::Instant::now();
    let r = request(addr, "GET /health HTTP/1.0\r\n\r\n");  // must be served after the idle one times out
    assert!(r.starts_with("HTTP/1.0 200 OK"));
    assert!(started.elapsed() < Duration::from_secs(2));
    let mut buf = [0u8; 1];
    assert_eq!(idle.read(&mut buf).unwrap(), 0);           // server closed it
    stop.store(true, Ordering::SeqCst); h.join().unwrap();
}

#[test]
fn http_oversized_request_is_bad_request() {
    let (addr, stop, h) = spawn(AllowList::parse(&[]).unwrap(), Options { read_timeout: Duration::from_secs(2), max_request: 64 });
    let r = request(addr, &format!("GET /metrics HTTP/1.0\r\nX: {}\r\n\r\n", "a".repeat(100)));
    assert!(r.starts_with("HTTP/1.0 400 Bad Request\r\n"));
    stop.store(true, Ordering::SeqCst); h.join().unwrap();
}

#[test]
fn http_rejects_unlisted_peer_before_reading_and_counts_it() {
    let allow = AllowList::parse(&["192.0.2.1".into()]).unwrap();
    let mut c = FakeCollector { up: true, samples: vec![], calls: Arc::new(AtomicUsize::new(0)) };
    let mut srv = Server::bind("127.0.0.1:0".parse().unwrap(), &allow, &mut c, "0.1.0", Options::default()).unwrap();

    let (accepted, mut client) = pair();
    assert!(matches!(srv.handle(accepted, "192.0.2.53".parse::<IpAddr>().unwrap()), Outcome::Rejected));
    let mut buf = [0u8; 1];
    assert_eq!(client.read(&mut buf).unwrap_or(0), 0);     // closed, nothing written

    let (accepted, mut client) = pair();
    client.write_all(b"GET /metrics HTTP/1.0\r\n\r\n").unwrap();
    assert!(matches!(srv.handle(accepted, "127.0.0.1".parse::<IpAddr>().unwrap()), Outcome::Served(200)));
    let mut out = String::new();
    client.read_to_string(&mut out).unwrap();
    assert!(out.contains("nvml_exporter_rejected_connections_total 1\n"));

    let (accepted, mut client) = pair();
    client.write_all(b"GET /health HTTP/1.0\r\n\r\n").unwrap();
    assert!(matches!(srv.handle(accepted, "192.0.2.1".parse::<IpAddr>().unwrap()), Outcome::Served(200)));
}

#[test]
fn http_health_and_metrics_each_scrape_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let calls2 = calls.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let allow = AllowList::parse(&[]).unwrap();
    let h = thread::spawn(move || {
        let mut c = FakeCollector { up: false, samples: vec![], calls: calls2 };
        let mut srv = Server::bind("127.0.0.1:0".parse().unwrap(), &allow, &mut c, "0.1.0", Options::default()).unwrap();
        tx.send(srv.local_addr()).unwrap();
        srv.run(&stop2).unwrap();
    });
    let addr = rx.recv().unwrap();
    let r = request(addr, "GET /health HTTP/1.0\r\n\r\n");
    assert!(r.ends_with("nvml-exporter 0.1.0, nvml_up 0\n"));
    let _ = request(addr, "GET /metrics HTTP/1.0\r\n\r\n");
    let _ = request(addr, "GET /nope HTTP/1.0\r\n\r\n");   // no scrape for a 404
    stop.store(true, Ordering::SeqCst); h.join().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test http`
Expected: compile error, `http` module missing.

- [ ] **Step 3: Implement `src/http.rs`**

`bind`: `TcpListener::bind`, `set_nonblocking(true)`. `run`: loop `accept()`; `WouldBlock` → sleep 50 ms and check `shutdown`; on a stream, `set_nonblocking(false)`, `set_read_timeout(Some(options.read_timeout))`, `peer_addr()` → `handle(stream, peer.ip())`. `handle`: if `!allow.allows(peer)` → drop the stream, `rejected += 1`, log at `info` the first rejection per peer address and `debug` afterwards (a `HashSet<IpAddr>`), return `Rejected`. Otherwise read into a `Vec<u8>` until `\r\n\r\n` (or `\n\n`), EOF, timeout or `max_request` bytes: timeout/EOF before a complete request line → drop silently (`BadRequest` without writing when nothing was received; `400` when a line was received but is malformed or the request exceeded `max_request`). Route: `/metrics` → collect, append `Sample::global("nvml_exporter_rejected_connections_total", rejected as f64)`, `render`, `200` with `Content-Type: text/plain; version=0.0.4; charset=utf-8`; `/` and `/health` → collect, body `nvml-exporter {version}, nvml_up {0|1}\n` with `Content-Type: text/plain; charset=utf-8`; other path `404`; method other than `GET` `405` (checked before the path). Every response is `HTTP/1.0 <code> <reason>\r\nContent-Type: ...\r\nContent-Length: <n>\r\nConnection: close\r\n\r\n<body>`; error responses carry the reason phrase plus newline as body. Write errors (peer went away) are ignored. `parse_request_line`: split on single spaces into exactly three tokens, the third starting with `HTTP/`; a target starting with `http://` or `https://` loses everything up to the third `/`; the path is cut at the first `?`.

- [ ] **Step 4: Replace the `serve` stub in `src/main.rs`** per the Interfaces block.

- [ ] **Step 5: Run the tests**

Run: `cargo test --test http --test cli`
Expected: PASS.

- [ ] **Step 6: Second spike round (James operates)**

Rebuild the deb (`cargo deb`), hand it over, and ask for, on the VM after `sudo dpkg -i`:

```bash
systemctl status nvml-exporter --no-pager; journalctl -u nvml-exporter -b --no-pager | tail
curl -s http://127.0.0.1:9835/health; curl -s http://127.0.0.1:9835/metrics | grep -E 'nvml_(up|memory_temperature|power_watts)'
systemctl show -p MemoryCurrent nvml-exporter
```

Then with `/etc/nvml-exporter/config.toml` set to `listen = "<VM address>:9835"` and `readers = ["192.0.2.1"]` and `sudo systemctl restart nvml-exporter`: `curl` from reader-host succeeds, `curl --max-time 3` from the sandbox VM (192.0.2.53) fails, loopback `curl` from the VM to its own LAN address is refused (expected per §11.4), `curl -s http://127.0.0.1:9835/` from the VM fails to connect (no loopback socket when `listen` is the LAN address), and the journal shows one `rejected` line for 192.0.2.53. Then the Task 6 step 4 driver-restart check. Record in `docs/spike-log.md`.

- [ ] **Step 7: Lint and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
git add src/http.rs src/main.rs src/lib.rs tests/http.rs tests/common docs/spike-log.md
git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit -m "HTTP server with reader allow-list; serve subcommand"
```

---

### Task 8: CI, release workflow, dependabot, tag script, README

**Files:**
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/dependabot.yml`, `scripts/tag-release.sh`
- Modify: `README.md`

**Interfaces:**
- Produces: workflows that run only once the remote exists (Task 9). Validate the YAML locally with `python3 -c 'import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]' .github/workflows/*.yml .github/dependabot.yml` (install `python3-yaml` if absent, or use `ruby -ryaml`) and `bash -n scripts/tag-release.sh`.

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

Triggers: `push` to `main`, `pull_request`. One job `check`, `runs-on: ubuntu-latest`, `container: debian:trixie`. Steps: `apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git build-essential pkg-config` (git before checkout, so checkout uses git and the container's `safe.directory` is set with `git config --global --add safe.directory "$GITHUB_WORKSPACE"`); `actions/checkout@v7`; install rustup with `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --profile minimal` and `echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"`; `rustup show` (installs the pinned toolchain from `rust-toolchain.toml`); `actions/cache@v5` on `~/.cargo/registry`, `~/.cargo/git`, `target` keyed on `${{ runner.os }}-trixie-${{ hashFiles('rust-toolchain.toml', 'Cargo.lock') }}`; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test --locked`; on `pull_request` only: `cargo install --locked cargo-deb@3.8.0`, `cargo deb`, `dpkg-deb -I target/debian/*.deb`, `actions/upload-artifact@v7` with name `nvml-exporter-deb`.

- [ ] **Step 2: Write `.github/workflows/release.yml`**

Triggers: `push` to `main` and tags `v*`. Workflow-level `permissions: contents: write, id-token: write, attestations: write` (cfab's comment about job-level permissions replacing the block applies; copy it). `concurrency: group: release-${{ github.ref }}`. Two jobs:

`build` (`container: debian:trixie`, same bootstrap as CI): `cargo test --locked`; resolve channel and version exactly as cfab's `meta` step (tag must equal `v<Cargo.toml version>`, dev version `${ver}~dev${GITHUB_RUN_NUMBER}+g${GITHUB_SHA::7}`); `cargo install --locked cargo-deb@3.8.0`; `cargo deb` (formal) or `cargo deb --deb-version ...` (dev); assemble `dist/`: formal = the deb plus `target/release/nvml-exporter` copied as `nvml-exporter-linux-x86_64`; dev = the deb as `nvml-exporter-dev_amd64.deb`; `actions/upload-artifact@v7` name `dist`; job outputs `channel`, `ver`, `deb_version`.

`publish` (`runs-on: ubuntu-latest`, `needs: build`, no container, so `gh` and the attestation action run on the runner): `actions/download-artifact@v7` into `dist/`; `actions/attest-build-provenance@v4` with `subject-path: dist/*`; formal: `sha256sum * > SHA256SUMS` inside `dist/`, `gh release create "$GITHUB_REF_NAME" dist/* --verify-tag --title "nvml-exporter $GITHUB_REF_NAME" --generate-notes`, then the pkg notify step copied from cfab (`continue-on-error: true`, `GH_TOKEN: ${{ secrets.PKG_TRIGGER_TOKEN }}`, the `--ref main` comment included, `gh workflow run check-releases.yml -R shkolnik/pkg --ref main`); dev: cfab's rolling `dev` prerelease steps with the asset `dist/nvml-exporter-dev_amd64.deb` and title `nvml-exporter dev (rolling)`.

- [ ] **Step 3: Write `.github/dependabot.yml` and `scripts/tag-release.sh`**

dependabot: cfab's file verbatim (cargo and github-actions, weekly). `tag-release.sh`: cfab's script with `cfab` → `nvml-exporter` in the lock-file awk, the tag message, and the final URL; `chmod +x`.

- [ ] **Step 4: Write `README.md`**

Sections: what it is (two paragraphs), install from pkg.jshkol.com (`apt install nvml-exporter`, note it is indexed after the first formal release), config (the three keys, the loopback default, the example for a LAN reader), endpoints, `once`/`check` usage, metrics (link `docs/metrics.md`), security posture (read-only, allow-list, no TLS, hardened unit), building and releasing (`cargo deb`, `scripts/tag-release.sh`, the dev channel), license.

- [ ] **Step 5: Validate and commit**

Run: the YAML and bash checks from the Interfaces block; `cargo test --locked` still green.

```bash
git add .github scripts README.md
git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit -m "CI and release workflows (debian:trixie), dependabot, tag script, README"
```

---

### Task 9: Remote, CI green, first formal release (gated on James)

**Files:**
- Modify: `AGENTS.md` (resume note), `Cargo.toml`/`Cargo.lock` only if the version must move.

Stops for James three times. Do not create the remote, set secrets, or merge anything yourself.

- [ ] **Step 1: Wait for the remote**

James creates `github.com/shkolnik/nvml-exporter` (empty, no README) and adds the agent as collaborator, sets the `PKG_TRIGGER_TOKEN` secret and the Actions permission to allow `GITHUB_TOKEN` to write releases (as in cfab). Then: `git remote add origin git@github.com:shkolnik/nvml-exporter.git && git push -u origin main`. Confirm the CI and the `release` (dev channel) runs are green at `https://github.com/shkolnik/nvml-exporter/actions`; fix any container bootstrap issue in a commit on `main` (James has said how PRs work in this repo by then; follow that).

- [ ] **Step 2: Confirm the dev deb**

Ask James to install `nvml-exporter-dev_amd64.deb` from the rolling `dev` release on the VM over the locally built one and paste `nvml-exporter --version`, `systemctl status nvml-exporter --no-pager` and a `/metrics` scrape from reader-host. Expected: same results as the second spike round.

- [ ] **Step 3: Tag v0.1.0**

`scripts/tag-release.sh --dry-run`, then with James's go-ahead `scripts/tag-release.sh`. Verify the formal release has the deb, the bare binary, `SHA256SUMS` and the attestation, and that the pkg repo received a `check-releases` run (James opens or merges the first `.redirect` PR by hand; onboarding is a trust decision, §7).

- [ ] **Step 4: Update the resume note and commit**

`AGENTS.md` §1: released v0.1.0, pkg onboarding state, next = the operator's config repo integration plan (design §9) in the the operator's config repo repo. Commit and push.

---

## Amendments

Rulings made during execution go here, newest last: `Task N: <what> — <why> — <cost if wrong>`.

- (none yet)

## Self-review notes (2026-10-20, author)

Spec coverage: §4.1 CLI → Tasks 5, 7; §4.2 config → Task 3; §4.3 HTTP → Task 7; §4.4 catalog → Tasks 2, 4; §5 behavior → Tasks 4, 7; §6 packaging → Task 5 (unit verified Task 6); §7 repo, lints, CI, release → Tasks 1, 8; §7a ordering → Task 6 placed between 5 and 7; §8 spike → Tasks 6 and 7 step 6; §9 the operator's config repo → out of scope by §7a; §10 builder choices → made in the Interfaces blocks (Sample shape, `env_logger`, `ctrlc`, addresses-only readers, generated `docs/metrics.md`, two-job release so `gh` runs outside the container); §11 → Global Constraints. Type names used across tasks: `Sample`, `Scrape`, `Collector`, `AllowList`, `Options`, `Outcome`, `FakeCollector` are defined once and reused by name. Known weak spots: the no-GPU sandbox cannot exercise NVML error paths (Review Focus 5 is verified by hand in Task 6/7 and documented); the catalog row count in `tests/catalog.rs` must be reconciled against §4.4 by the implementer of Task 2.

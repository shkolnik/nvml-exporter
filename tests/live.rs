//! Tests that need the V100 and its driver; run on the VM with `cargo test -- --ignored`.

use std::process::Command;

fn once_output() -> String {
    let dir = std::env::temp_dir().join(format!("nvml-exporter-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("config.toml");
    std::fs::write(&cfg, "").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_nvml-exporter"))
        .args(["once", "--config"])
        .arg(&cfg)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

fn assert_parses(text: &str) {
    let lines = text.lines().map(|l| Ok(l.to_owned()));
    prometheus_parse::Scrape::parse(lines).expect("exposition parses");
}

#[test]
#[ignore = "needs an NVIDIA GPU and driver"]
fn live_once_reports_up_and_thermals() {
    let text = once_output();
    assert_parses(&text);
    let has = |prefix: &str| text.lines().any(|l| l.starts_with(prefix));
    assert!(text.contains("nvml_up 1\n"), "{text}");
    assert!(text.contains("nvml_device_count 1\n"), "{text}");
    for prefix in [
        "nvml_gpu_info{",
        "nvml_gpu_temperature_celsius{",
        "nvml_memory_temperature_celsius{",
        "nvml_power_watts{",
        "nvml_power_limit_watts{",
        "nvml_energy_joules_total{",
    ] {
        assert!(has(prefix), "missing {prefix}\n{text}");
    }
    let throttle = text
        .lines()
        .filter(|l| l.starts_with("nvml_throttle_reason{"))
        .count();
    assert_eq!(throttle, 9, "{text}");
    assert!(!has("nvml_fan_speed_ratio"), "V100 has no fan\n{text}");
}

/// Placeholder: an NVML error other than `NotSupported` is counted in
/// `nvml_exporter_scrape_errors_total` and its series omitted. Provoking such an error needs
/// the card in a specific state, so this is verified by hand in `docs/spike-log.md`.
#[test]
#[ignore = "needs an NVIDIA GPU and driver; verified by hand in docs/spike-log.md"]
fn collector_reads_other_errors_count_and_omit() {
    assert_parses(&once_output());
}

/// Placeholder: a failing device handle (GPU lost, driver reload) drops the scrape to
/// `nvml_up 0` and re-initializes on the next one. Only the card can fail that way, so this is
/// verified by hand in `docs/spike-log.md`.
#[test]
#[ignore = "needs an NVIDIA GPU and driver; verified by hand in docs/spike-log.md"]
fn collector_device_failure_drops_to_down() {
    assert_parses(&once_output());
}

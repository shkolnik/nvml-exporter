//! Black-box tests of the binary's `--version`, `check` and `once`.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nvml-exporter"))
}

#[test]
fn cli_version() {
    let out = bin().arg("--version").output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        concat!("nvml-exporter ", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn cli_check_prints_resolved_config() {
    let dir = std::env::temp_dir().join(format!("nvml-exporter-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("config.toml");
    std::fs::write(
        &cfg,
        "listen = \"127.0.0.1:19835\"\nreaders = [\"192.0.2.1\"]\n",
    )
    .unwrap();
    let out = bin()
        .args(["check", "--config"])
        .arg(&cfg)
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("listen = 127.0.0.1:19835")
            && text.contains("readers = [192.0.2.1/32]")
            && text.contains("collect_pcie = false")
    );
}

#[test]
fn cli_check_missing_config_fails_with_path() {
    let out = bin()
        .args(["check", "--config", "/nonexistent/config.toml"])
        .output()
        .unwrap();
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
    if text.contains("nvml_up 1") {
        return; // driver present on this host
    }
    assert_eq!(out.status.code(), Some(1));
    assert!(text.contains("nvml_up 0\n"));
}

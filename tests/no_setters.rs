const SETTERS: &[&str] = &[
    "clear_accounting_pids",
    "clear_cpu_affinity",
    "clear_ecc_error_counts",
    "reset_applications_clocks",
    "reset_gpu_locked_clocks",
    "reset_mem_locked_clocks",
    "set_accounting",
    "set_api_restricted",
    "set_applications_clocks",
    "set_auto_boosted_clocks",
    "set_auto_boosted_clocks_default",
    "set_clock_offset",
    "set_compute_mode",
    "set_confidential_compute_state",
    "set_cpu_affinity",
    "set_default_fan_speed",
    "set_drain",
    "set_driver_model",
    "set_ecc",
    "set_fan_control_policy",
    "set_fan_speed",
    "set_gpc_clock_vf_offset",
    "set_gpm_streaming_enabled",
    "set_gpu_locked_clocks",
    "set_gpu_op_mode",
    "set_mem_clock_vf_offset",
    "set_mem_locked_clocks",
    "set_mig_mode",
    "set_persistent",
    "set_power_management_limit",
    "set_power_mizer_mode",
    "set_temperature_threshold",
    "set_vgpu_scheduler_state",
];

#[test]
fn clippy_toml_disallows_every_setter() {
    let toml =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/clippy.toml")).unwrap();
    for name in SETTERS {
        assert!(
            toml.contains(&format!("nvml_wrapper::Device::{name}\"")),
            "clippy.toml lacks {name}"
        );
    }
    assert_eq!(SETTERS.len(), 33);
}

#[test]
fn sources_never_call_a_setter() {
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for name in SETTERS {
            assert!(
                !text.contains(&format!("{name}(")),
                "{} calls {name}",
                path.display()
            );
        }
    }
}

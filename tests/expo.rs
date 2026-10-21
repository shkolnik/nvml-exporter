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
    assert_eq!(out, expected); // catalog order, not sample order
}

#[test]
fn expo_escapes_label_values() {
    let s = Sample::global("nvml_gpu_info", 1.0).with_label("name", "a\"b\\c\nd");
    assert!(render(&[s]).contains(r#"name="a\"b\\c\nd""#));
}

#[test]
fn expo_parses_back_with_prometheus_parse() {
    let samples = vec![
        Sample::global("nvml_up", 0.0),
        Sample::global("nvml_device_count", 0.0),
    ];
    let text = render(&samples);
    let scrape = prometheus_parse::Scrape::parse(text.lines().map(|l| Ok(l.to_owned()))).unwrap();
    assert_eq!(scrape.samples.len(), 2);
}

#[test]
#[should_panic(expected = "not in CATALOG")]
fn expo_rejects_unknown_metric() {
    render(&[Sample::global("nvml_bogus", 1.0)]);
}

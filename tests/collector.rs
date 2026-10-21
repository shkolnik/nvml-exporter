mod common;

use nvml_exporter::collector::{Collector, NvmlCollector};

#[test]
fn collector_without_driver_reports_down_and_keeps_serving() {
    // This sandbox has no libnvidia-ml.so.1; on a host that has one this test is skipped.
    let mut c = NvmlCollector::new(false);
    let s1 = c.collect();
    if s1.up {
        eprintln!("driver present, skipping");
        return;
    }
    let names: Vec<&str> = s1.samples.iter().map(|x| x.name).collect();
    assert!(
        names.contains(&"nvml_up")
            && names.contains(&"nvml_device_count")
            && names.contains(&"nvml_exporter_build_info")
            && names.contains(&"nvml_exporter_scrape_duration_seconds")
    );
    assert!(!names.contains(&"nvml_driver_info"));
    let up = s1
        .samples
        .iter()
        .find(|x| x.name == "nvml_up")
        .unwrap()
        .value;
    let count = s1
        .samples
        .iter()
        .find(|x| x.name == "nvml_device_count")
        .unwrap()
        .value;
    assert_eq!((up, count), (0.0, 0.0));
    let s2 = c.collect(); // retries init, still down, does not panic
    assert!(!s2.up);
}

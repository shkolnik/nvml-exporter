//! The NVML collector (design §4.4, §5): one scrape reads every GPU through the read-only NVML API.
//! NVML is initialized lazily and re-initialized on the next scrape after any failure.

use std::collections::BTreeMap;
use std::time::Instant;

use nvml_wrapper::Nvml;
use nvml_wrapper::bitmasks::device::ThrottleReasons;
use nvml_wrapper::enum_wrappers::device::{Clock, PcieUtilCounter, TemperatureSensor};
use nvml_wrapper::enums::device::SampleValue;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::structs::device::FieldId;
use nvml_wrapper::sys_exports::field_id::NVML_FI_DEV_MEMORY_TEMP;

use crate::catalog::Sample;

#[derive(Debug, Clone, PartialEq)]
pub struct Scrape {
    pub up: bool,
    pub samples: Vec<Sample>,
}

pub trait Collector {
    fn collect(&mut self) -> Scrape;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ErrorCounts(BTreeMap<&'static str, u64>);

impl ErrorCounts {
    fn record(&mut self, metric: &'static str) {
        *self.0.entry(metric).or_insert(0) += 1;
    }

    fn samples(&self) -> impl Iterator<Item = Sample> + '_ {
        self.0.iter().map(|(metric, n)| {
            Sample::global("nvml_exporter_scrape_errors_total", *n as f64)
                .with_label("metric", *metric)
        })
    }
}

pub const THROTTLE_REASONS: &[(&str, ThrottleReasons)] = &[
    ("gpu_idle", ThrottleReasons::GPU_IDLE),
    (
        "applications_clocks_setting",
        ThrottleReasons::APPLICATIONS_CLOCKS_SETTING,
    ),
    ("sw_power_cap", ThrottleReasons::SW_POWER_CAP),
    ("hw_slowdown", ThrottleReasons::HW_SLOWDOWN),
    ("sync_boost", ThrottleReasons::SYNC_BOOST),
    ("sw_thermal_slowdown", ThrottleReasons::SW_THERMAL_SLOWDOWN),
    ("hw_thermal_slowdown", ThrottleReasons::HW_THERMAL_SLOWDOWN),
    (
        "hw_power_brake_slowdown",
        ThrottleReasons::HW_POWER_BRAKE_SLOWDOWN,
    ),
    (
        "display_clock_setting",
        ThrottleReasons::DISPLAY_CLOCK_SETTING,
    ),
];

pub fn throttle_samples(gpu: u32, reasons: ThrottleReasons) -> Vec<Sample> {
    THROTTLE_REASONS
        .iter()
        .map(|(name, bit)| {
            let v = if reasons.contains(*bit) { 1.0 } else { 0.0 };
            Sample::gpu("nvml_throttle_reason", gpu, v).with_label("reason", *name)
        })
        .collect()
}

pub fn milli_to_unit(v: u32) -> f64 {
    f64::from(v) / 1000.0
}

pub fn mhz_to_hz(v: u32) -> f64 {
    f64::from(v) * 1e6
}

pub fn percent_to_ratio(v: u32) -> f64 {
    f64::from(v) / 100.0
}

fn bool_value(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

fn sample_value_to_f64(v: &SampleValue) -> f64 {
    match *v {
        SampleValue::F64(x) => x,
        SampleValue::U32(x) => f64::from(x),
        SampleValue::U64(x) => x as f64,
        SampleValue::I64(x) => x as f64,
    }
}

fn read<T>(errors: &mut ErrorCounts, metric: &'static str, r: Result<T, NvmlError>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(NvmlError::NotSupported) => None,
        Err(e) => {
            errors.record(metric);
            log::debug!("{metric}: {e}");
            None
        }
    }
}

fn memory_temperature(errors: &mut ErrorCounts, device: &nvml_wrapper::Device<'_>) -> Option<f64> {
    const METRIC: &str = "nvml_memory_temperature_celsius";
    let fields = read(
        errors,
        METRIC,
        device.field_values_for(&[FieldId(NVML_FI_DEV_MEMORY_TEMP)]),
    )?;
    let sample = read(errors, METRIC, fields.into_iter().next()?)?;
    read(errors, METRIC, sample.value)
        .as_ref()
        .map(sample_value_to_f64)
}

fn gauge<T>(
    errors: &mut ErrorCounts,
    out: &mut Vec<Sample>,
    gpu: u32,
    metric: &'static str,
    r: Result<T, NvmlError>,
    convert: impl FnOnce(T) -> f64,
) {
    if let Some(v) = read(errors, metric, r) {
        out.push(Sample::gpu(metric, gpu, convert(v)));
    }
}

fn identity_sample(errors: &mut ErrorCounts, device: &nvml_wrapper::Device<'_>, i: u32) -> Sample {
    const INFO: &str = "nvml_gpu_info";
    let uuid = read(errors, INFO, device.uuid()).unwrap_or_default();
    let name = read(errors, INFO, device.name()).unwrap_or_default();
    let bus_id = read(errors, INFO, device.pci_info())
        .map(|p| p.bus_id)
        .unwrap_or_default();
    let vbios = read(errors, INFO, device.vbios_version()).unwrap_or_default();
    Sample::gpu(INFO, i, 1.0)
        .with_label("uuid", uuid)
        .with_label("name", name)
        .with_label("pci_bus_id", bus_id)
        .with_label("vbios_version", vbios)
}

fn device_samples(
    errors: &mut ErrorCounts,
    device: &nvml_wrapper::Device<'_>,
    i: u32,
    collect_pcie: bool,
    out: &mut Vec<Sample>,
) {
    out.push(identity_sample(errors, device, i));
    let temp = device.temperature(TemperatureSensor::Gpu);
    gauge(
        errors,
        out,
        i,
        "nvml_gpu_temperature_celsius",
        temp,
        f64::from,
    );
    if let Some(v) = memory_temperature(errors, device) {
        out.push(Sample::gpu("nvml_memory_temperature_celsius", i, v));
    }
    let power = device.power_usage();
    gauge(errors, out, i, "nvml_power_watts", power, milli_to_unit);
    let limit = device.enforced_power_limit();
    gauge(
        errors,
        out,
        i,
        "nvml_power_limit_watts",
        limit,
        milli_to_unit,
    );
    let energy = device.total_energy_consumption();
    gauge(errors, out, i, "nvml_energy_joules_total", energy, |mj| {
        mj as f64 / 1000.0
    });
    let sm = device.clock_info(Clock::SM);
    gauge(errors, out, i, "nvml_sm_clock_hertz", sm, mhz_to_hz);
    let mem_clock = device.clock_info(Clock::Memory);
    gauge(
        errors,
        out,
        i,
        "nvml_memory_clock_hertz",
        mem_clock,
        mhz_to_hz,
    );
    if let Some(u) = read(
        errors,
        "nvml_utilization_gpu_ratio",
        device.utilization_rates(),
    ) {
        out.push(Sample::gpu(
            "nvml_utilization_gpu_ratio",
            i,
            percent_to_ratio(u.gpu),
        ));
        out.push(Sample::gpu(
            "nvml_utilization_memory_ratio",
            i,
            percent_to_ratio(u.memory),
        ));
    }
    if let Some(mem) = read(errors, "nvml_memory_used_bytes", device.memory_info()) {
        out.push(Sample::gpu("nvml_memory_used_bytes", i, mem.used as f64));
        out.push(Sample::gpu("nvml_memory_total_bytes", i, mem.total as f64));
    }
    if let Some(r) = read(
        errors,
        "nvml_throttle_reason",
        device.current_throttle_reasons(),
    ) {
        out.extend(throttle_samples(i, r));
    }
    if collect_pcie {
        const PCIE: &str = "nvml_pcie_throughput_bytes_per_second";
        for (counter, direction) in [
            (PcieUtilCounter::Send, "tx"),
            (PcieUtilCounter::Receive, "rx"),
        ] {
            if let Some(kbs) = read(errors, PCIE, device.pcie_throughput(counter)) {
                out.push(
                    Sample::gpu(PCIE, i, f64::from(kbs) * 1000.0)
                        .with_label("direction", direction),
                );
            }
        }
    }
    let fan = device.fan_speed(0);
    gauge(
        errors,
        out,
        i,
        "nvml_fan_speed_ratio",
        fan,
        percent_to_ratio,
    );
    let persistence = device.is_in_persistent_mode();
    gauge(
        errors,
        out,
        i,
        "nvml_persistence_mode",
        persistence,
        bool_value,
    );
}

pub struct NvmlCollector {
    nvml: Option<Nvml>,
    collect_pcie: bool,
    errors: ErrorCounts,
    log_state: LogState,
}

impl NvmlCollector {
    pub fn new(collect_pcie: bool) -> Self {
        NvmlCollector {
            nvml: None,
            collect_pcie,
            errors: ErrorCounts::default(),
            log_state: LogState::default(),
        }
    }

    fn ensure_init(&mut self) -> Option<String> {
        if self.nvml.is_some() {
            return None;
        }
        match Nvml::init() {
            Ok(nvml) => {
                self.nvml = Some(nvml);
                None
            }
            Err(e) => Some(format!("init failed: {e}")),
        }
    }

    fn shutdown(&mut self) {
        if let Some(n) = self.nvml.take() {
            let _ = n.shutdown();
        }
    }

    fn finish(&self, up: bool, mut samples: Vec<Sample>, start: Instant) -> Scrape {
        if !up {
            samples.clear();
            samples.push(Sample::global("nvml_up", 0.0));
            samples.push(Sample::global("nvml_device_count", 0.0));
        }
        samples.push(
            Sample::global("nvml_exporter_build_info", 1.0)
                .with_label("version", env!("CARGO_PKG_VERSION")),
        );
        samples.push(Sample::global(
            "nvml_exporter_scrape_duration_seconds",
            start.elapsed().as_secs_f64(),
        ));
        samples.extend(self.errors.samples());
        Scrape { up, samples }
    }
}

impl Collector for NvmlCollector {
    fn collect(&mut self) -> Scrape {
        let start = Instant::now();
        if let Some(why) = self.ensure_init() {
            self.log_transition(false, &why, &[]);
            return self.finish(false, Vec::new(), start);
        }
        let Some(nvml) = self.nvml.as_ref() else {
            return self.finish(false, Vec::new(), start);
        };
        let errors = &mut self.errors;
        let mut samples = Vec::new();
        let mut device_failure: Option<NvmlError> = None;
        match nvml.device_count() {
            Ok(count) => {
                samples.push(Sample::global("nvml_up", 1.0));
                samples.push(Sample::global("nvml_device_count", f64::from(count)));
                if let Some(v) = read(errors, "nvml_driver_info", nvml.sys_driver_version()) {
                    samples.push(Sample::global("nvml_driver_info", 1.0).with_label("version", v));
                }
                for i in 0..count {
                    match nvml.device_by_index(i) {
                        Ok(device) => {
                            device_samples(errors, &device, i, self.collect_pcie, &mut samples);
                        }
                        Err(e) => {
                            device_failure = Some(e);
                            break;
                        }
                    }
                }
            }
            Err(e) => device_failure = Some(e),
        }
        if let Some(e) = device_failure {
            self.shutdown();
            self.log_transition(false, &e.to_string(), &[]);
            return self.finish(false, Vec::new(), start);
        }
        self.log_transition(true, "", &samples);
        self.finish(true, samples, start)
    }
}

/// What has last been logged about NVML availability, so state changes are logged once.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LogState {
    #[default]
    Unknown,
    Up,
    Down,
}

impl LogState {
    /// Records the outcome of a scrape; true when it is a state change that must be logged.
    pub fn observe(&mut self, up: bool) -> bool {
        let next = if up { LogState::Up } else { LogState::Down };
        let changed = *self != next;
        *self = next;
        changed
    }
}

impl NvmlCollector {
    fn log_transition(&mut self, up: bool, why: &str, samples: &[Sample]) {
        if !self.log_state.observe(up) {
            return;
        }
        if !up {
            log::warn!("NVML down: {why}");
            return;
        }
        let label = |s: &Sample, key: &str| {
            s.labels
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let driver = samples
            .iter()
            .find(|s| s.name == "nvml_driver_info")
            .map(|s| label(s, "version"))
            .unwrap_or_default();
        let names: Vec<String> = samples
            .iter()
            .filter(|s| s.name == "nvml_gpu_info")
            .map(|s| label(s, "name"))
            .collect();
        log::info!("NVML up: driver {driver}, devices {names:?}");
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // the conversions under test are exact for these inputs
mod tests {
    use super::*;
    use nvml_wrapper::bitmasks::device::ThrottleReasons;

    #[test]
    fn throttle_bits_become_nine_zero_one_series() {
        let s = throttle_samples(
            0,
            ThrottleReasons::SW_POWER_CAP | ThrottleReasons::HW_SLOWDOWN,
        );
        assert_eq!(s.len(), 9);
        let on: Vec<&str> = s
            .iter()
            .filter(|x| x.value == 1.0)
            .map(|x| x.labels[1].1.as_str())
            .collect();
        assert_eq!(on, ["sw_power_cap", "hw_slowdown"]);
        assert!(
            s.iter()
                .all(|x| x.name == "nvml_throttle_reason" && x.labels[0] == ("gpu", "0".into()))
        );
        assert!(
            throttle_samples(0, ThrottleReasons::empty())
                .iter()
                .all(|x| x.value == 0.0)
        );
    }

    #[test]
    fn log_state_logs_each_transition_once() {
        let mut st = LogState::default();
        assert!(st.observe(false), "first down warns");
        assert!(
            !st.observe(false),
            "init ok then device failure: still down, silent"
        );
        assert!(!st.observe(false));
        assert!(st.observe(true), "first full scrape after down logs up");
        assert!(!st.observe(true));
        assert!(st.observe(false));
        let mut fresh = LogState::default();
        assert!(fresh.observe(true), "first successful scrape logs up");
    }

    #[test]
    fn unit_conversions() {
        assert_eq!(milli_to_unit(150_000), 150.0);
        assert_eq!(mhz_to_hz(1380), 1_380_000_000.0);
        assert_eq!(percent_to_ratio(37), 0.37);
    }
}

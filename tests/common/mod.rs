//! Test doubles shared by integration tests.
#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nvml_exporter::catalog::Sample;
use nvml_exporter::collector::{Collector, Scrape};

pub struct FakeCollector {
    pub up: bool,
    pub samples: Vec<Sample>,
    pub calls: Arc<AtomicUsize>,
}

impl Collector for FakeCollector {
    fn collect(&mut self) -> Scrape {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut samples = vec![Sample::global("nvml_up", if self.up { 1.0 } else { 0.0 })];
        samples.extend(self.samples.iter().cloned());
        Scrape {
            up: self.up,
            samples,
        }
    }
}

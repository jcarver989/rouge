#![allow(dead_code)]

use rouge_bench::metrics::{Metrics, SPREAD};
use std::error::Error;

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[derive(Default)]
pub(crate) struct MetricsBuilder {
    metrics: Metrics,
}

impl MetricsBuilder {
    pub(crate) fn set(mut self, key: &str, value: f64) -> Self {
        self.metrics.insert(key.to_owned(), value);
        self
    }

    pub(crate) fn time(self, key: &str, value: f64, spread: f64) -> Self {
        self.set(key, value).set(&format!("{key}{SPREAD}"), spread)
    }

    pub(crate) fn build(self) -> Metrics {
        self.metrics
    }
}

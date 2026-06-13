//!OpenTelemetry integration for [metrics](https://crates.io/crates/metrics).
//!
//! MSRV 1.85
//!
//! ## Usage
//!
//!Example of initialization using in memory exporter
//!
//!```rust
//!use core::time;
//!
//!use metrics_opentelemetry::{metrics, OpenTelemetryMetrics, OpenTelemetryRecorder};
//!use metrics_opentelemetry::opentelemetry::metrics::MeterProvider;
//!use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
//!use opentelemetry_sdk::metrics::data::{MetricData, AggregatedMetrics, ResourceMetrics};
//!
//!let exporter = InMemoryMetricExporter::default();
//!let reader = PeriodicReader::builder(exporter.clone()).with_interval(time::Duration::from_millis(100)).build();
//!let provider = SdkMeterProvider::builder().with_reader(reader).build();
//!let meter = provider.meter("app");
//!let metrics = OpenTelemetryMetrics::new(meter);
//!let recorder = OpenTelemetryRecorder::new(metrics);
//!
//!// After installing recorder you're good to use `metrics` crate
//!metrics::set_global_recorder(recorder);
//!
//!metrics::describe_counter!("requests_total", metrics::Unit::Count, "Total number of requests");
//!let get_requests = metrics::counter!("requests_total", "method" => "GET", "status" => "200");
//!let post_requests = metrics::counter!("requests_total", "method" => "POST", "status" => "201");
//!
//!get_requests.increment(1);
//!post_requests.increment(2);
//!```

#![warn(missing_docs)]
#![allow(clippy::style)]

mod identity;
mod otel;
pub use otel::OpenTelemetryMetrics;

pub use metrics;
pub use opentelemetry;

///Metrics recorder
pub struct OpenTelemetryRecorder {
    metrics: OpenTelemetryMetrics,
}

impl OpenTelemetryRecorder {
    #[inline]
    ///Creates new instance from initialized opentelemetry metrics
    pub fn new(metrics: OpenTelemetryMetrics) -> Self {
        Self {
            metrics,
        }
    }

    #[inline]
    ///Sets boundaries for set of histograms with specified key names.
    ///
    ///Implies lock, if you have full control, then prefer `*_mut` version
    pub fn set_histogram_bounds(&self, keys: impl Iterator<Item = impl Into<metrics::KeyName>>, bounds: &[f64]) {
        let mut histograms = self.metrics.metadata.histogram.write();
        for key in keys {
            histograms.entry(key.into()).or_default().set_bounds(bounds.to_vec());
        }
    }

    #[inline]
    ///Sets boundaries for set of histograms with specified key names.
    pub fn set_histogram_bounds_mut(&mut self, keys: impl Iterator<Item = impl Into<metrics::KeyName>>, bounds: &[f64]) {
        let histograms = self.metrics.metadata.histogram.get_mut();
        for key in keys {
            histograms.entry(key.into()).or_default().set_bounds(bounds.to_vec());
        }
    }
}

impl metrics::Recorder for OpenTelemetryRecorder {
    #[inline]
    fn describe_counter(&self, key: metrics::KeyName, unit: Option<metrics::Unit>, description: metrics::SharedString) {
        let description = otel::Metadata::from_metrics(description, unit);
        self.metrics.metadata.counter.write().insert(key, description);
    }
    #[inline]
    fn register_counter(&self, key: &metrics::Key, _metadata: &metrics::Metadata<'_>) -> metrics::Counter {
        self.metrics.get_or_create_counter(key)
    }

    #[inline]
    fn describe_gauge(&self, key: metrics::KeyName, unit: Option<metrics::Unit>, description: metrics::SharedString) {
        let description = otel::Metadata::from_metrics(description, unit);
        self.metrics.metadata.gauge.write().insert(key, description);
    }
    #[inline]
    fn register_gauge(&self, key: &metrics::Key, _metadata: &metrics::Metadata<'_>) -> metrics::Gauge {
        self.metrics.get_or_create_gauge(key)
    }

    #[inline]
    fn describe_histogram(&self, key: metrics::KeyName, unit: Option<metrics::Unit>, description: metrics::SharedString) {
        let description = otel::Metadata::from_metrics(description, unit);
        self.metrics.metadata.histogram.write().insert(key, description.into());
    }
    #[inline]
    fn register_histogram(&self, key: &metrics::Key, _metadata: &metrics::Metadata<'_>) -> metrics::Histogram {
        self.metrics.get_or_create_histogram(key)
    }
}

# metrics-opentelemetry

[![Rust](https://github.com/DoumanAsh/metrics-opentelemetry/actions/workflows/rust.yml/badge.svg)](https://github.com/DoumanAsh/metrics-opentelemetry/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/metrics-opentelemetry.svg)](https://crates.io/crates/metrics-opentelemetry)
[![Documentation](https://docs.rs/metrics-opentelemetry/badge.svg)](https://docs.rs/crate/metrics-opentelemetry/)
[![dependency status](https://deps.rs/crate/metrics-opentelemetry/0.24.0/status.svg)](https://deps.rs/crate/metrics-opentelemetry/0.24.0)

OpenTelemetry integration for [metrics](https://crates.io/crates/metrics).

 MSRV 1.85

 ## Usage

Example of initialization using in memory exporter

```rust
use core::time;

use metrics_opentelemetry::{metrics, OpenTelemetryMetrics, OpenTelemetryRecorder};
use metrics_opentelemetry::opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::metrics::data::{MetricData, AggregatedMetrics, ResourceMetrics};

let exporter = InMemoryMetricExporter::default();
let reader = PeriodicReader::builder(exporter.clone()).with_interval(time::Duration::from_millis(100)).build();
let provider = SdkMeterProvider::builder().with_reader(reader).build();
let meter = provider.meter("app");
let metrics = OpenTelemetryMetrics::new(meter);
let recorder = OpenTelemetryRecorder::new(metrics);

// After installing recorder you're good to use `metrics` crate
metrics::set_global_recorder(recorder);

metrics::describe_counter!("requests_total", metrics::Unit::Count, "Total number of requests");
let get_requests = metrics::counter!("requests_total", "method" => "GET", "status" => "200");
let post_requests = metrics::counter!("requests_total", "method" => "POST", "status" => "201");

get_requests.increment(1);
post_requests.increment(2);
```

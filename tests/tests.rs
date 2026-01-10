use core::time;

use opentelemetry::{Key, Value};
use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::metrics::data::{MetricData, AggregatedMetrics, ResourceMetrics};
use metrics_opentelemetry::{OpenTelemetryMetrics, OpenTelemetryRecorder};

struct Fixture {
    exporter: InMemoryMetricExporter,
    provider: SdkMeterProvider,
    recorder: OpenTelemetryRecorder,
}

impl Fixture {
    fn new(name: &'static str) -> Self {
        let exporter = InMemoryMetricExporter::default();
        let reader = PeriodicReader::builder(exporter.clone()).with_interval(time::Duration::from_millis(100)).build();
        let provider = SdkMeterProvider::builder().with_reader(reader).build();
        let meter = provider.meter(name);
        let metrics = OpenTelemetryMetrics::new(meter);
        let recorder = OpenTelemetryRecorder::new(metrics);

        Self {
            exporter,
            provider,
            recorder
        }
    }

    #[track_caller]
    #[inline]
    fn force_export_metrics(&self) -> Vec<ResourceMetrics> {
        self.provider.force_flush().expect("to flush metrics");
        self.exporter.get_finished_metrics().expect("to export metrics")
    }

    pub fn init_local_recorder(&self) -> metrics::LocalRecorderGuard<'_> {
        metrics::set_default_local_recorder(&self.recorder)
    }
}

#[test]
fn should_verify_counters_collection() {
    let fixture = Fixture::new("counters_meter");
    let _guard = fixture.init_local_recorder();

    metrics::describe_counter!("requests_total", metrics::Unit::Count, "Total number of requests");
    metrics::counter!("requests_total", "method" => "GET", "status" => "200").increment(1);
    metrics::counter!("requests_total", "method" => "POST", "status" => "201").increment(2);

    let metrics = fixture.force_export_metrics();
    let requests_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|scope| scope.metrics())
        .find(|metric| metric.name() == "requests_total")
        .expect("requests_total metric should exist");

    let sum = match requests_metric.data() {
        AggregatedMetrics::U64(MetricData::Sum(sum)) => sum,
        unexpected => panic!("Expected u64 sum, but got {:?}", unexpected),
    };

    let data_points: Vec<_> = sum.data_points().collect();
    assert_eq!(data_points.len(), 2, "Should have 2 data points for different label combinations");

    let get_point = data_points
        .iter()
        .find(|data_point| {
            data_point.attributes().any(|attr| attr.key == Key::from("method") && attr.value == Value::from("GET"))
        })
        .expect("Should have GET data point");
    assert_eq!(get_point.value(), 1, "GET counter should be 1");

    let post_point = data_points
        .iter()
        .find(|data_point| {
            data_point.attributes().any(|attr| attr.key == Key::from("method") && attr.value == Value::from("POST"))
        })
        .expect("Should have POST data point");
    assert_eq!(post_point.value(), 2, "POST counter should be 2");

    //Modify counters
    metrics::counter!("requests_total", "method" => "GET", "status" => "200").increment(2);
    metrics::counter!("requests_total", "method" => "POST", "status" => "201").absolute(0);

    let metrics = fixture.force_export_metrics();
    let requests_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|scope| scope.metrics())
        .find(|metric| metric.name() == "requests_total")
        .expect("requests_total metric should exist");

    let sum = match requests_metric.data() {
        AggregatedMetrics::U64(MetricData::Sum(sum)) => sum,
        unexpected => panic!("Expected u64 sum, but got {:?}", unexpected),
    };

    let data_points: Vec<_> = sum.data_points().collect();
    assert_eq!(data_points.len(), 2, "Should have 2 data points for different label combinations");

    let get_point = data_points
        .iter()
        .find(|data_point| {
            data_point.attributes().any(|attr| attr.key == Key::from("method") && attr.value == Value::from("GET"))
        })
        .expect("Should have GET data point");
    assert_eq!(get_point.value(), 3, "GET counter should be 1+2");

    let post_point = data_points
        .iter()
        .find(|data_point| {
            data_point.attributes().any(|attr| attr.key == Key::from("method") && attr.value == Value::from("POST"))
        })
        .expect("Should have POST data point");
    assert_eq!(post_point.value(), 0, "POST counter should be reset to 0");
}

#[test]
fn should_verify_gauges_collection() {
    let fixture = Fixture::new("counters_meter");
    let _guard = fixture.init_local_recorder();

    metrics::describe_counter!("requests_ongoing", metrics::Unit::Count, "Total number of requests");
    metrics::gauge!("requests_ongoing", "method" => "GET", "status" => "200").set(1.0);
    metrics::gauge!("requests_ongoing", "method" => "POST", "status" => "201").set(2.0);

    let metrics = fixture.force_export_metrics();
    let requests_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|scope| scope.metrics())
        .find(|metric| metric.name() == "requests_ongoing")
        .expect("requests_ongoing metric should exist");

    let sum = match requests_metric.data() {
        AggregatedMetrics::F64(MetricData::Gauge(sum)) => sum,
        unexpected => panic!("Expected f64 gauge, but got {:?}", unexpected),
    };

    let data_points: Vec<_> = sum.data_points().collect();
    assert_eq!(data_points.len(), 2, "Should have 2 data points for different label combinations");

    let get_point = data_points
        .iter()
        .find(|data_point| {
            data_point.attributes().any(|attr| attr.key == Key::from("method") && attr.value == Value::from("GET"))
        })
        .expect("Should have GET data point");
    assert_eq!(get_point.value(), 1.0, "GET counter should be 1");

    let post_point = data_points
        .iter()
        .find(|data_point| {
            data_point.attributes().any(|attr| attr.key == Key::from("method") && attr.value == Value::from("POST"))
        })
        .expect("Should have POST data point");
    assert_eq!(post_point.value(), 2.0, "POST counter should be 2");

    //Modify counters
    metrics::gauge!("requests_ongoing", "method" => "GET", "status" => "200").increment(1.0);
    metrics::gauge!("requests_ongoing", "method" => "POST", "status" => "201").decrement(1.0);

    let metrics = fixture.force_export_metrics();
    let requests_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|scope| scope.metrics())
        .find(|metric| metric.name() == "requests_ongoing")
        .expect("requests_ongoing metric should exist");

    let sum = match requests_metric.data() {
        AggregatedMetrics::F64(MetricData::Gauge(sum)) => sum,
        unexpected => panic!("Expected f64 gauge, but got {:?}", unexpected),
    };

    let data_points: Vec<_> = sum.data_points().collect();
    assert_eq!(data_points.len(), 2, "Should have 2 data points for different label combinations");

    let get_point = data_points
        .iter()
        .find(|data_point| {
            data_point.attributes().any(|attr| attr.key == Key::from("method") && attr.value == Value::from("GET"))
        })
        .expect("Should have GET data point");
    assert_eq!(get_point.value(), 2.0, "GET counter should be incremented to 2");

    let post_point = data_points
        .iter()
        .find(|data_point| {
            data_point.attributes().any(|attr| attr.key == Key::from("method") && attr.value == Value::from("POST"))
        })
        .expect("Should have POST data point");
    assert_eq!(post_point.value(), 1.0, "POST counter should be decremented to 1");
}

#[test]
fn should_verify_histogram_collection() {
    let fixture = Fixture::new("counters_meter");
    let _guard = fixture.init_local_recorder();

    metrics::describe_counter!("requests_time", metrics::Unit::Seconds, "Request processing time");
    metrics::histogram!("requests_time", "path" => "/users").record(0.225);
    metrics::histogram!("requests_time", "path" => "/users").record(0.775);
    metrics::histogram!("requests_time", "path" => "/posts").record(1.225);

    let metrics = fixture.force_export_metrics();
    let requests_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|scope| scope.metrics())
        .find(|metric| metric.name() == "requests_time")
        .expect("requests_ongoing metric should exist");

    let sum = match requests_metric.data() {
        AggregatedMetrics::F64(MetricData::Histogram(sum)) => sum,
        unexpected => panic!("Expected f64 histogram, but got {:?}", unexpected),
    };

    let data_points: Vec<_> = sum.data_points().collect();
    assert_eq!(data_points.len(), 2, "Should have 2 data points for different label combinations");

    let users_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes()
                .any(|a| a.key == Key::from("path") && a.value == Value::from("/users"))
        })
        .expect("Should have /users data point");
    assert_eq!(users_point.count(), 2, "/users should have 2 recordings");
    assert_eq!(users_point.sum(), 1.0, "/users sum should be 1.0");

    let posts_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes()
                .any(|a| a.key == Key::from("path") && a.value == Value::from("/posts"))
        })
        .expect("Should have /posts data point");
    assert_eq!(posts_point.count(), 1, "/posts should have 1 recording");
    assert_eq!(posts_point.sum(), 1.225, "/posts sum should be 1.225");

    //Modify data
    metrics::histogram!("requests_time", "path" => "/users").record(0.5);
    metrics::histogram!("requests_time", "path" => "/users").record(0.5);
    metrics::histogram!("requests_time", "path" => "/posts").record(1.775);

    let metrics = fixture.force_export_metrics();
    let requests_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|scope| scope.metrics())
        .find(|metric| metric.name() == "requests_time")
        .expect("requests_ongoing metric should exist");

    let sum = match requests_metric.data() {
        AggregatedMetrics::F64(MetricData::Histogram(sum)) => sum,
        unexpected => panic!("Expected f64 histogram, but got {:?}", unexpected),
    };

    let data_points: Vec<_> = sum.data_points().collect();
    assert_eq!(data_points.len(), 2, "Should have 2 data points for different label combinations");

    let users_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes()
                .any(|a| a.key == Key::from("path") && a.value == Value::from("/users"))
        })
        .expect("Should have /users data point");
    assert_eq!(users_point.count(), 4, "/users should have 2 recordings");
    assert_eq!(users_point.sum(), 2.0, "/users sum should be 1.0");

    let posts_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes()
                .any(|a| a.key == Key::from("path") && a.value == Value::from("/posts"))
        })
        .expect("Should have /posts data point");
    assert_eq!(posts_point.count(), 2, "/posts should have 1 recording");
    assert_eq!(posts_point.sum(), 3.0, "/posts sum should be 1.225");
}

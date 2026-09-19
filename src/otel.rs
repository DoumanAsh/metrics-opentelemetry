use std::sync::Arc;
use std::borrow::Cow;
use std::collections::hash_map::HashMap;
use portable_atomic::{AtomicU64, AtomicF64, Ordering};

use crate::identity::IdentityHasherBuilder;
use crate::metrics::{Key, KeyName, CounterFn, HistogramFn, GaugeFn, Unit};
use opentelemetry::KeyValue;

type OtelCache<T> = parking_lot::RwLock<HashMap<KeyName, T>>;

#[inline(always)]
fn metrics_label_to_otel(label: &metrics::Label) -> KeyValue {
    let (key, value) = label.clone().into_parts();
    let key: Cow<'static, str> = key.into();
    let value: Cow<'static, str> = value.into();
    KeyValue::new(key, value)
}

fn metrics_labels_to_otel(key: &Key) -> Vec<KeyValue> {
    let mut labels = key.labels().map(metrics_label_to_otel).collect::<Vec<_>>();
    labels.sort_unstable_by(|a, b| a.key.cmp(&b.key));
    labels.dedup_by(|a, b| a.key == b.key);
    labels
}

#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyIdentity(u64);

impl From<&Key> for KeyIdentity {
    #[inline]
    fn from(value: &Key) -> Self {
        Self(value.get_hash())
    }
}

const fn unit_to_ucum_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Count            => "1",
        Unit::Percent          => "%",

        Unit::Seconds          => "s",
        Unit::Milliseconds     => "ms",
        Unit::Microseconds     => "us",
        Unit::Nanoseconds      => "ns",

        Unit::Tebibytes        => "TiBy",
        Unit::Gibibytes        => "GiBy",
        Unit::Mebibytes        => "MiBy",
        Unit::Kibibytes        => "KiBy",
        Unit::Bytes            => "By",

        Unit::TerabitsPerSecond => "Tbit/s",
        Unit::GigabitsPerSecond => "Gbit/s",
        Unit::MegabitsPerSecond => "Mbit/s",
        Unit::KilobitsPerSecond => "kbit/s",
        Unit::BitsPerSecond     => "bit/s",

        Unit::CountPerSecond    => "1/s",
    }
}

pub trait CounterSink {
    fn add(&self, value: u64);
}

pub struct OtelCounter {
    inner: opentelemetry::metrics::Counter<u64>,
    labels: Vec<KeyValue>,
}

impl CounterSink for OtelCounter {
    #[inline(always)]
    fn add(&self, value: u64) {
        self.inner.add(value, &self.labels);
    }
}

#[cfg(feature = "experimental_metrics_bound_instruments")]
impl CounterSink for opentelemetry::metrics::BoundCounter<u64> {
    #[inline(always)]
    fn add(&self, value: u64) {
        opentelemetry::metrics::BoundCounter::<u64>::add(self, value)
    }
}

pub struct CounterpWrapper<T: CounterSink> {
    value: AtomicU64,
    otel: T,
}

impl<T: CounterSink> CounterpWrapper<T> {
    fn new(otel: T) -> Self {
        //Initialize metric on first creation
        otel.add(0);
        Self {
            value: AtomicU64::new(0),
            otel,
        }
    }
}

impl<T: CounterSink> CounterFn for CounterpWrapper<T> {
    #[inline(always)]
    fn absolute(&self, value: u64) {
        let prev = self.value.fetch_max(value, Ordering::AcqRel);
        //OTEL expects increasing counter, so it cannot have negative increment
        self.otel.add(value.saturating_sub(prev));
    }

    #[inline(always)]
    fn increment(&self, value: u64) {
        self.value.fetch_add(value, Ordering::Release);
        self.otel.add(value);
    }
}

pub trait HistogramSink {
    fn record(&self, value: f64);
}

pub struct OtelHistogram {
    inner: opentelemetry::metrics::Histogram<f64>,
    labels: Vec<KeyValue>,
}

impl HistogramSink for OtelHistogram {
    #[inline(always)]
    fn record(&self, value: f64) {
        self.inner.record(value, &self.labels);
    }
}

#[cfg(feature = "experimental_metrics_bound_instruments")]
impl HistogramSink for opentelemetry::metrics::BoundHistogram<f64> {
    #[inline(always)]
    fn record(&self, value: f64) {
        opentelemetry::metrics::BoundHistogram::<f64>::record(self, value)
    }
}

pub struct HistogramWrapper<T: HistogramSink> {
    otel: T,
}

impl<T: HistogramSink> HistogramWrapper<T> {
    fn new(otel: T) -> Self {
        Self {
            otel,
        }
    }
}

impl<T: HistogramSink> HistogramFn for HistogramWrapper<T> {
    fn record(&self, value: f64) {
        self.otel.record(value)
    }
}

pub trait GaugeSink {
    fn record(&self, value: f64);
}

pub struct OtelGauge {
    inner: opentelemetry::metrics::Gauge<f64>,
    labels: Vec<KeyValue>,
}

impl GaugeSink for OtelGauge {
    #[inline(always)]
    fn record(&self, value: f64) {
        self.inner.record(value, &self.labels);
    }
}

#[cfg(feature = "experimental_metrics_bound_instruments")]
impl GaugeSink for opentelemetry::metrics::BoundGauge<f64> {
    #[inline(always)]
    fn record(&self, value: f64) {
        opentelemetry::metrics::BoundGauge::<f64>::record(self, value)
    }
}

pub struct GaugeWrapper<T: GaugeSink> {
    value: AtomicF64,
    otel: T,
}

impl<T: GaugeSink> GaugeWrapper<T> {
    fn new(otel: T) -> Self {
        //Initialize metric on first creation
        otel.record(0.0);
        Self {
            value: AtomicF64::new(0.0),
            otel,
        }
    }
}

impl<T: GaugeSink> GaugeFn for GaugeWrapper<T> {
    #[inline(always)]
    fn set(&self, value: f64) {
        self.value.store(value, Ordering::Release);
        self.otel.record(value);
    }

    #[inline(always)]
    fn increment(&self, value: f64) {
        let prev = self.value.fetch_add(value, Ordering::AcqRel);
        self.otel.record(prev + value);
    }

    #[inline(always)]
    fn decrement(&self, value: f64) {
        let prev = self.value.fetch_sub(value, Ordering::AcqRel);
        self.otel.record(prev - value)
    }
}

pub(crate) struct Metadata {
    description: metrics::SharedString,
    unit: Option<&'static str>
}

impl Metadata {
    #[inline(always)]
    pub const fn from_metrics(description: metrics::SharedString, unit: Option<metrics::Unit>) -> Self {
        Self {
            unit: match unit {
                Some(unit) => Some(unit_to_ucum_label(unit)),
                None => None,
            },
            description,
        }
    }
}

#[derive(Default)]
pub(crate) struct HistogramMetadata {
    meta: Option<Metadata>,
    bounds: Vec<f64>,
}

impl HistogramMetadata {
    #[inline]
    pub fn set_bounds(&mut self, bounds: Vec<f64>) {
        self.bounds = bounds;
    }

    #[inline]
    pub fn set_metadata(&mut self, meta: Metadata) {
        self.meta = Some(meta);
    }
}

impl From<Metadata> for HistogramMetadata {
    #[inline]
    fn from(value: Metadata) -> Self {
        Self {
            meta: Some(value),
            bounds: Vec::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct MetadataStore {
    pub(crate) counter: parking_lot::RwLock<HashMap<KeyName, Metadata>>,
    pub(crate) gauge: parking_lot::RwLock<HashMap<KeyName, Metadata>>,
    pub(crate) histogram: parking_lot::RwLock<HashMap<KeyName, HistogramMetadata>>,
}

#[derive(Default)]
pub(crate) struct InstrumentsStore {
    //Cache for OTEL instruments which can be re-used regardless of label combination
    otel_counter: OtelCache<opentelemetry::metrics::Counter<u64>>,
    otel_gauge: OtelCache<opentelemetry::metrics::Gauge<f64>>,
    otel_histogram: OtelCache<opentelemetry::metrics::Histogram<f64>>,
    pub(crate) counter: parking_lot::RwLock<HashMap<KeyIdentity, metrics::Counter, IdentityHasherBuilder>>,
    pub(crate) gauge: parking_lot::RwLock<HashMap<KeyIdentity, metrics::Gauge, IdentityHasherBuilder>>,
    pub(crate) histogram: parking_lot::RwLock<HashMap<KeyIdentity, metrics::Histogram, IdentityHasherBuilder>>,
}

///Opentelemetry metrics storage
pub struct OpenTelemetryMetrics {
    metrics: opentelemetry::metrics::Meter,
    pub(crate) metadata: MetadataStore,
    instruments: InstrumentsStore,
}

impl OpenTelemetryMetrics {
    ///Creates new instance with provided opentelemetry's metrics backend
    ///
    ///## Stability
    ///
    ///This function provides no stability guarantee and will require latest `opentelemetry` version at the time of publishing crate
    pub fn new(metrics: opentelemetry::metrics::Meter) -> Self {
        Self {
            metrics,
            metadata: Default::default(),
            instruments: Default::default(),
        }
    }

    fn create_counter(&self, key: &Key) -> metrics::Counter {
        let key_name = key.name_shared();

        let inner = {
            let otel_counters = self.instruments.otel_counter.upgradable_read();
            match otel_counters.get(&key_name) {
                Some(counter) => counter.clone(),
                None => {
                    let mut counter = self.metrics.u64_counter(key_name.clone().into_inner());
                    if let Some(meta) = self.metadata.counter.read().get(&key_name) {
                        counter = counter.with_description(meta.description.clone());
                        if let Some(unit) = meta.unit {
                            counter = counter.with_unit(unit);
                        }
                    }
                    let counter = counter.build();
                    parking_lot::lock_api::RwLockUpgradableReadGuard::upgrade(otel_counters).insert(key_name.clone(), counter.clone());
                    counter
                }
            }
        };

        let labels = metrics_labels_to_otel(key);
        #[cfg(feature = "experimental_metrics_bound_instruments")]
        if labels.is_empty() {
            metrics::Counter::from_arc(Arc::new(CounterpWrapper::new(OtelCounter {
                inner,
                labels,
            })))
        } else {
            metrics::Counter::from_arc(Arc::new(CounterpWrapper::new(inner.bind(&labels))))
        }

        #[cfg(not(feature = "experimental_metrics_bound_instruments"))]
        metrics::Counter::from_arc(Arc::new(CounterpWrapper::new(OtelCounter {
            inner,
            labels,
        })))
    }

    pub(crate) fn get_or_create_counter(&self, key: &Key) -> metrics::Counter {
        let guard = self.instruments.counter.upgradable_read();
        if let Some(counter) = guard.get(&key.into()) {
            counter.clone()
        } else {
            let mut guard = parking_lot::lock_api::RwLockUpgradableReadGuard::upgrade(guard);
            let counter = self.create_counter(key);
            guard.insert(key.into(), counter.clone());
            counter
        }
    }

    fn create_gauge(&self, key: &Key) -> metrics::Gauge {
        let key_name = key.name_shared();

        let inner = {
            let otel_gauge = self.instruments.otel_gauge.upgradable_read();
            match otel_gauge.get(&key_name) {
                Some(gauge) => gauge.clone(),
                None => {
                    let mut gauge = self.metrics.f64_gauge(key_name.clone().into_inner());
                    if let Some(meta) = self.metadata.gauge.read().get(&key_name) {
                        gauge = gauge.with_description(meta.description.clone());
                        if let Some(unit) = meta.unit {
                            gauge = gauge.with_unit(unit);
                        }
                    }
                    let gauge = gauge.build();
                    parking_lot::lock_api::RwLockUpgradableReadGuard::upgrade(otel_gauge).insert(key_name.clone(), gauge.clone());
                    gauge
                }
            }
        };

        let labels = metrics_labels_to_otel(key);
        #[cfg(feature = "experimental_metrics_bound_instruments")]
        if labels.is_empty() {
            metrics::Gauge::from_arc(Arc::new(GaugeWrapper::new(OtelGauge {
                inner,
                labels,
            })))
        } else {
            metrics::Gauge::from_arc(Arc::new(GaugeWrapper::new(inner.bind(&labels))))
        }

        #[cfg(not(feature = "experimental_metrics_bound_instruments"))]
        metrics::Gauge::from_arc(Arc::new(GaugeWrapper::new(OtelGauge {
            inner,
            labels,
        })))
    }

    pub(crate) fn get_or_create_gauge(&self, key: &Key) -> metrics::Gauge {
        let guard = self.instruments.gauge.upgradable_read();
        if let Some(gauge) = guard.get(&key.into()) {
            gauge.clone()
        } else {
            let mut guard = parking_lot::lock_api::RwLockUpgradableReadGuard::upgrade(guard);
            let gauge = self.create_gauge(key);
            guard.insert(key.into(), gauge.clone());
            gauge
        }
    }

    fn create_histogram(&self, key: &Key) -> metrics::Histogram {
        let key_name = key.name_shared();

        let histogram = {
            let otel_histograms = self.instruments.otel_histogram.upgradable_read();
            match otel_histograms.get(&key_name) {
                Some(histogram) => histogram.clone(),
                None => {
                    let mut histogram = self.metrics.f64_histogram(key_name.clone().into_inner());

                    if let Some(metadata) = self.metadata.histogram.read().get(&key_name) {
                        if let Some(meta) = &metadata.meta {
                            histogram = histogram.with_description(meta.description.clone());
                            if let Some(unit) = meta.unit {
                                histogram = histogram.with_unit(unit);
                            }
                        }
                        if !metadata.bounds.is_empty() {
                            histogram = histogram.with_boundaries(metadata.bounds.clone());
                        }
                    }
                    let histogram = histogram.build();
                    parking_lot::lock_api::RwLockUpgradableReadGuard::upgrade(otel_histograms).insert(key_name.clone(), histogram.clone());
                    histogram
                }
            }
        };

        let labels = metrics_labels_to_otel(key);
        #[cfg(feature = "experimental_metrics_bound_instruments")]
        if labels.is_empty() {
            metrics::Histogram::from_arc(Arc::new(HistogramWrapper::new(OtelHistogram {
                inner: histogram,
                labels,
            })))
        } else {
            metrics::Histogram::from_arc(Arc::new(HistogramWrapper::new(histogram.bind(&labels))))
        }

        #[cfg(not(feature = "experimental_metrics_bound_instruments"))]
        metrics::Histogram::from_arc(Arc::new(HistogramWrapper::new(OtelHistogram {
            inner: histogram,
            labels,
        })))
    }

    pub(crate) fn get_or_create_histogram(&self, key: &Key) -> metrics::Histogram {
        let guard = self.instruments.histogram.upgradable_read();
        if let Some(histogram) = guard.get(&key.into()) {
            histogram.clone()
        } else {
            let mut guard = parking_lot::lock_api::RwLockUpgradableReadGuard::upgrade(guard);
            let histogram = self.create_histogram(key);
            guard.insert(key.into(), histogram.clone());
            histogram
        }
    }
}

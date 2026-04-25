use std::sync::Arc;
use std::borrow::Cow;
use std::collections::hash_map::HashMap;
use core::cell::Cell;
use portable_atomic::{AtomicU64, AtomicF64, Ordering};

use crate::identity::IdentityHasherBuilder;
use crate::metrics::{Key, KeyName, CounterFn, HistogramFn, GaugeFn, Unit};
use opentelemetry::KeyValue;

#[inline(always)]
fn metrics_label_to_otel(label: &metrics::Label) -> KeyValue {
    let (key, value) = label.clone().into_parts();
    let key: Cow<'static, str> = key.into();
    let value: Cow<'static, str> = value.into();
    KeyValue::new(key, value)
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

#[repr(transparent)]
//This can be done with ManuallyDrop, but 1 extra byte is not really big on size
struct UninitOtelItem<T>(Cell<Option<T>>);

impl<T> UninitOtelItem<T> {
    #[inline]
    const fn new_uninit() -> Self {
        UninitOtelItem(Cell::new(None))
    }

    #[inline]
    fn init(&self, value: T) {
        self.0.set(Some(value));
    }
}

unsafe impl<T: Sync> Sync for UninitOtelItem<T> {}

pub struct Counter {
    value: AtomicU64,
    _otel: UninitOtelItem<opentelemetry::metrics::ObservableCounter<u64>>,
}

impl Counter {
    #[inline]
    const fn new() -> Self {
        Self {
            _otel: UninitOtelItem::new_uninit(),
            value: AtomicU64::new(0),
        }
    }
}

impl CounterFn for Counter {
    #[inline(always)]
    fn absolute(&self, value: u64) {
        self.value.store(value, Ordering::Release);
    }

    #[inline(always)]
    fn increment(&self, value: u64) {
        self.value.fetch_add(value, Ordering::AcqRel);
    }
}

pub struct Histogram {
    otel: opentelemetry::metrics::Histogram<f64>,
    labels: Vec<KeyValue>
}

impl HistogramFn for Histogram {
    #[inline(always)]
    fn record(&self, value: f64) {
        self.otel.record(value, &self.labels);
    }
}

pub struct Gauge {
    value: AtomicF64,
    _otel: UninitOtelItem<opentelemetry::metrics::ObservableGauge<f64>>,
}

impl Gauge {
    #[inline]
    const fn new() -> Self {
        Self {
            _otel: UninitOtelItem::new_uninit(),
            value: AtomicF64::new(0.0),
        }
    }
}

impl GaugeFn for Gauge {
    #[inline(always)]
    fn set(&self, value: f64) {
        self.value.store(value, Ordering::Release);
    }
    #[inline(always)]
    fn increment(&self, value: f64) {
        self.value.fetch_add(value, Ordering::AcqRel);
    }
    #[inline(always)]
    fn decrement(&self, value: f64) {
        self.value.fetch_sub(value, Ordering::AcqRel);
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
pub(crate) struct MetadataStore {
    pub(crate) counter: parking_lot::RwLock<HashMap<KeyName, Metadata>>,
    pub(crate) gauge: parking_lot::RwLock<HashMap<KeyName, Metadata>>,
    pub(crate) histogram: parking_lot::RwLock<HashMap<KeyName, Metadata>>,
}

#[derive(Default)]
pub(crate) struct InstrumentsStore {
    pub(crate) counter: parking_lot::RwLock<HashMap<KeyIdentity, Arc<Counter>, IdentityHasherBuilder>>,
    pub(crate) gauge: parking_lot::RwLock<HashMap<KeyIdentity, Arc<Gauge>, IdentityHasherBuilder>>,
    pub(crate) histogram: parking_lot::RwLock<HashMap<KeyIdentity, Arc<Histogram>, IdentityHasherBuilder>>,
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

    fn create_counter(&self, key: &Key) -> Arc<Counter> {
        let key_name = key.name_shared();
        let labels = key.labels().map(metrics_label_to_otel).collect::<Vec<_>>();
        let mut counter = self.metrics.u64_observable_counter(key_name.clone().into_inner());

        if let Some(metadata) = self.metadata.counter.read().get(&key_name) {
           counter = counter.with_description(metadata.description.clone());
           if let Some(unit) = metadata.unit {
               counter = counter.with_unit(unit);
           }
        }

        let this = Arc::new(Counter::new());
        let observe_this = this.clone();
        let _counter = counter.with_callback(move |observer| {
            observer.observe(observe_this.value.load(Ordering::Acquire), &labels);
        }).build();
        this._otel.init(_counter);
        this
    }

    pub(crate) fn get_or_create_counter(&self, key: &Key) -> Arc<Counter> {
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

    fn create_gauge(&self, key: &Key) -> Arc<Gauge> {
        let key_name = key.name_shared();
        let labels = key.labels().map(metrics_label_to_otel).collect::<Vec<_>>();

        let mut gauge = self.metrics.f64_observable_gauge(key_name.clone().into_inner());

        if let Some(metadata) = self.metadata.gauge.read().get(&key_name) {
           gauge = gauge.with_description(metadata.description.clone());
           if let Some(unit) = metadata.unit {
               gauge = gauge.with_unit(unit);
           }
        }

        let this = Arc::new(Gauge::new());
        let observe_this = this.clone();
        let _gauge = gauge.with_callback(move |observer| {
            observer.observe(observe_this.value.load(Ordering::Acquire), &labels);
        }).build();
        this._otel.init(_gauge);
        this
    }

    pub(crate) fn get_or_create_gauge(&self, key: &Key) -> Arc<Gauge> {
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

    fn create_histogram(&self, key: &Key) -> Histogram {
        let key_name = key.name_shared();
        let labels = key.labels().map(metrics_label_to_otel).collect::<Vec<_>>();
        let mut histogram = self.metrics.f64_histogram(key_name.clone().into_inner());

        if let Some(metadata) = self.metadata.histogram.read().get(&key_name) {
           histogram = histogram.with_description(metadata.description.clone());
           if let Some(unit) = metadata.unit {
               histogram = histogram.with_unit(unit);
           }
        }

        Histogram {
            otel: histogram.build(),
            labels
        }
    }

    pub(crate) fn get_or_create_histogram(&self, key: &Key) -> Arc<Histogram> {
        let guard = self.instruments.histogram.upgradable_read();
        if let Some(histogram) = guard.get(&key.into()) {
            histogram.clone()
        } else {
            let mut guard = parking_lot::lock_api::RwLockUpgradableReadGuard::upgrade(guard);
            let histogram = Arc::new(self.create_histogram(key));
            guard.insert(key.into(), histogram.clone());
            histogram
        }
    }
}

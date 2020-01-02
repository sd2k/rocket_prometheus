/*!
Prometheus instrumentation for Rocket applications.

# Usage

Add this crate to your `Cargo.toml`:

```toml
[dependencies]
rocket_prometheus = "0.3.1"
```

Then attach and mount a `PrometheusMetrics` instance to your Rocket app:

```rust
use rocket_prometheus::PrometheusMetrics;

let prometheus = PrometheusMetrics::new();
# if false {
rocket::ignite()
    .attach(prometheus.clone())
    .mount("/metrics", prometheus)
    .launch();
# }
```

This will expose metrics like this at the /metrics endpoint of your application:

```shell
$ curl localhost:8000/metrics
# HELP rocket_http_requests_duration_seconds HTTP request duration in seconds for all requests
# TYPE rocket_http_requests_duration_seconds histogram
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.005"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.01"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.025"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.05"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.1"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.25"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.5"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="1"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="2.5"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="5"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="10"} 2
rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="+Inf"} 2
rocket_http_requests_duration_seconds_sum{endpoint="/metrics",method="GET",status="200"} 0.0011045669999999999
rocket_http_requests_duration_seconds_count{endpoint="/metrics",method="GET",status="200"} 2
# HELP rocket_http_requests_total Total number of HTTP requests
# TYPE rocket_http_requests_total counter
rocket_http_requests_total{endpoint="/metrics",method="GET",status="200"} 2
```

# Metrics

By default this crate tracks two metrics:

- `rocket_http_requests_total` (labels: endpoint, method, status): the
  total number of HTTP requests handled by Rocket.
- `rocket_http_requests_duration_seconds` (labels: endpoint, method, status):
  the request duration for all HTTP requests handled by Rocket.

The 'rocket' prefix of these metrics can be changed by setting the
`ROCKET_PROMETHEUS_NAMESPACE` environment variable.

## Custom Metrics

Further metrics can be tracked by registering them with the registry of the
PrometheusMetrics instance:

```rust
#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

use once_cell::sync::Lazy;
use rocket::http::RawStr;
use rocket_prometheus::{
    prometheus::{opts, IntCounterVec},
    PrometheusMetrics,
};

static NAME_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| {
    IntCounterVec::new(opts!("name_counter", "Count of names"), &["name"])
        .expect("Could not create NAME_COUNTER")
});

#[get("/hello/<name>")]
pub fn hello(name: &RawStr) -> String {
    NAME_COUNTER.with_label_values(&[name]).inc();
    format!("Hello, {}!", name)
}

fn main() {
    let prometheus = PrometheusMetrics::new();
    prometheus
        .registry()
        .register(Box::new(NAME_COUNTER.clone()))
        .unwrap();
    # if false {
    rocket::ignite()
        .attach(prometheus.clone())
        .mount("/", routes![hello])
        .mount("/metrics", prometheus)
        .launch();
    # }
}
```

*/
#![deny(missing_docs)]

use std::{env, time::Instant};

use prometheus::{opts, Encoder, HistogramVec, IntCounterVec, Registry, TextEncoder};
use rocket::{
    fairing::{Fairing, Info, Kind},
    handler::Outcome,
    http::Method,
    Data, Handler, Request, Response, Route,
};

/// Re-export Prometheus so users can use it without having to explicitly
/// add a specific version to their dependencies, which can result in
/// mysterious compiler error messages.
pub use prometheus;

/// Environment variable used to configure the namespace of metrics exposed
/// by PrometheusMetrics.
const NAMESPACE_ENV_VAR: &str = "ROCKET_PROMETHEUS_NAMESPACE";

// This can be removed when `duration_float` feature is stabilised - see
// the `on_response` method of the Fairing impl for PrometheusMetrics.
const NANOS_PER_SEC: f64 = 1_000_000_000_f64;

#[derive(Clone)]
#[must_use = "must be attached and mounted to a Rocket instance"]
/// Fairing and Handler implementing request instrumentation.
///
/// By default this tracks two metrics:
///
/// - `rocket_http_requests_total` (labels: endpoint, method, status): the
///   total number of HTTP requests handled by Rocket.
/// - `rocket_http_requests_duration_seconds` (labels: endpoint, method, status):
///   the request duration for all HTTP requests handled by Rocket.
///
/// The 'rocket' prefix of these metrics can be changed by setting the
/// `ROCKET_PROMETHEUS_NAMESPACE` environment variable.
///
/// # Usage
///
/// Simply attach and mount a `PrometheusMetrics` instance to your Rocket
/// app as for a normal fairing / handler:
///
/// ```rust
/// use rocket_prometheus::PrometheusMetrics;
///
/// let prometheus = PrometheusMetrics::new();
/// # if false {
/// rocket::ignite()
///     .attach(prometheus.clone())
///     .mount("/metrics", prometheus)
///     .launch();
/// # }
/// ```
///
/// Metrics will then be available on the "/metrics" endpoint:
///
/// ```shell
/// $ curl localhost:8000/metrics
/// # HELP rocket_http_requests_duration_seconds HTTP request duration in seconds for all requests
/// # TYPE rocket_http_requests_duration_seconds histogram
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.005"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.01"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.025"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.05"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.1"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.25"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="0.5"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="1"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="2.5"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="5"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="10"} 2
/// rocket_http_requests_duration_seconds_bucket{endpoint="/metrics",method="GET",status="200",le="+Inf"} 2
/// rocket_http_requests_duration_seconds_sum{endpoint="/metrics",method="GET",status="200"} 0.0011045669999999999
/// rocket_http_requests_duration_seconds_count{endpoint="/metrics",method="GET",status="200"} 2
/// # HELP rocket_http_requests_total Total number of HTTP requests
/// # TYPE rocket_http_requests_total counter
/// rocket_http_requests_total{endpoint="/metrics",method="GET",status="200"} 2
/// ```
pub struct PrometheusMetrics {
    // Standard metrics tracked by the fairing.
    http_requests_total: IntCounterVec,
    http_requests_duration_seconds: HistogramVec,

    // The registry used by the fairing.
    registry: Registry,
}

impl PrometheusMetrics {
    /// Get the registry used by this fairing.
    ///
    /// You can use this to register further metrics,
    /// causing them to be exposed along with the default metrics
    /// on the PrometheusMetrics handler.
    ///
    /// ```rust
    /// use once_cell::sync::Lazy;
    /// use prometheus::{opts, IntCounter};
    /// use rocket_prometheus::PrometheusMetrics;
    ///
    /// static MY_COUNTER: Lazy<IntCounter> = Lazy::new(|| {
    ///     IntCounter::new("my_counter", "A counter I use a lot")
    ///         .expect("Could not create counter")
    /// });
    ///
    /// let prometheus = PrometheusMetrics::new();
    /// prometheus.registry().register(Box::new(MY_COUNTER.clone()));
    /// ```
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

impl PrometheusMetrics {
    /// Create a new `PrometheusMetrics`.
    pub fn new() -> Self {
        Self::with_registry(Registry::new())
    }

    /// Create a new `PrometheusMetrics` with a custom `Registry`.
    pub fn with_registry(registry: Registry) -> Self {
        let namespace = env::var(NAMESPACE_ENV_VAR).unwrap_or_else(|_| "rocket".into());

        let http_requests_total_opts =
            opts!("http_requests_total", "Total number of HTTP requests")
                .namespace(namespace.clone());
        let http_requests_total =
            IntCounterVec::new(http_requests_total_opts, &["endpoint", "method", "status"])
                .unwrap();
        registry
            .register(Box::new(http_requests_total.clone()))
            .unwrap();

        let http_requests_duration_seconds_opts = opts!(
            "http_requests_duration_seconds",
            "HTTP request duration in seconds for all requests"
        )
        .namespace(namespace);
        let http_requests_duration_seconds = HistogramVec::new(
            http_requests_duration_seconds_opts.into(),
            &["endpoint", "method", "status"],
        )
        .unwrap();
        registry
            .register(Box::new(http_requests_duration_seconds.clone()))
            .unwrap();

        PrometheusMetrics {
            http_requests_total,
            http_requests_duration_seconds,
            registry,
        }
    }
}

impl Default for PrometheusMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Value stored in request-local state to measure response time.
#[derive(Copy, Clone)]
struct TimerStart(Option<Instant>);

impl Fairing for PrometheusMetrics {
    fn info(&self) -> Info {
        Info {
            name: "Prometheus metric collection",
            kind: Kind::Request | Kind::Response,
        }
    }

    fn on_request(&self, request: &mut Request, _: &Data) {
        request.local_cache(|| TimerStart(Some(Instant::now())));
    }

    fn on_response(&self, request: &Request, response: &mut Response) {
        // Don't touch metrics if the request didn't match a route.
        if request.route().is_none() {
            return;
        }

        let endpoint = request.route().unwrap().uri.to_string();
        let method = request.method().as_str();
        let status = response.status().code.to_string();
        self.http_requests_total
            .with_label_values(&[&endpoint, method, &status])
            .inc();

        let start_time = request.local_cache(|| TimerStart(None));
        if let Some(duration) = start_time.0.map(|st| st.elapsed()) {
            // Can replace this with `duration.as_secs_f64()` when `duration_float`
            // feature is stabilised (https://github.com/rust-lang/rust/issues/54361).
            // let duration_secs = duration.as_secs_f64();
            let duration_secs =
                (duration.as_secs() as f64) + f64::from(duration.subsec_nanos()) / NANOS_PER_SEC;
            self.http_requests_duration_seconds
                .with_label_values(&[&endpoint, method, &status])
                .observe(duration_secs);
        }
    }
}

impl Handler for PrometheusMetrics {
    fn handle<'r>(&self, req: &'r Request, _: Data) -> Outcome<'r> {
        // Gather the metrics.
        let mut buffer = vec![];
        let encoder = TextEncoder::new();
        encoder
            .encode(&self.registry.gather(), &mut buffer)
            .unwrap();
        let out = String::from_utf8(buffer).unwrap();
        Outcome::from(req, out)
    }
}

impl Into<Vec<Route>> for PrometheusMetrics {
    fn into(self) -> Vec<Route> {
        vec![Route::new(Method::Get, "/", self)]
    }
}

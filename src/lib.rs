/*!
Prometheus instrumentation for Rocket applications.

# Usage

Add this crate to your `Cargo.toml`:

```toml
[dependencies]
rocket_prometheus = "0.8.1"
```

Then attach and mount a [`PrometheusMetrics`] instance to your Rocket app:

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
[`PrometheusMetrics`] instance:

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
#![forbid(unsafe_code)]

use std::{env, time::Instant};

use prometheus::{opts, Encoder, HistogramVec, IntCounterVec, Registry, TextEncoder};
use rocket::{
    fairing::{Fairing, Info, Kind},
    handler::Outcome,
    http::{ContentType, Method},
    response::Content,
    Data, Handler, Request, Response, Route,
};

/// Re-export Prometheus so users can use it without having to explicitly
/// add a specific version to their dependencies, which can result in
/// mysterious compiler error messages.
pub use prometheus;

/// Environment variable used to configure the namespace of metrics exposed
/// by `PrometheusMetrics`.
const NAMESPACE_ENV_VAR: &str = "ROCKET_PROMETHEUS_NAMESPACE";

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

    // The registry used by the fairing for Rocket metrics.
    //
    // This registry is created by `PrometheusMetrics::with_registry` and is
    // private to each `PrometheusMetrics` instance, allowing multiple
    // `PrometheusMetrics` instances to share the same `extra_registry`.
    //
    // Previously the fairing tried to register the internal metrics on the `extra_registry`,
    // which caused conflicts if the same registry was passed twice. This is now avoided
    // by using an internal registry for those metrics.
    rocket_registry: Registry,

    // The registry used by the fairing for custom metrics.
    //
    // See `rocket_registry` for details on why these metrics are stored on a separate registry.
    custom_registry: Registry,
}

impl PrometheusMetrics {
    /// Create a new [`PrometheusMetrics`].
    pub fn new() -> Self {
        Self::with_registry(Registry::new())
    }

    /// Create a new [`PrometheusMetrics`] with a custom [`Registry`].
    // Allow `clippy::missing_panics_doc` because we know:
    // - the two metrics can't fail to be created (their config is valid)
    // - registering the metrics can't fail (the registry is new, so there is no chance of metric duplication)
    #[allow(clippy::missing_panics_doc)]
    pub fn with_registry(registry: Registry) -> Self {
        let rocket_registry = Registry::new();
        let namespace = env::var(NAMESPACE_ENV_VAR).unwrap_or_else(|_| "rocket".into());

        let http_requests_total_opts =
            opts!("http_requests_total", "Total number of HTTP requests")
                .namespace(namespace.clone());
        let http_requests_total = IntCounterVec::new(
            http_requests_total_opts,
            &["endpoint", "method", "status", "referrer"],
        )
        .unwrap();
        let http_requests_duration_seconds_opts = opts!(
            "http_requests_duration_seconds",
            "HTTP request duration in seconds for all requests"
        )
        .namespace(namespace);
        let http_requests_duration_seconds = HistogramVec::new(
            http_requests_duration_seconds_opts.into(),
            &["endpoint", "method", "status", "referrer"],
        )
        .unwrap();

        rocket_registry
            .register(Box::new(http_requests_total.clone()))
            .unwrap();
        rocket_registry
            .register(Box::new(http_requests_duration_seconds.clone()))
            .unwrap();

        Self {
            http_requests_total,
            http_requests_duration_seconds,
            rocket_registry,
            custom_registry: registry,
        }
    }

    /// Create a new [`PrometheusMetrics`] using the default Prometheus [`Registry`].
    ///
    /// This will cause the fairing to include metrics created by the various
    /// `prometheus` macros, e.g.  `register_int_counter`.
    pub fn with_default_registry() -> Self {
        Self::with_registry(prometheus::default_registry().clone())
    }

    /// Get the registry used by this fairing to track additional metrics.
    ///
    /// You can use this to register further metrics,
    /// causing them to be exposed along with the default metrics
    /// on the [`PrometheusMetrics`] handler.
    ///
    /// Note that the `http_requests_total` and `http_requests_duration_seconds` metrics
    /// are _not_ included in this registry.
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
    #[must_use]
    pub const fn registry(&self) -> &Registry {
        &self.custom_registry
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
        let referrer = request.headers().get("Referer").next().unwrap_or("?");
        self.http_requests_total
            .with_label_values(&[&endpoint, method, &status, referrer])
            .inc();

        let start_time = request.local_cache(|| TimerStart(None));
        if let Some(duration) = start_time.0.map(|st| st.elapsed()) {
            let duration_secs = duration.as_secs_f64();
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
            .encode(&self.custom_registry.gather(), &mut buffer)
            .unwrap();
        encoder
            .encode(&self.rocket_registry.gather(), &mut buffer)
            .unwrap();
        let body = String::from_utf8(buffer).unwrap();
        Outcome::from(
            req,
            Content(
                ContentType::with_params(
                    "text",
                    "plain",
                    &[("version", "0.0.4"), ("charset", "utf-8")],
                ),
                body,
            ),
        )
    }
}

impl From<PrometheusMetrics> for Vec<Route> {
    fn from(other: PrometheusMetrics) -> Self {
        vec![Route::new(Method::Get, "/", other)]
    }
}

#[cfg(test)]
mod test {
    use super::PrometheusMetrics;

    #[test]
    fn test_multiple_instantiations() {
        let _pm1 = PrometheusMetrics::with_default_registry();
        let _pm2 = PrometheusMetrics::with_default_registry();
    }
}

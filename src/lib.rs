/*!
Prometheus instrumentation for Rocket applications.

# Usage

Add this crate to your `Cargo.toml` alongside Rocket 0.5.0-rc.2:

```toml
[dependencies]
rocket = "0.5.0-rc.2"
rocket_prometheus = "0.10.0-rc.2"
```

Then attach and mount a [`PrometheusMetrics`] instance to your Rocket app:

```rust
use rocket_prometheus::PrometheusMetrics;

#[rocket::launch]
fn launch() -> _ {
    let prometheus = PrometheusMetrics::new();
    rocket::build()
        .attach(prometheus.clone())
        .mount("/metrics", prometheus)
}
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
use once_cell::sync::Lazy;
use rocket::{get, launch, routes};
use rocket_prometheus::{
    prometheus::{opts, IntCounterVec},
    PrometheusMetrics,
};

static NAME_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| {
    IntCounterVec::new(opts!("name_counter", "Count of names"), &["name"])
        .expect("Could not create NAME_COUNTER")
});

#[get("/hello/<name>")]
pub fn hello(name: &str) -> String {
    NAME_COUNTER.with_label_values(&[name]).inc();
    format!("Hello, {}!", name)
}

#[launch]
fn launch() -> _ {
    let prometheus = PrometheusMetrics::new();
    prometheus
        .registry()
        .register(Box::new(NAME_COUNTER.clone()))
        .unwrap();
    rocket::build()
        .attach(prometheus.clone())
        .mount("/", routes![hello])
        .mount("/metrics", prometheus)
}
```

*/
#![deny(missing_docs)]
#![deny(unsafe_code)]

use std::{env, time::Instant};

use prometheus::{opts, Encoder, HistogramVec, IntCounterVec, Registry, TextEncoder};
use rocket::{
    fairing::{Fairing, Info, Kind},
    http::{ContentType, Method},
    route::{Handler, Outcome},
    Data, Orbit, Request, Response, Rocket, Route,
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
/// The `rocket` prefix of these metrics can be changed by setting the
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
/// #[rocket::launch]
/// fn launch() -> _ {
///     let prometheus = PrometheusMetrics::new();
///     rocket::build()
///         .attach(prometheus.clone())
///         .mount("/metrics", prometheus)
/// }
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
        let http_requests_total =
            IntCounterVec::new(http_requests_total_opts, &["endpoint", "method", "status"])
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

/// A status code which tries not to allocate to produce a `&str` representation.
enum StatusCode {
    /// A 'standard' status code, i.e. between 100 and 999.
    ///
    /// Most status codes should be represented as this variant,
    /// which doesn't allocate and provides a non-allocating `&str`
    /// representation.
    Standard(rocket::http::Status),
    /// A non-standard status code.
    ///
    /// This is the fallback option used when a status code can't be
    /// parsed by [`http::StatusCode`]. It requires an allocation.
    NonStandard(String),
}

// A string of packed 3-ASCII-digit status code values for the supported range
// of [100, 999] (900 codes, 2700 bytes).
// Taken directly from `http::status`.
const CODE_DIGITS: &str = "\
100101102103104105106107108109110111112113114115116117118119\
120121122123124125126127128129130131132133134135136137138139\
140141142143144145146147148149150151152153154155156157158159\
160161162163164165166167168169170171172173174175176177178179\
180181182183184185186187188189190191192193194195196197198199\
200201202203204205206207208209210211212213214215216217218219\
220221222223224225226227228229230231232233234235236237238239\
240241242243244245246247248249250251252253254255256257258259\
260261262263264265266267268269270271272273274275276277278279\
280281282283284285286287288289290291292293294295296297298299\
300301302303304305306307308309310311312313314315316317318319\
320321322323324325326327328329330331332333334335336337338339\
340341342343344345346347348349350351352353354355356357358359\
360361362363364365366367368369370371372373374375376377378379\
380381382383384385386387388389390391392393394395396397398399\
400401402403404405406407408409410411412413414415416417418419\
420421422423424425426427428429430431432433434435436437438439\
440441442443444445446447448449450451452453454455456457458459\
460461462463464465466467468469470471472473474475476477478479\
480481482483484485486487488489490491492493494495496497498499\
500501502503504505506507508509510511512513514515516517518519\
520521522523524525526527528529530531532533534535536537538539\
540541542543544545546547548549550551552553554555556557558559\
560561562563564565566567568569570571572573574575576577578579\
580581582583584585586587588589590591592593594595596597598599\
600601602603604605606607608609610611612613614615616617618619\
620621622623624625626627628629630631632633634635636637638639\
640641642643644645646647648649650651652653654655656657658659\
660661662663664665666667668669670671672673674675676677678679\
680681682683684685686687688689690691692693694695696697698699\
700701702703704705706707708709710711712713714715716717718719\
720721722723724725726727728729730731732733734735736737738739\
740741742743744745746747748749750751752753754755756757758759\
760761762763764765766767768769770771772773774775776777778779\
780781782783784785786787788789790791792793794795796797798799\
800801802803804805806807808809810811812813814815816817818819\
820821822823824825826827828829830831832833834835836837838839\
840841842843844845846847848849850851852853854855856857858859\
860861862863864865866867868869870871872873874875876877878879\
880881882883884885886887888889890891892893894895896897898899\
900901902903904905906907908909910911912913914915916917918919\
920921922923924925926927928929930931932933934935936937938939\
940941942943944945946947948949950951952953954955956957958959\
960961962963964965966967968969970971972973974975976977978979\
980981982983984985986987988989990991992993994995996997998999";

/// Returns a &str representation of the `StatusCode`
///
/// The return value only includes a numerical representation of the
/// status code. The canonical reason is not included.
///
/// This is taken directly from `http::Status::as_str`.
#[inline]
fn status_as_str(s: &rocket::http::Status) -> &'static str {
    let offset = (s.code - 100) as usize;
    let offset = offset * 3;

    // Invariant: s.code has checked range [100, 999] and CODE_DIGITS is
    // ASCII-only, of length 900 * 3 = 2700 bytes

    #[cfg(debug_assertions)]
    {
        &CODE_DIGITS[offset..offset + 3]
    }

    #[cfg(not(debug_assertions))]
    #[allow(unsafe_code)]
    unsafe {
        CODE_DIGITS.get_unchecked(offset..offset + 3)
    }
}

impl StatusCode {
    fn as_str(&self) -> &str {
        match self {
            Self::Standard(s) => status_as_str(s),
            Self::NonStandard(s) => s.as_str(),
        }
    }
}

impl From<u16> for StatusCode {
    fn from(code: u16) -> Self {
        rocket::http::Status::from_code(code)
            .map_or_else(|| Self::NonStandard(code.to_string()), Self::Standard)
    }
}

#[rocket::async_trait]
impl Fairing for PrometheusMetrics {
    fn info(&self) -> Info {
        Info {
            name: "Prometheus metric collection",
            kind: Kind::Liftoff | Kind::Request | Kind::Response,
        }
    }

    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        for route in rocket.routes() {
            let uri = route.uri.as_str();
            let method = route.method.as_str();
            let status = StatusCode::from(200);

            self.http_requests_total
                .with_label_values(&[uri, method, status.as_str()]);

            self.http_requests_duration_seconds
                .with_label_values(&[uri, method, status.as_str()]);
        }
    }

    async fn on_request(&self, req: &mut Request<'_>, _: &mut Data<'_>) {
        req.local_cache(|| TimerStart(Some(Instant::now())));
    }

    async fn on_response<'r>(&self, req: &'r Request<'_>, response: &mut Response<'r>) {
        // Don't touch metrics if the request didn't match a route.
        if req.route().is_none() {
            return;
        }

        let endpoint = req.route().unwrap().uri.as_str();
        let method = req.method().as_str();
        let status = StatusCode::from(response.status().code);
        self.http_requests_total
            .with_label_values(&[endpoint, method, status.as_str()])
            .inc();

        let start_time = req.local_cache(|| TimerStart(None));
        if let Some(duration) = start_time.0.map(|st| st.elapsed()) {
            let duration_secs = duration.as_secs_f64();
            self.http_requests_duration_seconds
                .with_label_values(&[endpoint, method, status.as_str()])
                .observe(duration_secs);
        }
    }
}

#[rocket::async_trait]
impl Handler for PrometheusMetrics {
    async fn handle<'r>(&self, req: &'r Request<'_>, _: Data<'r>) -> Outcome<'r> {
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
            (
                ContentType::new("text", "plain")
                    .with_params([("version", "0.0.4"), ("charset", "utf-8")]),
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

# Rocket Prometheus

[![Build Status](https://travis-ci.org/sd2k/rocket_prometheus.svg?branch=master)](https://travis-ci.org/sd2k/rocket_prometheus)
[![docs.rs](https://docs.rs/rocket_prometheus/badge.svg)](https://docs.rs/rocket_prometheus)
[![crates.io](https://img.shields.io/crates/v/rocket_prometheus.svg)](https://crates.io/crates/rocket_prometheus)


Prometheus instrumentation for Rocket applications.

## Usage

Add this crate to your `Cargo.toml`:

```toml
[dependencies]
rocket_prometheus = "0.2"
```

Then attach and mount a `PrometheusMetrics` instance to your Rocket app:

```rust
use rocket_prometheus::PrometheusMetrics;

fn main() {
    let prometheus = PrometheusMetrics::new();
    rocket::ignite()
        .attach(prometheus.clone())
        .mount("/metrics", prometheus)
        .launch();
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

## Metrics

By default this crate tracks two metrics:

- `rocket_http_requests_total` (labels: endpoint, method, status): the
  total number of HTTP requests handled by Rocket.
- `rocket_http_requests_duration_seconds` (labels: endpoint, method, status):
  the request duration for all HTTP requests handled by Rocket.

The 'rocket' prefix of these metrics can be changed by setting the
`ROCKET_PROMETHEUS_NAMESPACE` environment variable.

### Custom Metrics

Further metrics can be tracked by registering them with the registry of the
PrometheusMetrics instance:

```rust
#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

use lazy_static::lazy_static;
use rocket::http::RawStr;
use rocket_prometheus::{
    prometheus::{opts, IntCounterVec},
    PrometheusMetrics,
};

lazy_static! {
    static ref NAME_COUNTER: IntCounterVec =
        IntCounterVec::new(opts!("name_counter", "Count of names"), &["name"]).unwrap();
}

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
    rocket::ignite()
        .attach(prometheus.clone())
        .mount("/", routes![hello])
        .mount("/metrics", prometheus)
        .launch();
}
```

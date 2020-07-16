#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

use once_cell::sync::Lazy;
use prometheus::{opts, IntCounterVec};
use rocket::{http::ContentType, local::blocking::Client};
use rocket_prometheus::PrometheusMetrics;
use serde_json::json;

static NAME_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| {
    IntCounterVec::new(opts!("name_counter", "Count of names"), &["name"])
        .expect("Could not create lazy IntCounterVec")
});

mod routes {
    use rocket::http::RawStr;
    use rocket_contrib::json::Json;
    use serde::Deserialize;

    use super::NAME_COUNTER;

    #[get("/hello/<name>")]
    pub fn hello(name: &RawStr) -> String {
        NAME_COUNTER.with_label_values(&[name]).inc();
        format!("Hello, {}!", name)
    }

    #[derive(Deserialize)]
    pub struct Person {
        age: u8,
    }

    #[post("/hello/<name>", format = "json", data = "<person>")]
    pub fn hello_post(name: String, person: Json<Person>) -> String {
        format!("Hello, {} year old named {}!", person.age, name)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_basic() {
        let prometheus = PrometheusMetrics::new();
        prometheus
            .registry()
            .register(Box::new(NAME_COUNTER.clone()))
            .unwrap();
        let rocket = rocket::ignite()
            .attach(prometheus.clone())
            .mount("/", routes![routes::hello, routes::hello_post])
            .mount("/metrics", prometheus);
        let client = Client::new(rocket).expect("valid rocket instance");
        client.get("/hello/foo").dispatch();
        client.get("/hello/foo").dispatch();
        client.get("/hello/bar").dispatch();
        client
            .post("/hello/bar")
            .header(ContentType::JSON)
            .body(serde_json::to_string(&json!({"age": 50})).unwrap())
            .dispatch();
        let metrics = client.get("/metrics").dispatch();
        let response = metrics.into_string().unwrap();
        assert_eq!(
            response
                .lines()
                .enumerate()
                .filter_map(|(i, line)|
                // Skip out the 'sum' lines since they depend on request duration.
                if i != 18 && i != 32 {
                    Some(line)
                } else {
                    None
                })
                .collect::<Vec<&str>>()
                .join("\n"),
            r#"# HELP name_counter Count of names
# TYPE name_counter counter
name_counter{name="bar"} 1
name_counter{name="foo"} 2
# HELP rocket_http_requests_duration_seconds HTTP request duration in seconds for all requests
# TYPE rocket_http_requests_duration_seconds histogram
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="0.005"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="0.01"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="0.025"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="0.05"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="0.1"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="0.25"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="0.5"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="1"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="2.5"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="5"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="10"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="GET",status="200",le="+Inf"} 3
rocket_http_requests_duration_seconds_count{endpoint="/hello/<name>",method="GET",status="200"} 3
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="0.005"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="0.01"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="0.025"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="0.05"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="0.1"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="0.25"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="0.5"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="1"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="2.5"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="5"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="10"} 1
rocket_http_requests_duration_seconds_bucket{endpoint="/hello/<name>",method="POST",status="200",le="+Inf"} 1
rocket_http_requests_duration_seconds_count{endpoint="/hello/<name>",method="POST",status="200"} 1
# HELP rocket_http_requests_total Total number of HTTP requests
# TYPE rocket_http_requests_total counter
rocket_http_requests_total{endpoint="/hello/<name>",method="GET",status="200"} 3
rocket_http_requests_total{endpoint="/hello/<name>",method="POST",status="200"} 1"#
        );
    }
}

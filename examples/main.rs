#[macro_use]
extern crate rocket;

use once_cell::sync::Lazy;
use prometheus::{opts, IntCounterVec};
use rocket_prometheus::PrometheusMetrics;

static NAME_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| {
    IntCounterVec::new(opts!("name_counter", "Count of names"), &["name"])
        .expect("Could not create lazy IntCounterVec")
});

mod routes {
    use rocket_contrib::json::Json;
    use serde::Deserialize;

    use super::NAME_COUNTER;

    #[get("/hello/<name>")]
    pub fn hello(name: &str) -> String {
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

#[tokio::main]
async fn main() {
    let prometheus = PrometheusMetrics::new();
    prometheus
        .registry()
        .register(Box::new(NAME_COUNTER.clone()))
        .unwrap();
    rocket::ignite()
        .attach(prometheus.clone())
        .mount("/", routes![routes::hello, routes::hello_post])
        .mount("/metrics", prometheus)
        .launch()
        .await
        .expect("Could not launch Rocket!");
}

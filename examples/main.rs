#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

use lazy_static::lazy_static;
use prometheus::{opts, IntCounterVec};
use rocket_prometheus::PrometheusMetrics;

lazy_static! {
    static ref NAME_COUNTER: IntCounterVec =
        IntCounterVec::new(opts!("name_counter", "Count of names"), &["name"]).unwrap();
}

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

fn main() {
    let prometheus = PrometheusMetrics::new();
    prometheus
        .registry()
        .register(Box::new(NAME_COUNTER.clone()))
        .unwrap();
    rocket::ignite()
        .attach(prometheus.clone())
        .mount("/", routes![routes::hello, routes::hello_post])
        .mount("/metrics", prometheus)
        .launch();
}

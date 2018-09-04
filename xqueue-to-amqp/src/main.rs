extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate futures_timer;
extern crate http;
extern crate hyper;
extern crate lapin_futures;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate tokio_core;
extern crate toml;

mod xqueue;

use std::process;

mod errors {
    error_chain! {
        foreign_links {
            Hyper(::hyper::Error);
            URI(::http::uri::InvalidUri);
        }
    }

    pub type Future<T> = ::futures::Future<Item = T, Error = Error>;
}

#[derive(Deserialize)]
struct Config {
    xqueue_base_url: String,
}

fn main() {
    env_logger::init();
    info!("starting");
    if let Err(e) = run() {
        error!("existing because of {}", e);
        process::exit(1);
    }
}

fn run() -> errors::Result<()> {
    Err("unimplemented".into())
}

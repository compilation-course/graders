#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate futures_timer;
extern crate hyper;
extern crate lapin_futures;
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
            URI(::hyper::error::UriError);
        }
    }

    pub type Future<T> = ::futures::Future<Item = T, Error = Error>;
}

#[derive(Deserialize)]
struct Config {
    xqueue_base_url: String,
}

fn main() {
    match run() {
        Ok(_) => (),
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}

fn run() -> errors::Result<()> {
    Err("unimplemented".into())
}

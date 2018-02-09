#[macro_use] extern crate error_chain;
extern crate futures;
extern crate hyper;
extern crate lapin_futures;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate tokio_core;
extern crate toml;

mod xqueue;

use std::process;

mod errors {
    error_chain! {
    }
}

#[derive(Deserialize)]
struct Config {
    xqueue_login_url: String,
    xqueue_logout_url: String,
    xqueue_status_url: String,
    xqueue_submit_url: String,
    xqueue_get_queuelen_url: String,
    xqueue_get_submission_url: String,
    xqueue_put_result_url: String,
}

fn main() {
    match run() {
        Ok(_) => (),
        Err(e) => { eprintln!("Error: {}", e); process::exit(1); }
    }
}

fn run() -> errors::Result<()> {
    Err("unimplemented".into())
}

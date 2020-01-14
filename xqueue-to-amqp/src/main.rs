#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

mod xqueue;

use failure::Error;
use std::process;

#[derive(Deserialize)]
struct Config {
    xqueue_base_url: String,
}

fn main() {
    env_logger::init();
    info!("starting");
    if let Err(e) = run() {
        error!("exiting because of {}", e);
        process::exit(1);
    }
}

fn run() -> Result<(), Error> {
    Err(format_err!("unimplemented"))
}

#[macro_use]
extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate graders_utils;
extern crate lapin_futures as lapin;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate tokio;
extern crate toml;

mod amqp;
mod config;
mod errors;

use clap::App;
use config::Configuration;

fn configuration() -> errors::Result<Configuration> {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    config::load_configuration(matches.value_of("config").unwrap())
}

fn main() {
    env_logger::init();
    info!("starting");
    run().unwrap()
}

fn run() -> errors::Result<()> {
    let config = configuration()?;
    Ok(())
}

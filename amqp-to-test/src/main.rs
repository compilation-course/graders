#[macro_use]
extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate futures_cpupool;
extern crate graders_utils;
extern crate lapin_futures as lapin;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate serde_yaml;
extern crate tokio;
extern crate toml;

mod amqp;
mod config;
mod errors;
mod tester;

use clap::App;
use config::Configuration;
use futures::sync::mpsc;
use std::sync::Arc;
use tokio::executor::current_thread;

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
    let config = Arc::new(configuration()?);
    let (send_request, receive_request) = mpsc::channel(16);
    let (send_response, receive_response) = mpsc::channel(16);
    let executor = tester::start_executor(&config, receive_request, send_response);
    let amqp_process = amqp::amqp_process(&config, send_request, receive_response);
    current_thread::run(|_| {
        current_thread::spawn(amqp_process);
        current_thread::spawn(executor);
    });
    Ok(())
}

#[macro_use]
extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate failure_derive;
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
extern crate tokio_current_thread;

mod amqp;
mod config;
mod tester;

use clap::App;
use config::Configuration;
use failure::Error;
use futures::sync::mpsc;
use futures::Future;
use std::sync::Arc;

fn configuration() -> Result<Configuration, Error> {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    config::load_configuration(matches.value_of("config").unwrap())
}

fn main() -> Result<(), Error> {
    env_logger::init();
    info!("starting");
    let config = Arc::new(configuration()?);
    let (send_request, receive_request) = mpsc::channel(16);
    let (send_response, receive_response) = mpsc::channel(16);
    let executor = tester::start_executor(&config, receive_request, send_response);
    let amqp_process = amqp::amqp_process(&config, send_request, receive_response);
    tokio::run(
        amqp_process
            .from_err::<Error>()
            .join(executor.then(|_| Err(format_err!("error in executor"))))
            .map(|(_, _): ((), ())| ())
            .map_err(|e| {
                error!("fatal error: {}", e);
            }),
    );
    Ok(())
}

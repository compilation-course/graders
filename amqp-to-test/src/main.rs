#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

mod amqp;
mod config;
mod tester;

use clap::{load_yaml, App};
use config::Configuration;
use failure::Error;
use futures::channel::mpsc;
use futures::try_join;
use futures::{FutureExt, TryFutureExt};
use std::sync::Arc;

fn configuration() -> Result<Configuration, Error> {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    config::load_configuration(matches.value_of("config").unwrap())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    info!("starting");
    let config = Arc::new(configuration()?);
    let (send_request, receive_request) = mpsc::channel(16);
    let (send_response, receive_response) = mpsc::channel(16);
    let executor = tester::start_executor(&config, receive_request, send_response).map(Ok);
    let amqp_process =
        amqp::amqp_process(&config, send_request, receive_response).err_into::<Error>();
    try_join!(executor, amqp_process)?;
    Ok(())
}

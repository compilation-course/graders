mod amqp;
mod config;
mod tester;

use clap::{arg, command};
use config::Configuration;
use failure::Error;
use futures::channel::mpsc;
use futures::try_join;
use futures::{FutureExt, TryFutureExt};
use std::sync::Arc;

fn configuration() -> Result<Configuration, Error> {
    let matches = command!()
        .arg(arg!(-c --config <FILE> "Configuration file containing credentials"))
        .get_matches();
    config::load_configuration(matches.get_one::<String>("config").unwrap())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    log::info!("starting");
    let config = Arc::new(configuration()?);
    let (send_request, receive_request) = mpsc::channel(16);
    let (send_response, receive_response) = mpsc::channel(16);
    let executor = tester::start_executor(&config, receive_request, send_response).map(Ok);
    let amqp_process =
        amqp::amqp_process(&config, send_request, receive_response).err_into::<Error>();
    try_join!(executor, amqp_process)?;
    Ok(())
}

mod amqp;
mod config;
mod tester;

use amqp_utils::AmqpError;
use clap::{arg, command};
use config::{Configuration, ConfigurationError};
use futures::FutureExt;
use futures::channel::mpsc;
use futures::try_join;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("configuration error")]
    ConfigurationError(#[source] ConfigurationError),
    #[error("AMQP error")]
    AmqpError(AmqpError),
}

fn configuration() -> Result<Configuration, ConfigurationError> {
    let matches = command!()
        .arg(arg!(-c --config <FILE> "Configuration file containing credentials").required(true))
        .get_matches();
    config::load_configuration(matches.get_one::<String>("config").unwrap())
}

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    env_logger::init();
    color_eyre::install()?;
    log::info!("starting");
    let config = Arc::new(configuration()?);
    let (send_request, receive_request) = mpsc::channel(16);
    let (send_response, receive_response) = mpsc::channel(16);
    let executor = tester::start_executor(&config, receive_request, send_response).map(Ok);
    let amqp_process = amqp::amqp_process(&config, send_request, receive_response);
    try_join!(executor, amqp_process)?;
    Ok(())
}

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

mod amqp;
mod config;
mod gitlab;
mod poster;
mod report;
mod web;

use clap::{arg, command};
use config::Configuration;
use failure::Error;
use futures::channel::mpsc;
use futures::{stream, try_join, StreamExt, TryFutureExt, TryStreamExt};
use std::process;
use std::sync::Arc;
use tokio::sync::Semaphore;

fn configuration() -> Result<Configuration, Error> {
    let matches = command!()
        .arg(arg!(-c --config <FILE> "Configuration file containing credentials"))
        .get_matches();
    let config = config::load_configuration(matches.get_one::<String>("config").unwrap())?;
    config::setup_dirs(&config)?;
    Ok(config)
}

async fn run() -> Result<(), Error> {
    let config = Arc::new(configuration()?);
    info!(
        "configured for labs {:?}",
        config
            .labs
            .iter()
            .filter(|l| l.is_enabled())
            .map(|l| &l.name)
            .collect::<Vec<_>>()
    );
    let cpu_access = Semaphore::new(config.package.threads);
    let (send_hook, receive_hook) = mpsc::channel(16);
    let (send_request, receive_request) = mpsc::channel(16);
    let (send_response, receive_response) = mpsc::channel(16);
    let packager = gitlab::packager(&config, &cpu_access, receive_hook, send_request);
    let amqp_process =
        amqp::amqp_process(&config, receive_request, send_response).map_err(|e| e.into());
    let response_poster = receive_response
        .map(Ok)
        .try_for_each_concurrent(None, |response| {
            let cloned_config = config.clone();
            async move {
                trace!("Received reponse: {:?}", response);
                match report::response_to_post(&cloned_config, &response) {
                    Ok(rqs) => {
                        stream::iter(rqs)
                            .for_each_concurrent(None, |rq| async {
                                if let Err(e) = poster::post(rq).await {
                                    warn!("unable to post response: {}", e);
                                }
                            })
                            .await;
                        Ok(())
                    }
                    Err(e) => {
                        error!("unable to build response: {}", e);
                        Err(e)
                    }
                }
            }
        });
    let web_server = web::web_server(&config, send_hook);
    try_join!(packager, amqp_process, response_poster, web_server)?;
    Ok(())
}

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("starting");
    if let Err(e) = run().await {
        error!("exiting because of {}", e);
        process::exit(1);
    }
}

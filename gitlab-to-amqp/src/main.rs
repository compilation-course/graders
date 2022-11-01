mod amqp;
mod config;
mod gitlab;
mod poster;
mod report;
mod web;

use clap::{arg, command};
use config::Configuration;
use futures::channel::mpsc;
use futures::{stream, try_join, StreamExt, TryFutureExt, TryStreamExt};
use std::sync::Arc;
use tokio::sync::Semaphore;

fn configuration() -> eyre::Result<Configuration> {
    let matches = command!()
        .arg(arg!(-c --config <FILE> "Configuration file containing credentials").required(true))
        .get_matches();
    let config = config::load_configuration(matches.get_one::<String>("config").unwrap())?;
    config::setup_dirs(&config)?;
    Ok(config)
}

async fn run() -> eyre::Result<()> {
    let config = Arc::new(configuration()?);
    log::info!(
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
                log::trace!("Received reponse: {:?}", response);
                match report::response_to_post(&cloned_config, &response) {
                    Ok(rqs) => {
                        stream::iter(rqs)
                            .for_each_concurrent(None, |rq| async {
                                if let Err(e) = poster::post(rq).await {
                                    log::warn!("unable to post response: {}", e);
                                }
                            })
                            .await;
                        Ok(())
                    }
                    Err(e) => {
                        log::error!("unable to build response: {}", e);
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
async fn main() -> eyre::Result<()> {
    env_logger::init();
    color_eyre::install()?;
    log::info!("starting");
    run().await
}

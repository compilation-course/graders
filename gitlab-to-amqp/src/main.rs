#[macro_use]
extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate futures_cpupool;
extern crate git2;
extern crate graders_utils;
extern crate hyper;
extern crate hyper_tls;
extern crate lapin_futures as lapin;
#[macro_use]
extern crate log;
extern crate mktemp;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate serde_yaml;
extern crate tokio;
extern crate url;
extern crate url_serde;
extern crate uuid;

mod amqp;
mod config;
mod errors;
mod gitlab;
mod poster;
mod report;
mod web;

use clap::App;
use config::Configuration;
use futures::sync::mpsc;
use futures::*;
use futures_cpupool::CpuPool;
use std::process;
use std::sync::Arc;
use tokio::runtime::current_thread;

fn configuration() -> errors::Result<Configuration> {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    let config = config::load_configuration(matches.value_of("config").unwrap())?;
    config::setup_dirs(&config)?;
    Ok(config)
}

fn run() -> errors::Result<()> {
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
    let cpu_pool = CpuPool::new(config.package.threads);
    let (send_hook, receive_hook) = mpsc::channel(16);
    let (send_request, receive_request) = mpsc::channel(16);
    let (send_response, receive_response) = mpsc::channel(16);
    let packager = gitlab::packager(&config, &cpu_pool, receive_hook, send_request);
    let amqp_process = amqp::amqp_process(&config, receive_request, send_response).map_err(|_| ());
    let cloned_config = config.clone();
    // Using a CPU pool to post responses is a bit dirty, but since we cannot hop threads
    // with the posting it is simpler at this time, so let's be synchronous.
    let post_cpu_pool = CpuPool::new(1);
    let response_poster = receive_response.for_each(move |response| {
        trace!("Received reponse: {:?}", response);
        match report::response_to_post(&cloned_config, &response) {
            Ok(rqs) => rqs.into_iter().for_each(|rq| {
                match post_cpu_pool
                    .spawn_fn(|| current_thread::block_on_all(poster::post(rq).map(|_| ())))
                    .wait()
                {
                    Ok(_) => (),
                    Err(e) => warn!("unable to post response: {}", e),
                }
            }),
            Err(e) => error!("could not build response to post: {}", e),
        }
        future::ok(())
    });
    let web_server = web::web_server(&cpu_pool, &config, send_hook).map_err(|_| ());
    tokio::run(
        packager
            .join4(amqp_process, response_poster, web_server)
            .map(|_| ()),
    );
    Ok(())
}

fn main() {
    env_logger::init();
    info!("starting");
    if let Err(e) = run() {
        error!("exiting because of {}", e);
        process::exit(1);
    }
}

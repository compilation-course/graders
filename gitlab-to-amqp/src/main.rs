#[macro_use]
extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate futures_cpupool;
extern crate git2;
extern crate graders_utils;
#[macro_use]
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
extern crate tokio_core;
extern crate tokio_current_thread;
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
use std::thread;

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
    let cloned_config = config.clone();
    let cloned_cpu_pool = cpu_pool.clone();
    thread::spawn(move || {
        if let Err(e) = tokio_current_thread::block_on_all({
            let packager =
                gitlab::packager(&cloned_config, &cloned_cpu_pool, receive_hook, send_request);
            let amqp_process = amqp::amqp_process(&cloned_config, receive_request, send_response);
            let parrot = receive_response.for_each(move |response| {
                trace!("Received reponse: {:?}", response);
                match report::response_to_post(&cloned_config, &response) {
                    Ok(rqs) => rqs
                        .into_iter()
                        .for_each(|rq| poster::post(&cloned_cpu_pool, rq)),
                    Err(e) => error!("could not build response to post: {}", e),
                }
                future::ok(())
            });
            packager
                .join3(
                    amqp_process.map_err(|e| {
                        error!("AMQP process error: {}", e);
                        ()
                    }),
                    parrot,
                ).map(|_| ())
        }) {
            error!("exiting because a fatal error occurred: {:?}", e);
            process::exit(1);
        }
    });
    match web::web_server_thread(&cpu_pool, &config, send_hook).join() {
        Ok(r) => r,
        Err(_) => Err(errors::ErrorKind::WebServerCrash)?,
    }
}

fn main() {
    env_logger::init();
    info!("starting");
    if let Err(e) = run() {
        error!("exiting because of {}", e);
        process::exit(1);
    }
}

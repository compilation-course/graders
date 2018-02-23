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
extern crate lapin_futures as lapin;
#[macro_use]
extern crate log;
extern crate mktemp;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate serde_yaml;
extern crate tokio;
extern crate toml;
extern crate url;
extern crate url_serde;

mod amqp;
mod config;
mod errors;
mod gitlab;
mod report;

use clap::App;
use config::Configuration;
use futures::*;
use futures::sync::mpsc::{self, Sender};
use futures_cpupool::CpuPool;
use gitlab::GitlabHook;
use graders_utils::fileutils;
use hyper::{Method, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::process;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;
use tokio::executor::current_thread;

static NOT_FOUND: &str = "Not found, try something else";

fn configuration() -> errors::Result<Configuration> {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    let config = config::load_configuration(matches.value_of("config").unwrap())?;
    config::setup_dirs(&config)?;
    Ok(config)
}

fn run() -> errors::Result<()> {
    let config = Arc::new(configuration()?);
    let addr = SocketAddr::new(config.server.ip, config.server.port);
    info!(
        "will listen on {:?} with base URL of {}",
        addr, config.server.base_url
    );
    let cpu_pool = CpuPool::new(config.package.threads);
    let (send_hook, receive_hook) = mpsc::channel(16);
    let (send_request, receive_request) = mpsc::channel(16);
    let (send_response, receive_response) = mpsc::channel(16);
    let cloned_config = config.clone();
    let cloned_cpu_pool = cpu_pool.clone();
    thread::spawn(move || {
        current_thread::run(|_| {
            let packager =
                gitlab::packager(&cloned_config, &cloned_cpu_pool, receive_hook, send_request);
            let amqp_process = amqp::amqp_process(&cloned_config, receive_request, send_response);
            let parrot = receive_response.for_each(|response| {
                info!("Received reponse: {:?}", response);
                info!(
                    "Report would look like: {:?}",
                    report::yaml_to_markdown(&response.step, &response.yaml_result)
                );
                future::ok(())
            });
            current_thread::spawn(
                packager
                    .join3(
                        amqp_process.map_err(|e| {
                            error!("AMQP process error: {}", e);
                            ()
                        }),
                        parrot,
                    )
                    .map(|_| ())
                    .map_err(|_| {
                        error!("exiting because a fatal error occurred");
                        process::exit(1);
                    }),
            );
        });
    });
    let gitlab_service = Rc::new(GitlabService::new(&cpu_pool, &config, send_hook));
    let server = Http::new().bind(&addr, move || Ok(gitlab_service.clone()))?;
    server.run()?;
    Ok(())
}

fn main() {
    env_logger::init();
    info!("starting");
    run().unwrap();
}

struct GitlabService {
    cpu_pool: CpuPool,
    config: Arc<Configuration>,
    send_request: Sender<GitlabHook>,
}

impl GitlabService {
    fn new(
        cpu_pool: &CpuPool,
        config: &Arc<Configuration>,
        send_request: Sender<GitlabHook>,
    ) -> GitlabService {
        GitlabService {
            cpu_pool: cpu_pool.clone(),
            config: config.clone(),
            send_request,
        }
    }
}

impl Service for GitlabService {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<future::Future<Item = Response, Error = hyper::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        trace!("got {} {}", req.method(), req.path());
        match (req.method(), req.path()) {
            (&Method::Post, "/push") => {
                let send_request = self.send_request.clone();
                Box::new(
                    req.body()
                        .concat2()
                        .and_then(|b| {
                            trace!("decoding body");
                            serde_json::from_slice::<gitlab::GitlabHook>(b.as_ref()).map_err(|e| {
                                error!("error when decoding body: {}", e);
                                hyper::Error::Status
                            })
                        })
                        .map(move |hook| {
                            trace!("received json and will pass it around: {:?}", hook);
                            current_thread::run(|_| {
                                let hook_clone = hook.clone();
                                current_thread::spawn(
                                    send_request
                                        .clone()
                                        .send(hook)
                                        .map(|_| ())
                                        .map_err(move |e| {
                                            error!(
                                                "unable to send hook {:?} around: {}",
                                                hook_clone, e
                                            );
                                            ()
                                        }),
                                )
                            });
                            Response::<hyper::Body>::new().with_status(StatusCode::NoContent)
                        }),
                )
            }
            (&Method::Get, _)
                if req.path().starts_with("/zips/") && !fileutils::contains_dotdot(req.path()) =>
            {
                let path = Path::new(req.path());
                let zip_dir = Path::new(&self.config.package.zip_dir);
                let zip_file = zip_dir.join(Path::new(path).strip_prefix("/zips/").unwrap());
                if zip_file.is_file() {
                    debug!("serving {}", req.path());
                    Box::new(
                        self.cpu_pool
                            .spawn_fn(move || {
                                let mut content =
                                    Vec::with_capacity(zip_file.metadata()?.len() as usize);
                                File::open(&zip_file)?.read_to_end(&mut content)?;
                                std::fs::remove_file(&zip_file)?;
                                Ok(content)
                            })
                            .map(|content| {
                                Response::<hyper::Body>::new()
                                    .with_header(ContentType("application/zip".parse().unwrap()))
                                    .with_header(ContentLength(content.len() as u64))
                                    .with_body(content)
                            }),
                    )
                } else {
                    warn!("unable to serve {}", req.path());
                    not_found()
                }
            }
            _ => not_found(),
        }
    }
}

fn not_found() -> Box<Future<Item = Response, Error = hyper::Error>> {
    Box::new(future::ok(
        Response::<hyper::Body>::new()
            .with_status(StatusCode::NotFound)
            .with_header(ContentLength(NOT_FOUND.len() as u64))
            .with_header(ContentType::plaintext())
            .with_body(NOT_FOUND),
    ))
}

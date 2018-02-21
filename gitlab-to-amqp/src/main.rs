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
extern crate lapin_futures as lapin;
#[macro_use]
extern crate log;
extern crate mktemp;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate tokio;
extern crate toml;
extern crate url;
extern crate url_serde;

mod config;
mod errors;
mod gitlab;

use clap::App;
use config::Configuration;
use futures::*;
use futures::sync::mpsc::{self, Receiver, Sender};
use futures_cpupool::CpuPool;
use gitlab::GitlabHook;
use graders_utils::fileutils;
use graders_utils::amqputils::{self, AMQPRequest, AMQPResponse};
use hyper::{Method, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};
use lapin::channel::*;
use lapin::types::FieldTable;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;
use tokio::executor::current_thread;
use tokio::reactor::Handle;

static NOT_FOUND: &str = "Not found, try something else";
static TEXT: &str = "Hello, World!";

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
    let cpu_pool = Arc::new(CpuPool::new(config.package.threads));
    let send_request = start_amqp_thread(&config);
    let gitlab_service = Rc::new(GitlabService::new(&cpu_pool, &config, send_request));
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
    cpu_pool: Arc<CpuPool>,
    config: Arc<Configuration>,
    send_request: Sender<AMQPRequest<GitlabHook>>,
}

impl GitlabService {
    fn new(
        cpu_pool: &Arc<CpuPool>,
        config: &Arc<Configuration>,
        send_request: Sender<AMQPRequest<GitlabHook>>,
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
                let config = self.config.clone();
                let cpu_pool = self.cpu_pool.clone();
                let send_request = self.send_request.clone();
                let base_url = config.server.base_url.clone();
                Box::new(
                    req.body()
                        .concat2()
                        .and_then(|b| {
                            serde_json::from_slice::<gitlab::GitlabHook>(b.as_ref())
                                .map_err(|_| hyper::Error::Status)
                        })
                        .map(move |hook| {
                            trace!("received json and will proceed to checkout: {:?}", hook);
                            let clone_hook = hook.clone();
                            let process = cpu_pool
                                .spawn_fn(move || {
                                    gitlab::package(&config, &clone_hook).map_err(|e| {
                                        warn!("could not clone {:?}: {}", clone_hook.url(), e);
                                        e
                                    })
                                })
                                .map(move |labs| {
                                    let items = stream::iter_ok(labs.into_iter().map(
                                        move |(step, zip)| {
                                            AMQPRequest {
                                                step: step,
                                                zip_url: base_url
                                                    .join("zips/")
                                                    .unwrap()
                                                    .join(&zip)
                                                    .unwrap()
                                                    .to_string(),
                                                result_queue: "gitlab".to_owned(),
                                                opaque: hook.clone(),
                                            }
                                        },
                                    ));
                                    let send = send_request.clone().send_all(items).map_err(|e| {
                                        error!("unable to send items to AMQP thread: {}", e);
                                        ()
                                    });
                                    current_thread::spawn(send.map(|_| ()));
                                });
                            current_thread::run(|_| current_thread::spawn(process.map_err(|_| ())));
                            Response::<hyper::Body>::new()
                                .with_header(ContentLength(TEXT.len() as u64))
                                .with_header(ContentType::plaintext())
                                .with_body(TEXT)
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

fn start_amqp_thread(config: &Arc<Configuration>) -> Sender<AMQPRequest<GitlabHook>> {
    let (send_request, receive_request) = mpsc::channel(16);
    let config = config.clone();
    thread::spawn(move || run_amqp(config, receive_request));
    send_request
}

fn run_amqp(config: Arc<Configuration>, receive_request: Receiver<AMQPRequest<GitlabHook>>) {
    current_thread::run(move |_| {
        current_thread::spawn(
            {
                debug!("connecting to AMQP server");
                amqputils::create_client(&Handle::default(), &config.amqp)
            }.and_then(|client| {
                debug!("creating AMQP channel for publication");
                client.create_channel().and_then(|channel| {
                    debug!("creating AMQP exchange");
                    channel
                        .exchange_declare(
                            &config.amqp.exchange,
                            "direct",
                            &ExchangeDeclareOptions::default(),
                            &FieldTable::new(),
                        )
                        .map(|_| {
                            current_thread::spawn(
                                receive_request
                                .map(move |req| {
                                    trace!("handling incoming AMQP publishing request: {:?}", req);
                                    channel.basic_publish(
                                        &config.amqp.exchange,
                                        &config.amqp.routing_key,
                                        serde_json::to_string(&req).unwrap().as_bytes(),
                                        &BasicPublishOptions::default(),
                                        BasicProperties::default(),
                                        )
                                })
                                // Switch for a fold or a ignore
                                .collect()
                                .map(|_| ())
                                .map_err(|_| ()),
                            )
                        })
                })
            })
                .then(|r| {
                    if let Err(e) = r {
                        error!("error in AMQP thread: {}", e);
                    }
                    future::ok(())
                }),
        )
    });
}

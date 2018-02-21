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
#[macro_use]
extern crate log;
extern crate mktemp;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate toml;
extern crate url;
extern crate url_serde;

mod config;
mod errors;
mod gitlab;

use clap::App;
use futures::*;
use futures_cpupool::CpuPool;
use graders_utils::{amqputils, fileutils};
use hyper::{Method, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

static NOT_FOUND: &str = "Not found, try something else";
static TEXT: &str = "Hello, World!";

fn run() -> errors::Result<()> {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    let config = config::load_configuration(matches.value_of("config").unwrap()).unwrap();
    config::setup_dirs(&config)?;
    let addr = SocketAddr::new(config.server.ip, config.server.port);
    info!("will listen on {:?} with base URL of {}", addr, config.server.base_url);
    let gi = Rc::new(GitlabInterface::new(config));
    setup();
    let server = Http::new().bind(&addr, move || Ok(gi.clone()))?;
    server.run()?;
    Ok(())
}

fn main() {
    env_logger::init();
    info!("starting");
    run().unwrap();
}

struct GitlabInterface {
    cpu_pool: Rc<CpuPool>,
    config: Arc<config::Configuration>,
}

impl GitlabInterface {
    fn new(config: config::Configuration) -> GitlabInterface {
        GitlabInterface {
            cpu_pool: Rc::new(CpuPool::new(config.package.threads)),
            config: Arc::new(config),
        }
    }
}

impl Service for GitlabInterface {
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
                Box::new(
                    req.body()
                        .concat2()
                        .and_then(|b| {
                            serde_json::from_slice::<gitlab::GitlabHook>(b.as_ref())
                                .map_err(|_| hyper::Error::Status)
                        })
                        .map(move |hook| {
                            trace!("received json and will proceed to checkout: {:?}", hook);
                            cpu_pool
                                .spawn_fn(move || {
                                    gitlab::package(&config, &hook).map_err(|e| {
                                        warn!("could not clone {:?}: {}", hook.url(), e);
                                        e
                                    })
                                })
                                .forget();
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

fn setup() {}

use config::Configuration;
use errors;
use futures::*;
use futures::sync::mpsc::Sender;
use futures_cpupool::CpuPool;
use gitlab::GitlabHook;
use graders_utils::fileutils;
use hyper::{Body, Error as HyperError, Method, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Request, Response, Service};
use serde_json;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tokio::executor::current_thread;

static NOT_FOUND: &str = "Not found, try something else";

pub fn web_server_thread(
    cpu_pool: &CpuPool,
    config: &Arc<Configuration>,
    send_hook: Sender<GitlabHook>,
) -> JoinHandle<errors::Result<()>> {
    let cpu_pool = cpu_pool.clone();
    let config = config.clone();
    thread::spawn(move || {
        let addr = SocketAddr::new(config.server.ip, config.server.port);
        info!(
            "will listen on {:?} with base URL of {}",
            addr, config.server.base_url
        );
        let gitlab_service = Rc::new(GitlabService::new(&cpu_pool, &config, send_hook));
        let server = Http::new().bind(&addr, move || Ok(gitlab_service.clone()))?;
        server.run()?;
        Ok(())
    })
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
    type Error = HyperError;
    type Future = Box<future::Future<Item = Response, Error = HyperError>>;

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
                            serde_json::from_slice::<GitlabHook>(b.as_ref()).map_err(|e| {
                                error!("error when decoding body: {}", e);
                                HyperError::Status
                            })
                        })
                        .map(move |hook| {
                            trace!("received json and will pass it around: {:?}", hook);
                            if let Err(e) = current_thread::block_on_all({
                                send_request.clone().send(hook.clone()).map(|_| ())
                            }) {
                                error!("unable to send hook {:?} around: {}", hook, e);
                            }
                            Response::<Body>::new().with_status(StatusCode::NoContent)
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
                                Ok(content)
                            })
                            .map(|content| {
                                Response::<Body>::new()
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

fn not_found() -> Box<Future<Item = Response, Error = HyperError>> {
    Box::new(future::ok(
        Response::<Body>::new()
            .with_status(StatusCode::NotFound)
            .with_header(ContentLength(NOT_FOUND.len() as u64))
            .with_header(ContentType::plaintext())
            .with_body(NOT_FOUND),
    ))
}

use futures::sync::mpsc::Sender;
use futures::*;
use futures_cpupool::CpuPool;
use graders_utils::fileutils;
use hyper::header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE};
use hyper::service::Service;
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use serde_json;
use std::fs::File;
use std::io::{self, Read};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio;

use crate::config::Configuration;
use crate::gitlab::GitlabHook;

pub fn web_server(
    cpu_pool: &CpuPool,
    config: &Arc<Configuration>,
    send_hook: Sender<GitlabHook>,
) -> Box<dyn Future<Item = (), Error = io::Error> + Send + 'static> {
    let cpu_pool = cpu_pool.clone();
    let config = config.clone();
    let addr = SocketAddr::new(config.server.ip, config.server.port);
    info!(
        "will listen on {:?} with base URL of {}",
        addr, config.server.base_url
    );
    let gitlab_service = GitlabService::new(&cpu_pool, &config, send_hook);
    Box::new(
        Server::bind(&addr)
            .serve(move || Ok(gitlab_service.clone()).map_err(|e: io::Error| e))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)),
    )
}

#[derive(Clone)]
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
    type ReqBody = Body;
    type ResBody = Body;
    type Error = io::Error;
    type Future =
        Box<dyn Future<Item = Response<Self::ResBody>, Error = Self::Error> + Send + 'static>;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        let (head, body) = req.into_parts();
        trace!("got {} {}", head.method, head.uri.path());
        match (head.method, head.uri.path()) {
            (Method::POST, "/push") => {
                let send_request = self.send_request.clone();
                Box::new(
                    body.concat2()
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
                        .and_then(|b| {
                            serde_json::from_slice::<GitlabHook>(b.as_ref()).map_err(|e| {
                                error!("error when decoding body: {}", e);
                                io::Error::new(io::ErrorKind::Other, e)
                            })
                        })
                        .map(move |hook| {
                            if hook.object_kind != "push" {
                                trace!(
                                    "received unknown object kind for {}: {}",
                                    hook.desc(),
                                    hook.object_kind
                                );
                            } else if !hook.is_delete() {
                                trace!("received json and will pass it around: {:?}", hook);
                                tokio::spawn(
                                    send_request.clone().send(hook.clone()).map(|_| ()).map_err(
                                        move |e| {
                                            error!("unable to send hook {:?} around: {}", hook, e);
                                        },
                                    ),
                                );
                            } else {
                                debug!("branch deletion event for {}", hook.desc());
                            }
                            Response::builder()
                                .status(StatusCode::NO_CONTENT)
                                .body(Body::empty())
                                .unwrap()
                        }),
                )
            }
            (Method::GET, path)
                if path.starts_with("/zips/") && !fileutils::contains_dotdot(path) =>
            {
                let path = Path::new(path);
                let zip_dir = Path::new(&self.config.package.zip_dir);
                let zip_file = zip_dir.join(Path::new(path).strip_prefix("/zips/").unwrap());
                if zip_file.is_file() {
                    debug!("serving {:?}", path);
                    Box::new(
                        self.cpu_pool
                            .spawn_fn(move || {
                                let mut content =
                                    Vec::with_capacity(zip_file.metadata()?.len() as usize);
                                File::open(&zip_file)?.read_to_end(&mut content)?;
                                Ok(content)
                            })
                            .map(|content| {
                                Response::builder()
                                    .header(
                                        CONTENT_TYPE,
                                        HeaderValue::from_static("application/zip"),
                                    )
                                    .header(CONTENT_LENGTH, content.len())
                                    .body(Body::from(content))
                                    .unwrap()
                            }),
                    )
                } else {
                    warn!("unable to serve {:?}", path);
                    not_found()
                }
            }
            _ => not_found(),
        }
    }
}

fn not_found() -> Box<dyn Future<Item = Response<Body>, Error = io::Error> + Send + 'static> {
    Box::new(future::ok(
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    ))
}

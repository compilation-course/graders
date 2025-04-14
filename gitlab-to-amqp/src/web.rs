use eyre::Context;
use futures::SinkExt;
use futures::channel::mpsc::Sender;
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderValue};
use hyper::server::conn::AddrStream;
use hyper::service;
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use std::convert::TryInto;
use std::ffi::OsStr;
use std::net::SocketAddr;
use std::path::{Component, Path};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::config::Configuration;
use crate::gitlab::GitlabHook;

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::too_many_lines)]
pub async fn web_server(
    config: &Arc<Configuration>,
    send_hook: Sender<GitlabHook>,
) -> eyre::Result<()> {
    let config = config.clone();
    let addr = SocketAddr::new(config.server.ip, config.server.port);
    log::info!(
        "will listen on {:?} with base URL of {}",
        addr,
        config.server.base_url
    );
    let make_svc = service::make_service_fn(|socket: &AddrStream| {
        log::debug!("connection from {:?}", socket.remote_addr());
        let send_hook = send_hook.clone();
        let config = config.clone();
        async move {
            Ok::<_, eyre::Report>(service::service_fn(move |req: Request<Body>| {
                let send_hook = send_hook.clone();
                let config = config.clone();
                async move {
                    let (head, body) = req.into_parts();
                    log::trace!("got {} {}", head.method, head.uri.path());
                    match (head.method, head.uri.path()) {
                        (Method::POST, "/push") => {
                            let body = hyper::body::to_bytes(body).await?;
                            let hook = serde_json::from_slice::<GitlabHook>(&body)
                                .map_err(|e| {
                                    log::error!("error when decoding body: {}", e);
                                    e
                                })
                                .with_context(|| "error when decoding body")?;
                            if let Some(secret_token) = config.gitlab.secret_token.clone() {
                                if let Some(from_request) = head
                                    .headers
                                    .get("X-Gitlab-Token")
                                    .and_then(|h| h.to_str().ok())
                                {
                                    if secret_token != from_request {
                                        log::error!("incorrect secret token sent to the hook");
                                        return Ok::<_, eyre::Report>(
                                            Response::builder()
                                                .status(StatusCode::FORBIDDEN)
                                                .body(Body::from("incorrect secret token"))?,
                                        );
                                    }
                                } else {
                                    log::error!("missing secret token with hook");
                                    return Ok::<_, eyre::Report>(
                                        Response::builder()
                                            .status(StatusCode::FORBIDDEN)
                                            .body(Body::from("missing secret token"))?,
                                    );
                                }
                            }
                            if hook.object_kind != "push" {
                                log::trace!(
                                    "received unknown object kind for {}: {}",
                                    hook.desc(),
                                    hook.object_kind
                                );
                            } else if hook.is_delete() {
                                log::debug!("branch deletion event for {}", hook.desc());
                            } else {
                                log::trace!("received json and will pass it around: {:?}", hook);
                                let mut send_hook = send_hook.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = send_hook.send(hook.clone()).await {
                                        log::error!("unable to send hook {:?} around: {}", hook, e);
                                    }
                                });
                            }
                            Ok::<_, eyre::Report>(
                                Response::builder()
                                    .status(StatusCode::NO_CONTENT)
                                    .body(Body::empty())?,
                            )
                        }
                        (Method::GET, path) if is_acceptable_path_name(path) => {
                            let path = Path::new(path);
                            let zip_dir = Path::new(&config.package.zip_dir);
                            let zip_file =
                                zip_dir.join(Path::new(path).strip_prefix("/zips/").unwrap());
                            if zip_file.is_file() {
                                log::debug!("serving {:?}", path);
                                let mut content =
                                    Vec::with_capacity(zip_file.metadata()?.len().try_into()?);
                                File::open(&zip_file)
                                    .await?
                                    .read_to_end(&mut content)
                                    .await?;
                                Ok(Response::builder()
                                    .header(
                                        CONTENT_TYPE,
                                        HeaderValue::from_static("application/zip"),
                                    )
                                    .header(CONTENT_LENGTH, content.len())
                                    .body(Body::from(content))?)
                            } else {
                                log::warn!("unable to serve {:?}", path);
                                Ok(not_found())
                            }
                        }
                        (method, path) => {
                            log::info!("unknown incoming request {:?} for {}", method, path);
                            Ok(not_found())
                        }
                    }
                }
            }))
        }
    });
    // let make_svc = service::make_service_fn(|_| async move { Ok::<_, Infallible>(svc) });
    Server::bind(&addr).serve(make_svc).await?;
    Ok(())
}

/// Check that the path starts with /zips/ and does not try to get
/// out of this hierarchy.
fn is_acceptable_path_name(path: &str) -> bool {
    let mut path = Path::new(path).components();
    path.next() == Some(Component::RootDir)
        && path.next() == Some(Component::Normal(OsStr::new("zips")))
        && path.all(|c| c != Component::ParentDir)
}

#[test]
fn test_is_acceptable_path_name() {
    assert!(is_acceptable_path_name("/zips/foo"));
    assert!(!is_acceptable_path_name("zips/foo"));
    assert!(!is_acceptable_path_name("foo/bar"));
    assert!(!is_acceptable_path_name("/zips/../foo"));
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

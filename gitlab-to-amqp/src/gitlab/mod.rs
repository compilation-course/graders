pub mod api;

use config::Configuration;
use errors;
use futures::{Future, Sink};
use futures::stream::{self, Stream};
use futures_cpupool::CpuPool;
use futures::sync::mpsc::{Receiver, Sender};
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use git2::build::{CheckoutBuilder, RepoBuilder};
use graders_utils::amqputils::AMQPRequest;
use graders_utils::ziputils::zip_recursive;
use mktemp::Temp;
use poster;
use self::api::State;
use serde_json;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;
use url::Url;
use url_serde;
use uuid::Uuid;

static GITLAB_USERNAME: &str = "grader";
pub static RESULT_QUEUE: &str = "gitlab";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GitlabHook {
    object_kind: String,
    checkout_sha: String,
    project_id: u32,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub repository: GitlabRepository,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GitlabRepository {
    name: String,
    #[serde(with = "url_serde")]
    homepage: Url,
    #[serde(with = "url_serde")]
    git_http_url: Url,
}

impl GitlabHook {
    pub fn url(&self) -> &Url {
        &self.repository.git_http_url
    }
}

fn clone(token: &str, hook: &GitlabHook, dir: &Path) -> errors::Result<Repository> {
    let token_for_clone = token.to_owned();
    let mut callbacks = RemoteCallbacks::new();
    callbacks
        .credentials(move |_, _, _| Cred::userpass_plaintext(GITLAB_USERNAME, &token_for_clone));
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    trace!("cloning {:?} into {:?}", hook.url(), dir);
    let repo = RepoBuilder::new()
        .fetch_options(fetch_options)
        .clone(&hook.url().to_string(), dir)?;
    {
        let head = repo.head()?;
        trace!("current head: {}", head.shorthand().unwrap_or("<unknown>"));
    }
    trace!("checkouting {}", hook.checkout_sha);
    {
        let rev = repo.revparse_single(&hook.checkout_sha)?;
        repo.checkout_tree(
            &rev,
            Some(&mut CheckoutBuilder::new()
                .force()
                .remove_untracked(true)
                .remove_ignored(true)),
        )?;
        repo.set_head_detached(rev.id())?;
    }
    Ok(repo)
}

/// Clone and package labs to test. Return a list of (lab, zip base name).
fn package(
    config: &Configuration,
    hook: &GitlabHook,
    cpu_pool: &CpuPool,
) -> errors::Result<Vec<(String, String)>> {
    let temp = Temp::new_dir()?;
    let root = temp.to_path_buf();
    let _repo = clone(&config.gitlab.token, hook, &root)?;
    let zip_dir = &Path::new(&config.package.zip_dir);
    let mut to_test = Vec::new();
    for lab in &config.labs {
        let path = root.join(&lab.base).join(&lab.dir);
        if path.is_dir()
            && lab.witness
                .clone()
                .map(|w| path.join(w).is_file())
                .unwrap_or(true)
        {
            poster::post(
                cpu_pool,
                api::post_status(
                    &config.gitlab,
                    hook,
                    &State::Running,
                    Some(&hook.ref_),
                    &lab.name,
                    Some("packaging and testing"),
                ),
            );
            trace!("packaging lab {} from {:?}", lab.name, path);
            let zip_basename = format!("{}.zip", Uuid::new_v4());
            let zip_file = zip_dir.join(&zip_basename);
            match zip_recursive(&path, &lab.dir, &zip_file) {
                Ok(_) => to_test.push((lab.name.clone(), zip_basename)),
                Err(e) => {
                    error!("cannot package {:?} (lab {}): {}", hook.url(), lab.name, e);
                    poster::post(
                        cpu_pool,
                        api::post_status(
                            &config.gitlab,
                            hook,
                            &State::Failed,
                            Some(&hook.ref_),
                            &lab.name,
                            Some("unable to package compiler"),
                        ),
                    );
                }
            }
        }
    }
    debug!("to test: {:?}", to_test);
    Ok(to_test)
}

fn labs_result_to_stream(
    base_url: &Url,
    hook: &GitlabHook,
    labs: Vec<(String, String)>,
) -> Box<Stream<Item = AMQPRequest, Error = ()>> {
    let hook = hook.clone();
    let base_url = base_url.clone();
    Box::new(stream::iter_ok(labs.into_iter().map(move |(lab, zip)| {
        AMQPRequest {
            job_name: format!(
                "[gitlab:{}:{}:{}:{}:{}]",
                &hook.repository.name,
                &hook.repository.homepage,
                &hook.ref_,
                &hook.checkout_sha,
                &lab
            ),
            lab: lab,
            zip_url: base_url
                .join("zips/")
                .unwrap()
                .join(&zip)
                .unwrap()
                .to_string(),
            result_queue: RESULT_QUEUE.to_owned(),
            opaque: to_opaque(&hook, &zip),
            delivery_tag: None,
        }
    })))
}

pub fn packager(
    config: &Arc<Configuration>,
    cpu_pool: &CpuPool,
    receive_hook: Receiver<GitlabHook>,
    send_request: Sender<AMQPRequest>,
) -> Box<Future<Item = (), Error = ()>> {
    let cpu_pool = cpu_pool.clone();
    let config = config.clone();
    Box::new(
        send_request
            .sink_map_err(|_| ())
            .send_all(
                receive_hook
                    .and_then(move |hook: GitlabHook| {
                        let clone_hook = hook.clone();
                        let base_url = config.server.base_url.clone();
                        let config = config.clone();
                        let cpu_pool_clone = cpu_pool.clone();
                        cpu_pool
                            .spawn_fn(move || {
                                package(&config, &clone_hook, &cpu_pool_clone).map_err(|_| ())
                            })
                            .map(move |labs| labs_result_to_stream(&base_url, &hook, labs))
                    })
                    .flatten(),
            )
            .map(|_| ()),
    )
}

pub fn remove_zip_file(config: &Configuration, zip: &String) -> io::Result<()> {
    fs::remove_file(config.package.zip_dir.join(zip))
}

pub fn to_opaque(hook: &GitlabHook, zip_file_name: &String) -> String {
    serde_json::to_string(&(hook, zip_file_name)).unwrap()
}

pub fn from_opaque(opaque: &String) -> errors::Result<(GitlabHook, String)> {
    Ok(serde_json::from_str(opaque)?)
}

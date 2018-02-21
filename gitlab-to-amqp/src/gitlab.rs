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
use std::path::Path;
use std::sync::Arc;
use url::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GitlabRepository {
    name: String,
    git_http_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GitlabHook {
    object_kind: String,
    checkout_sha: String,
    #[serde(rename = "ref")]
    ref_: String,
    repository: GitlabRepository,
}

impl GitlabHook {
    pub fn url(&self) -> &str {
        &self.repository.git_http_url
    }
}

fn clone(token: &str, hook: &GitlabHook, dir: &Path) -> errors::Result<Repository> {
    let token_for_clone = token.to_owned();
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_, _, _| Cred::userpass_plaintext("grader", &token_for_clone));
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    trace!("cloning {:?} into {:?}", hook.url(), dir);
    let repo = RepoBuilder::new()
        .fetch_options(fetch_options)
        .clone(hook.url(), dir)?;
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

fn package(config: &Configuration, hook: &GitlabHook) -> errors::Result<Vec<(String, String)>> {
    let temp = Temp::new_dir()?;
    let root = temp.to_path_buf();
    let _repo = clone(&config.gitlab.token, hook, &root)?;
    let zip_dir = &Path::new(&config.package.zip_dir);
    let dir = &config.labs.dir;
    let mut to_test = Vec::new();
    for step in &config.labs.steps {
        let path = root.join(&step).join(dir);
        if path.is_dir() {
            trace!("packaging step {} from {:?}", step, path);
            let zip_basename = format!("{}-{}.zip", step, hook.checkout_sha);
            let zip_file = zip_dir.join(&zip_basename);
            match zip_recursive(&path, dir, &zip_file) {
                Ok(_) => to_test.push((step.to_owned(), zip_basename)),
                Err(e) => error!("cannot package {:?} (step {}): {}", hook.url(), step, e),
            }
        }
    }
    info!("to test: {:?}", to_test);
    Ok(to_test)
}

fn labs_result_to_stream(
    base_url: &Url,
    hook: &GitlabHook,
    labs: Vec<(String, String)>,
    ) -> Box<Stream<Item = AMQPRequest<GitlabHook>, Error = ()>> {
    let hook = hook.clone();
    let base_url = base_url.clone();
    Box::new(stream::iter_ok(labs.into_iter().map(move |(step, zip)| {
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
    })))
}

pub fn packager(
    config: &Arc<Configuration>,
    cpu_pool: &CpuPool,
    receive_hook: Receiver<GitlabHook>,
    send_request: Sender<AMQPRequest<GitlabHook>>
    ) -> Box<Future<Item = (), Error = ()>>
{
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
                cpu_pool
                    .spawn_fn(move || package(&config, &clone_hook).map_err(|_| ()))
                    .map(move |labs| labs_result_to_stream(&base_url, &hook, labs))
            })
            .flatten(),
            )
        .map(|_| ())
        )
}

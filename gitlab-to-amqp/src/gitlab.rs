use config::Configuration;
use errors;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use git2::build::{CheckoutBuilder, RepoBuilder};
use graders_utils::ziputils::zip_recursive;
use mktemp::Temp;
use std::path::Path;

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

pub fn package(config: &Configuration, hook: &GitlabHook) -> errors::Result<Vec<(String, String)>> {
    let temp = Temp::new_dir()?;
    let root = temp.to_path_buf();
    let repo = clone(&config.gitlab.token, hook, &root)?;
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

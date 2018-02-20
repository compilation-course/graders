use config::Configuration;
use errors;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use git2::build::{CheckoutBuilder, RepoBuilder};
use graders_utils::ziputils::zip_recursive;
use mktemp::Temp;
use std::path::{Path, PathBuf};

static STEPS: [&str; 2] = ["lab2", "lab3"];

#[derive(Clone, Debug, Deserialize)]
pub struct GitlabRepository {
    name: String,
    git_http_url: String,
}

#[derive(Clone, Debug, Deserialize)]
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

pub fn package(
    config: &Configuration,
    hook: &GitlabHook,
) -> errors::Result<Vec<(&'static str, PathBuf)>> {
    let temp = Temp::new_dir()?;
    let root = temp.to_path_buf();
    let repo = clone(&config.gitlab.token, hook, &root)?;
    let mut to_test = Vec::new();
    let zip_dir = &Path::new(&config.package.zip_dir);
    for &step in &STEPS {
        let path = root.join(&step).join("dragon-tiger");
        if path.is_dir() {
            trace!("packaging step {} from {:?}", step, path);
            let zip_file = zip_dir.join(&format!("{}-{}.zip", step, hook.checkout_sha));
            match zip_recursive(&path, "dragon-tiger", &zip_file) {
                Ok(_) => to_test.push((step, zip_file)),
                Err(e) => error!("cannot package {:?} (step {}): {}", hook.url(), step, e),
            }
        }
    }
    if to_test.is_empty() {
        info!("{:?}: nothing to test", hook.url());
    }
    Ok(to_test)
}

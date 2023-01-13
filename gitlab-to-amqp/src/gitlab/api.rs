use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};
use hyper::Request;
use std::borrow::Borrow;
use std::fmt;
use url::{form_urlencoded, Url};

use super::GitlabHook;
use crate::config::GitlabConfiguration;

fn base_api(config: &GitlabConfiguration) -> Url {
    config.base_url.join("api/v4/").unwrap()
}

fn make_post<I, K, V>(config: &GitlabConfiguration, fragment: &str, params: I) -> Request<String>
where
    I: IntoIterator,
    I::Item: Borrow<(K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let uri = base_api(config).join(fragment).unwrap();
    let params = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params)
        .finish();
    Request::post(uri.to_string())
        .header("Private-Token", config.token.clone())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(CONTENT_LENGTH, params.len())
        .body(params)
        .unwrap()
}

#[derive(Debug, Eq, PartialEq)]
pub enum State {
    Running,
    Success,
    Failed,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match *self {
                State::Running => "running",
                State::Success => "success",
                State::Failed => "failed",
            }
        )
    }
}

pub fn post_status(
    config: &GitlabConfiguration,
    hook: &GitlabHook,
    state: &State,
    ref_: Option<&str>,
    name: &str,
    description: Option<&str>,
) -> Request<String> {
    let state = format!("{state}");
    let mut params: Vec<(&str, &str)> = vec![("state", &state), ("name", name)];
    if let Some(r) = ref_ {
        params.push(("ref", r));
    }
    if let Some(d) = description {
        params.push(("description", d));
    }
    make_post(
        config,
        &format!(
            "projects/{}/statuses/{}",
            hook.project_id,
            hook.pushed_sha()
        ),
        &params,
    )
}

pub fn post_comment(
    config: &GitlabConfiguration,
    hook: &GitlabHook,
    note: &str,
) -> Request<String> {
    make_post(
        config,
        &format!(
            "projects/{}/repository/commits/{}/comments",
            hook.project_id,
            hook.pushed_sha()
        ),
        &vec![("note", note)],
    )
}

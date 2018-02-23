use config::GitlabConfiguration;
use hyper::{Method, Request};
use hyper::header::{ContentLength, ContentType};
use std::borrow::Borrow;
use std::fmt;
use super::GitlabHook;
use url::{form_urlencoded, Url};

fn base_api(config: &GitlabConfiguration) -> Url {
    config.base_url.join("api/v4/").unwrap()
}

header! { (PrivateToken, "Private-Token") => [String] }

fn make_post<I, K, V>(config: &GitlabConfiguration, fragment: &str, params: I) -> Request
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
    let mut req = Request::new(Method::Post, uri.to_string().parse().unwrap());
    req.headers_mut().set(PrivateToken(config.token.to_owned()));
    req.headers_mut().set(ContentType::form_url_encoded());
    req.headers_mut().set(ContentType::form_url_encoded());
    req.headers_mut().set(ContentLength(params.len() as u64));
    req.set_body(params);
    req
}

pub enum State {
    Pending,
    Running,
    Success,
    Failed,
    Canceled,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match *self {
                State::Pending => "pending",
                State::Running => "running",
                State::Success => "success",
                State::Failed => "failed",
                State::Canceled => "canceled",
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
) -> Request {
    let state = format!("{}", state);
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
            hook.project_id, hook.checkout_sha
        ),
        &params,
    )
}

pub fn post_comment(config: &GitlabConfiguration, hook: &GitlabHook, note: &str) -> Request {
    make_post(
        config,
        &format!(
            "projects/{}/repository/commits/{}/comments",
            hook.project_id, hook.checkout_sha
        ),
        &vec![("note", note)],
    )
}

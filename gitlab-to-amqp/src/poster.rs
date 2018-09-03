use futures::Future;
use hyper::{self, Body, Client, Request, StatusCode};
use hyper_tls::HttpsConnector;

pub fn post(
    request: Request<String>,
) -> impl Future<Item = StatusCode, Error = hyper::Error> + 'static {
    let https = HttpsConnector::new(4).unwrap();
    let client = Client::builder().build::<_, Body>(https);
    trace!(
        "preparing to post request to {} ({})",
        request.uri(),
        request.method()
    );
    let request = request.map(|s| Body::from(s));
    let uri = request.uri().clone();
    let method = request.method().clone();
    let post = client
        .request(request)
        .map(move |r| {
            trace!("request to {} ({}) returned {}", uri, method, r.status());
            r.status()
        }).map_err(|e| {
            error!("could not post request: {}", e);
            e
        });
    post
}

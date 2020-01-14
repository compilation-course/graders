use futures::TryFutureExt;
use hyper::{self, Body, Client, Request, StatusCode};
use hyper_tls::HttpsConnector;

pub async fn post(request: Request<String>) -> Result<StatusCode, hyper::Error> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, Body>(https);
    trace!(
        "preparing to post request to {} ({})",
        request.uri(),
        request.method()
    );
    let request = request.map(Body::from);
    let uri = request.uri().clone();
    let method = request.method().clone();
    client
        .request(request)
        .map_ok(move |r| {
            trace!("request to {} ({}) returned {}", uri, method, r.status());
            r.status()
        })
        .inspect_err(|e| {
            error!("could not post request: {}", e);
        })
        .await
}

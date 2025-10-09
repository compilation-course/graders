use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Request, StatusCode};
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

pub async fn post(
    request: Request<String>,
) -> Result<StatusCode, Box<dyn std::error::Error + Send + Sync>> {
    let https = HttpsConnector::new();
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(https);
    log::trace!(
        "preparing to post request to {} ({})",
        request.uri(),
        request.method()
    );
    let request = request.map(|body| Full::new(Bytes::from(body)));
    let uri = request.uri().clone();
    let method = request.method().clone();
    let result = client
        .request(request)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    log::trace!(
        "request to {} ({}) returned {}",
        uri,
        method,
        result.status()
    );
    Ok(result.status())
}

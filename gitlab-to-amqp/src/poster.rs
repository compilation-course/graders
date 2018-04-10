use futures::Future;
use futures_cpupool::CpuPool;
use hyper::{Client, Request};
use hyper_tls::HttpsConnector;
use tokio_core::reactor::Core;

pub fn post(cpu_pool: &CpuPool, request: Request) {
    // This convoluted way will be removed when hyper adopts Tokio reform and
    // futures 0.2.
    cpu_pool
        .spawn_fn(move || {
            let mut core = Core::new().unwrap();
            let client = Client::configure()
                .connector(HttpsConnector::new(4, &core.handle()).unwrap())
                .build(&core.handle());
            trace!(
                "posting request to {} ({})",
                request.uri(),
                request.method()
            );
            let post = client.request(request).map_err(|e| {
                error!("could not post request: {}", e);
                e
            });
            core.run(post)
        })
        .forget();
}

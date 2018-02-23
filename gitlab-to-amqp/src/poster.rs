use futures_cpupool::CpuPool;
use futures::Future;
use hyper::client::Client;
use hyper::Request;
use tokio_core::reactor::Core;

pub fn post(cpu_pool: &CpuPool, request: Request) {
    // This convoluted way will be removed when hyper adopts Tokio reform and
    // futures 0.2.
    cpu_pool.spawn_fn(move || {
        let mut core = Core::new().unwrap();
        let client = Client::new(&core.handle());
        trace!("posting request to {} ({})", request.uri(), request.method());
        let post = client.request(request).map_err(|e| {
                                        error!("could not post request: {}", e);
                                        e
        });
        core.run(post)
    }).forget();
}

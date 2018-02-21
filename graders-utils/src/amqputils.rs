use futures::future::{self, Future};
use lapin;
use lapin::channel::Channel;
use lapin::client::{Client, ConnectionOptions};
use std::fmt::Debug;
use std::io;
use std::net;
use std::result::Result;
use tokio::executor::current_thread;
use tokio::net::TcpStream;
use tokio::reactor::Handle;

#[derive(Deserialize)]
pub struct AMQPConfiguration {
    pub host: String,
    pub port: u16,
    pub exchange: String,
    pub routing_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AMQPRequest<O: Debug> {
    pub step: String,
    pub zip_url: String,
    pub result_queue: String,
    pub opaque: O,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AMQPResponse<O> {
    pub step: String,
    pub opaque: O,
    pub yaml_result: String,
}

pub fn create_client(
    handle: &Handle,
    config: &AMQPConfiguration,
) -> Box<Future<Item = Client<TcpStream>, Error = io::Error>> {
    match net::TcpStream::connect(&format!("{}:{}", config.host, config.port)) {
        Ok(s) => Box::new(
            future::result(TcpStream::from_std(s, handle))
                .and_then(|stream| {
                    lapin::client::Client::connect(stream, &ConnectionOptions::default())
                })
                .map(|(client, heartbeat_future_fn)| {
                    let heartbeat_client = client.clone();
                    current_thread::spawn(heartbeat_future_fn(&heartbeat_client).map_err(|_| ()));
                    client
                }),
        ),
        Err(e) => Box::new(future::err(e)),
    }
}

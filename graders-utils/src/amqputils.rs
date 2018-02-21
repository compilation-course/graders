use futures::future::{self, Future};
use lapin;
use lapin::channel::{Channel, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::client::{Client, ConnectionOptions};
use lapin::types::FieldTable;
use std::io;
use std::net;
use tokio::executor::current_thread;
use tokio::net::TcpStream;
use tokio::reactor::Handle;

#[derive(Clone, Deserialize)]
pub struct AMQPConfiguration {
    pub host: String,
    pub port: u16,
    pub exchange: String,
    pub routing_key: String,
    pub queue: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AMQPRequest {
    pub step: String,
    pub zip_url: String,
    pub result_queue: String,
    pub opaque: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AMQPResponse {
    pub step: String,
    pub opaque: String,
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

pub fn declare_exchange_and_queue(
    channel: &Channel<TcpStream>,
    config: &AMQPConfiguration,
) -> Box<Future<Item = (), Error = ()>> {
    let channel = channel.clone();
    let channel1 = channel.clone();
    let config = config.clone();
    let config1 = config.clone();
    Box::new(
        declare_exchange(&channel, &config)
            .and_then(move |_| declare_queue(&channel, &config))
            .and_then(move |_| bind_queue(&channel1, &config1)),
    )
}

fn declare_exchange(
    channel: &Channel<TcpStream>,
    config: &AMQPConfiguration,
) -> Box<Future<Item = (), Error = ()>> {
    let exchange = config.exchange.clone();
    Box::new(
        channel
            .exchange_declare(
                &exchange,
                "direct",
                &ExchangeDeclareOptions::default(),
                &FieldTable::new(),
            )
            .map_err(move |e| {
                error!("cannot declare exchange {}: {}", exchange, e);
                ()
            }),
    )
}

fn declare_queue(
    channel: &Channel<TcpStream>,
    config: &AMQPConfiguration,
) -> Box<Future<Item = (), Error = ()>> {
    let queue = config.queue.clone();
    Box::new(
        channel
            .queue_declare(&queue, &QueueDeclareOptions::default(), &FieldTable::new())
            .map_err(move |e| {
                error!("could not declare queue {}: {}", queue, e);
                ()
            }),
    )
}

fn bind_queue(
    channel: &Channel<TcpStream>,
    config: &AMQPConfiguration,
) -> Box<Future<Item = (), Error = ()>> {
    let queue = config.queue.clone();
    let exchange = config.exchange.clone();
    let routing_key = config.routing_key.clone();
    Box::new(
        channel
            .queue_bind(
                &queue,
                &exchange,
                &routing_key,
                &QueueBindOptions::default(),
                &FieldTable::new(),
            )
            .map_err(move |e| {
                error!(
                    "could not bind queue {} to exchange {} using routing key {}: {}",
                    queue, exchange, routing_key, e
                );
                ()
            }),
    )
}

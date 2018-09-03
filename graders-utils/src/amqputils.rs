use futures::future::{self, Future};
use lapin::channel::{Channel, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::client::{Client, ConnectionOptions};
use lapin::queue::Queue;
use lapin::types::FieldTable;
use std::io;
use std::net;
use tokio;
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
    pub job_name: String,
    pub lab: String,
    pub dir: String,
    pub zip_url: String,
    pub result_queue: String,
    pub opaque: String,
    /// The delivery tag will be set upon message reception
    pub delivery_tag: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AMQPResponse {
    pub job_name: String,
    pub lab: String,
    pub opaque: String,
    pub yaml_result: String,
    /// The delivery tag and result queue will be removed before message emission
    pub result_queue: String,
    pub delivery_tag: u64,
}

/// Return a client that will connect to a remote AMQP server.
pub fn create_client(
    config: &AMQPConfiguration,
) -> impl Future<Item = Client<TcpStream>, Error = io::Error> + Send + 'static {
    let dest = format!("{}:{}", config.host, config.port);
    future::result(net::TcpStream::connect(&dest))
        .and_then(|stream| future::result(TcpStream::from_std(stream, &Handle::default())))
        .and_then(|stream| Client::connect(stream, ConnectionOptions::default()))
        .map(|(client, heartbeat)| {
            tokio::spawn(heartbeat.map_err(|e| {
                warn!("cannot send AMQP heartbeat: {}", e);
                ()
            }));
            client
        }).map_err(move |e| {
            warn!("error when connecting AMQP client to {}: {}", dest, e);
            e
        })
}

pub fn declare_exchange_and_queue(
    channel: &Channel<TcpStream>,
    config: &AMQPConfiguration,
) -> impl Future<Item = Queue, Error = io::Error> + Send + 'static {
    let channel = channel.clone();
    let config = config.clone();
    declare_exchange(&channel, &config)
        .and_then(move |_| {
            declare_queue(&channel, &config).map(move |queue| (channel, config, queue))
        }).and_then(move |(channel, config, queue)| bind_queue(&channel, &config).map(|()| queue))
}

fn declare_exchange(
    channel: &Channel<TcpStream>,
    config: &AMQPConfiguration,
) -> impl Future<Item = (), Error = io::Error> + Send + 'static {
    let exchange = config.exchange.clone();
    channel
        .exchange_declare(
            &exchange,
            "direct",
            ExchangeDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::new(),
        ).map_err(move |e| {
            error!("cannot declare exchange {}: {}", exchange, e);
            e
        })
}

fn declare_queue(
    channel: &Channel<TcpStream>,
    config: &AMQPConfiguration,
) -> impl Future<Item = Queue, Error = io::Error> + Send + 'static {
    let queue = config.queue.clone();
    channel
        .queue_declare(
            &queue,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::new(),
        ).map_err(move |e| {
            error!("could not declare queue {}: {}", queue, e);
            e
        })
}

fn bind_queue(
    channel: &Channel<TcpStream>,
    config: &AMQPConfiguration,
) -> impl Future<Item = (), Error = io::Error> + Send + 'static {
    let queue = config.queue.clone();
    let exchange = config.exchange.clone();
    let routing_key = config.routing_key.clone();
    channel
        .queue_bind(
            &queue,
            &exchange,
            &routing_key,
            QueueBindOptions::default(),
            FieldTable::new(),
        ).map_err(move |e| {
            error!(
                "could not bind queue {} to exchange {} using routing key {}: {}",
                queue, exchange, routing_key, e
            );
            e
        })
}

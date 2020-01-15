use futures::future::TryFutureExt;
use lapin::options::{ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use lapin::{Channel, Connection, ConnectionProperties, ExchangeKind, Queue};

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
pub async fn create_connection(config: &AMQPConfiguration) -> Result<Connection, lapin::Error> {
    // TODO Check how to add heartbeat
    let dest = format!("amqp://{}:{}/%2f", config.host, config.port);
    Connection::connect(&dest, ConnectionProperties::default())
        .inspect_err(|e| {
            warn!("error when connecting AMQP client to {}: {}", dest, e);
        })
        .await
}

pub async fn declare_exchange_and_queue(
    channel: &Channel,
    config: &AMQPConfiguration,
) -> Result<Queue, lapin::Error> {
    declare_exchange(channel, config).await?;
    let queue = declare_queue(channel, config).await?;
    bind_queue(channel, config).await?;
    Ok(queue)
}

async fn declare_exchange(
    channel: &Channel,
    config: &AMQPConfiguration,
) -> Result<(), lapin::Error> {
    let exchange = config.exchange.clone();
    channel
        .exchange_declare(
            &exchange,
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .inspect_err(|e| {
            error!("cannot declare exchange {}: {}", exchange, e);
        })
        .await
}

async fn declare_queue(
    channel: &Channel,
    config: &AMQPConfiguration,
) -> Result<Queue, lapin::Error> {
    let queue = config.queue.clone();
    channel
        .queue_declare(
            &queue,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .inspect_err(|e| {
            error!("could not declare queue {}: {}", queue, e);
        })
        .await
}

async fn bind_queue(channel: &Channel, config: &AMQPConfiguration) -> Result<(), lapin::Error> {
    let queue = config.queue.clone();
    let exchange = config.exchange.clone();
    let routing_key = config.routing_key.clone();
    channel
        .queue_bind(
            &queue,
            &exchange,
            &routing_key,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .inspect_err(move |e| {
            error!(
                "could not bind queue {} to exchange {} using routing key {}: {}",
                queue, exchange, routing_key, e
            );
        })
        .await
}

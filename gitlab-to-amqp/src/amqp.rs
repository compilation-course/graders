use config::Configuration;
use futures::future::{self, Future};
use futures::Stream;
use futures::sync::mpsc::Receiver;
use graders_utils::amqputils::{self, AMQPRequest};
use lapin;
use lapin::channel::{BasicProperties, BasicPublishOptions};
use serde_json;
use std::sync::Arc;
use tokio;
use tokio::reactor::Handle;

fn amqp_publisher(
    channel: &lapin::channel::Channel<tokio::net::TcpStream>,
    config: &Arc<Configuration>,
    receive_request: Receiver<AMQPRequest>,
) -> Box<Future<Item = (), Error = ()>> {
    let channel = channel.clone();
    let config = config.clone();
    Box::new(
        receive_request
            .map(move |req| {
                trace!("handling incoming AMQP publishing request: {:?}", req);
                channel.basic_publish(
                    &config.amqp.exchange,
                    &config.amqp.routing_key,
                    serde_json::to_string(&req).unwrap().as_bytes(),
                    &BasicPublishOptions::default(),
                    BasicProperties::default(),
                )
            })
            .for_each(|_| future::ok(())),
    )
}

pub fn amqp_process(
    config: &Arc<Configuration>,
    receive_request: Receiver<AMQPRequest>,
) -> Box<Future<Item = (), Error = ()>> {
    let client = amqputils::create_client(&Handle::default(), &config.amqp);
    let config = config.clone();
    Box::new(
        client
            .and_then(|client| client.create_channel())
            .map_err(|_| ())
            .and_then(|channel| {
                amqputils::declare_exchange_and_queue(&channel, &config.amqp)
                    .and_then(move |_| amqp_publisher(&channel, &config, receive_request))
            }),
    )
}

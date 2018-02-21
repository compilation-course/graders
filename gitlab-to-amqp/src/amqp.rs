use config::Configuration;
use futures::future::{self, Future};
use futures::Stream;
use futures::sync::mpsc::Receiver;
use gitlab::GitlabHook;
use graders_utils::amqputils::{self, AMQPRequest};
use lapin;
use lapin::channel::*;
use lapin::types::FieldTable;
use serde_json;
use std::sync::Arc;
use tokio;
use tokio::reactor::Handle;

fn amqp_declare_exchange(
    channel: &lapin::channel::Channel<tokio::net::TcpStream>,
    config: &Arc<Configuration>,
) -> Box<Future<Item = (), Error = ()>> {
    Box::new(
        channel
            .exchange_declare(
                &config.amqp.exchange,
                "direct",
                &ExchangeDeclareOptions::default(),
                &FieldTable::new(),
            )
            .map_err(|_| ()),
    )
}

fn amqp_publisher(
    channel: &lapin::channel::Channel<tokio::net::TcpStream>,
    config: &Arc<Configuration>,
    receive_request: Receiver<AMQPRequest<GitlabHook>>,
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
    receive_request: Receiver<AMQPRequest<GitlabHook>>
) -> 
    Box<Future<Item = (), Error = ()>>
 {
    let client = amqputils::create_client(&Handle::default(), &config.amqp);
    let config = config.clone();
    Box::new(client
        .and_then(|client| client.create_channel())
        .map_err(|_| ())
        .and_then(|channel| {
            amqp_declare_exchange(&channel, &config)
                .and_then(move |_| amqp_publisher(&channel, &config, receive_request))
        }))
}

use config::Configuration;
use futures::future::{self, Future};
use futures::{Sink, Stream};
use futures::sync::mpsc::{Receiver, Sender};
use gitlab;
use graders_utils::amqputils::{self, AMQPRequest, AMQPResponse};
use lapin::channel::{BasicConsumeOptions, BasicProperties, BasicPublishOptions, Channel,
                     QueueDeclareOptions};
use lapin::types::FieldTable;
use serde_json;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::reactor::Handle;

fn amqp_publisher(
    channel: &Channel<TcpStream>,
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

fn amqp_receiver(
    channel: &Channel<TcpStream>,
    send_response: Sender<AMQPResponse>,
) -> Box<Future<Item = (), Error = ()>> {
    let channel = channel.clone();
    Box::new(
        channel
            .queue_declare(
                &gitlab::RESULT_QUEUE,
                &QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                &FieldTable::new(),
            )
            .map_err(|e| {
                error!(
                    "could not declare AMQP queue {}: {}",
                    gitlab::RESULT_QUEUE,
                    e
                );
                ()
            })
            .and_then(move |_| {
                channel
                    .basic_consume(
                        &gitlab::RESULT_QUEUE,
                        "gitlab-to-amqp",
                        &BasicConsumeOptions::default(),
                        &FieldTable::new(),
                    )
                    .map_err(|e| {
                        error!(
                            "could not declare consume queue {}: {}",
                            gitlab::RESULT_QUEUE,
                            e
                        );
                        ()
                    })
                    .and_then(move |stream| {
                        let data = stream
                            .and_then(move |msg| {
                                channel
                                    .basic_ack(msg.delivery_tag)
                                    .map(|_| msg)
                                    .map_err(|e| {
                                        error!("cannot send ack: {}", e);
                                        e
                                    })
                            })
                            .filter_map(|msg| match String::from_utf8(msg.data) {
                                Ok(s) => Some(s),
                                Err(e) => {
                                    error!("cannot decode message: {}", e);
                                    None
                                }
                            })
                            .filter_map(|s| match serde_json::from_str(&s) {
                                Ok(response) => Some(response),
                                Err(e) => {
                                    error!("unable to decode {} as AMQPResponse: {}", s, e);
                                    None
                                }
                            })
                            .map_err(|_| ());
                        send_response
                            .sink_map_err(|_| ())
                            .send_all(data)
                            .map(|_| ())
                    })
            }),
    )
}

pub fn amqp_process(
    config: &Arc<Configuration>,
    receive_request: Receiver<AMQPRequest>,
    send_response: Sender<AMQPResponse>,
) -> Box<Future<Item = (), Error = ()>> {
    let client = amqputils::create_client(&Handle::default(), &config.amqp);
    let config = config.clone();
    Box::new(
        client
            .and_then(|client| client.create_channel())
            .map_err(|_| ())
            .and_then(|channel| {
                amqputils::declare_exchange_and_queue(&channel, &config.amqp).and_then(move |_| {
                    amqp_publisher(&channel, &config, receive_request)
                        .join(amqp_receiver(&channel, send_response))
                        .map(|_| ())
                })
            }),
    )
}

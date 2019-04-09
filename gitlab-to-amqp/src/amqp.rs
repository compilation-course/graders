use failure::format_err;
use futures::future::{self, Future};
use futures::sync::mpsc::{Receiver, Sender};
use futures::{Sink, Stream};
use graders_utils::amqputils::{self, AMQPRequest, AMQPResponse};
use lapin::channel::{
    BasicConsumeOptions, BasicProperties, BasicPublishOptions, Channel, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin_futures as lapin;
use serde_json;
use std::sync::Arc;
use tokio::net::TcpStream;

use crate::config::Configuration;
use crate::gitlab;

fn amqp_publisher(
    channel: &Channel<TcpStream>,
    config: &Arc<Configuration>,
    receive_request: Receiver<AMQPRequest>,
) -> impl Future<Item = (), Error = lapin::error::Error> + Send + 'static {
    let channel = channel.clone();
    let config = config.clone();
    receive_request
        .then(move |req| {
            let req = req.unwrap();
            info!("publishing AMQP job request {}", req.job_name);
            channel.basic_publish(
                &config.amqp.exchange,
                &config.amqp.routing_key,
                serde_json::to_string(&req).unwrap().as_bytes().to_vec(),
                BasicPublishOptions::default(),
                BasicProperties::default(),
            )
        })
        .for_each(|_| future::ok(()))
}

fn amqp_receiver(
    channel: &Channel<TcpStream>,
    send_response: Sender<AMQPResponse>,
) -> impl Future<Item = (), Error = failure::Error> + Send + 'static {
    let channel = channel.clone();
    let channel_clone = channel.clone();
    channel
        .queue_declare(
            gitlab::RESULT_QUEUE,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::new(),
        )
        .and_then(move |result_queue| {
            channel.basic_consume(
                &result_queue,
                "gitlab-to-amqp",
                BasicConsumeOptions::default(),
                FieldTable::new(),
            )
        })
        .map_err(|e| format_err!("{}", e))
        .and_then(move |stream| {
            info!("listening onto the {} queue", gitlab::RESULT_QUEUE);
            let data = stream
                .and_then(move |msg| {
                    channel_clone
                        .basic_ack(msg.delivery_tag, false)
                        .map(|_| msg)
                })
                .filter_map(|msg| match String::from_utf8(msg.data) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        error!("unable to decode message as valid utf8 string: {}", e);
                        None
                    }
                })
                .filter_map(|s| match serde_json::from_str::<AMQPResponse>(&s) {
                    Ok(response) => {
                        trace!("received response for {}", response.job_name);
                        Some(response)
                    }
                    Err(e) => {
                        error!("unable to decode message {} as AMQPResponse: {}", s, e);
                        None
                    }
                });
            send_response
                .sink_map_err(|e| {
                    warn!("sink error: {}", e);
                    format_err!("sink error: {}", e)
                })
                .send_all(data)
                .map(|_| {
                    warn!(
                        "terminating listening onto the {} queue",
                        gitlab::RESULT_QUEUE
                    );
                    ()
                })
        })
}

pub fn amqp_process(
    config: &Arc<Configuration>,
    receive_request: Receiver<AMQPRequest>,
    send_response: Sender<AMQPResponse>,
) -> impl Future<Item = (), Error = failure::Error> + Send + 'static {
    let client = amqputils::create_client(&config.amqp);
    let config = config.clone();
    client.and_then(move |client| {
        let publisher = client
            .create_channel()
            .and_then(move |channel| {
                amqputils::declare_exchange_and_queue(&channel, &config.amqp)
                    .map(|_| (channel, config))
            })
            .and_then(move |(channel, config)| amqp_publisher(&channel, &config, receive_request));
        let receiver = client
            .create_channel()
            .map_err(|e| e.into())
            .and_then(|channel| amqp_receiver(&channel, send_response));
        publisher.map_err(|e| e.into()).join(receiver).map(|_| ())
    })
}

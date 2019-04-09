use futures::future::Future;
use futures::sync::mpsc::{Receiver, Sender};
use futures::{Sink, Stream};
use graders_utils::amqputils::{self, AMQPRequest, AMQPResponse};
use lapin::channel::{
    BasicConsumeOptions, BasicProperties, BasicPublishOptions, BasicQosOptions, Channel,
};
use lapin::queue::Queue;
use lapin::types::FieldTable;
use lapin_futures as lapin;
use serde_json;
use std::mem;
use std::sync::Arc;
use tokio::net::TcpStream;

use crate::config::Configuration;

fn amqp_receiver(
    channel: &Channel<TcpStream>,
    config: &Arc<Configuration>,
    queue: Queue,
    send_request: Sender<AMQPRequest>,
) -> impl Future<Item = (), Error = failure::Error> + Send + 'static {
    let channel = channel.clone();
    let prefetch_count = config.tester.parallelism as u16;
    channel
        .basic_qos(BasicQosOptions {
            prefetch_count,
            global: false,
            ..Default::default()
        })
        .and_then(move |_| {
            channel.basic_consume(
                &queue,
                "amqp-to-test",
                BasicConsumeOptions::default(),
                FieldTable::new(),
            )
        })
        .map_err(|e| e.into())
        .and_then(move |stream| {
            let data = stream
                .filter_map(|msg| match String::from_utf8(msg.data) {
                    Ok(s) => Some((s, msg.delivery_tag)),
                    Err(e) => {
                        error!("cannot decode message: {}", e);
                        None
                    }
                })
                .filter_map(move |(s, tag)| match serde_json::from_str(&s) {
                    Ok(request) => Some(AMQPRequest {
                        delivery_tag: Some(tag),
                        ..request
                    }),
                    Err(e) => {
                        error!("unable to decode {} as AMQPRequest: {}", s, e);
                        None
                    }
                });
            send_request
                .sink_map_err(|e| format_err!("error when receiving: {}", e))
                .send_all(data)
                .map(|_| ())
        })
}

// Acks must be sent on the original channel. Sending concurrently
// is supposed to be compatible with basic_consume.
fn amqp_sender(
    channel: &Channel<TcpStream>,
    ack_channel: &Channel<TcpStream>,
    receive_response: Receiver<AMQPResponse>,
) -> impl Future<Item = (), Error = failure::Error> + Send + 'static {
    let channel = channel.clone();
    let ack_channel = ack_channel.clone();
    receive_response
        .map_err(|_| format_err!("")) // Dummy
        .for_each(move |mut response| {
            info!(
                "sending response {} to queue {}",
                response.job_name, response.result_queue
            );
            let queue = mem::replace(&mut response.result_queue, "".to_owned());
            let delivery_tag = mem::replace(&mut response.delivery_tag, 0);
            let channel = channel.clone();
            let ack_channel = ack_channel.clone();
            channel
                .basic_publish(
                    "",
                    &queue,
                    serde_json::to_string(&response)
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                    BasicPublishOptions::default(),
                    BasicProperties::default(),
                )
                .and_then(move |_| ack_channel.basic_ack(delivery_tag, false))
                .map_err(|e| e.into())
        })
}

pub fn amqp_process(
    config: &Arc<Configuration>,
    send_request: Sender<AMQPRequest>,
    receive_response: Receiver<AMQPResponse>,
) -> impl Future<Item = (), Error = failure::Error> + Send + 'static {
    let client = amqputils::create_client(&config.amqp);
    let config = config.clone();
    client.and_then(move |client| {
        client
            .create_channel()
            .map_err(|e| e.into())
            .and_then(move |channel| {
                let ack_channel = channel.clone();
                let receiver = amqputils::declare_exchange_and_queue(&channel, &config.amqp)
                    .map_err(|e| e.into())
                    .and_then(move |queue| amqp_receiver(&channel, &config, queue, send_request));
                let sender = client
                    .create_channel()
                    .map_err(|e| e.into())
                    .and_then(move |channel| amqp_sender(&channel, &ack_channel, receive_response));
                receiver.join(sender).map(|_| ())
            })
    })
}

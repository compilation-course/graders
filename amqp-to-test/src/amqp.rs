use config::Configuration;
use futures::future::Future;
use futures::{Sink, Stream};
use futures::sync::mpsc::{Receiver, Sender};
use graders_utils::amqputils::{self, AMQPRequest, AMQPResponse};
use lapin::channel::{BasicConsumeOptions, BasicProperties, BasicPublishOptions, Channel};
use lapin::types::FieldTable;
use serde_json;
use std::io;
use std::mem;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::reactor::Handle;

fn amqp_receiver(
    channel: &Channel<TcpStream>,
    config: &Arc<Configuration>,
    send_request: Sender<AMQPRequest>,
) -> Box<Future<Item = (), Error = io::Error>> {
    Box::new(
        channel
            .basic_consume(
                &config.amqp.queue,
                "amqp-to-test",
                &BasicConsumeOptions::default(),
                &FieldTable::new(),
            )
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
                    .sink_map_err(|e| {
                        io::Error::new(io::ErrorKind::Other, format!("error when receiving: {}", e))
                    })
                    .send_all(data)
                    .map(|_| ())
            }),
    )
}

fn amqp_sender(
    channel: &Channel<TcpStream>,
    receive_response: Receiver<AMQPResponse>,
) -> Box<Future<Item = (), Error = io::Error>> {
    let channel = channel.clone();
    Box::new(
        receive_response
            .map_err(|_| io::Error::new(io::ErrorKind::Other, ""))  // Dummy
            .for_each(move |mut response| {
                trace!(
                    "sending response for step {} to queue {}",
                    response.step,
                    response.result_queue
                );
                let queue = mem::replace(&mut response.result_queue, "".to_owned());
                let delivery_tag = mem::replace(&mut response.delivery_tag, 0);
                let channel_clone = channel.clone();
                channel
                    .basic_publish(
                        "",
                        &queue,
                        serde_json::to_string(&response).unwrap().as_bytes(),
                        &BasicPublishOptions::default(),
                        BasicProperties::default(),
                    )
                    .and_then(move |_| channel_clone.basic_ack(delivery_tag))
            }),
    )
}

pub fn amqp_process(
    config: &Arc<Configuration>,
    send_request: Sender<AMQPRequest>,
    receive_response: Receiver<AMQPResponse>,
) -> Box<Future<Item = (), Error = io::Error>> {
    let client = amqputils::create_client(&Handle::default(), &config.amqp);
    let config = config.clone();
    Box::new(
        client
            .and_then(|client| client.create_channel())
            .and_then(|channel| {
                amqputils::declare_exchange_and_queue(&channel, &config.amqp).and_then(move |_| {
                    amqp_receiver(&channel, &config, send_request)
                        .join(amqp_sender(&channel, receive_response))
                        .map(|_| ())
                })
            }),
    )
}

use config::Configuration;
use futures::future::Future;
use futures::{Sink, Stream};
use futures::sync::mpsc::Sender;
use graders_utils::amqputils::{self, AMQPRequest, AMQPResponse};
use lapin::channel::{BasicConsumeOptions, Channel};
use lapin::types::FieldTable;
use serde_json;
use std::sync::Arc;
use tokio;
use tokio::reactor::Handle;

fn amqp_receiver(
    channel: &Channel<tokio::net::TcpStream>,
    config: &Arc<Configuration>,
    send_request: Sender<AMQPRequest>,
) -> Box<Future<Item = (), Error = ()>> {
    Box::new(
        channel
            .basic_consume(
                &config.amqp.queue,
                "amqp-to-test",
                &BasicConsumeOptions::default(),
                &FieldTable::new(),
            )
            .map_err(|e| {
                error!("cannot read AMQP queue: {}", e);
                ()
            })
            .and_then(|stream| {
                let data = stream
                    .filter_map(|msg| match String::from_utf8(msg.data) {
                        Ok(s) => Some(s),
                        Err(e) => {
                            error!("cannot decode message: {}", e);
                            None
                        }
                    })
                    .filter_map(|s| match serde_json::from_str(&s) {
                        Ok(request) => Some(request),
                        Err(e) => {
                            error!("unable to decode {} as AMQPRequest: {}", s, e);
                            None
                        }
                    })
                    .map_err(|_| ());
                send_request.sink_map_err(|_| ()).send_all(data).map(|_| ())
            }),
    )
}

pub fn amqp_process(
    config: &Arc<Configuration>,
    send_request: Sender<AMQPRequest>,
) -> Box<Future<Item = (), Error = ()>> {
    let client = amqputils::create_client(&Handle::default(), &config.amqp);
    let config = config.clone();
    Box::new(
        client
            .and_then(|client| client.create_channel())
            .map_err(|_| ())
            .and_then(|channel| {
                amqputils::declare_exchange_and_queue(&channel, &config.amqp)
                    .and_then(move |_| amqp_receiver(&channel, &config, send_request))
            }),
    )
}

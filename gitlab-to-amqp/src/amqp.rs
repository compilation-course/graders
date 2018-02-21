use config::Configuration;
use futures::future::{self, Future};
use futures::Stream;
use futures::sync::mpsc::{self, Receiver, Sender};
use gitlab::GitlabHook;
use graders_utils::amqputils::{self, AMQPRequest};
use lapin;
use lapin::channel::*;
use lapin::types::FieldTable;
use serde_json;
use std::sync::Arc;
use std::thread;
use tokio;
use tokio::executor::current_thread;
use tokio::reactor::Handle;

pub fn start_amqp_thread(config: &Arc<Configuration>) -> Sender<AMQPRequest<GitlabHook>> {
    let (send_request, receive_request) = mpsc::channel(16);
    let config = config.clone();
    thread::spawn(move || run_amqp(config, receive_request));
    send_request
}

fn amqp_declare_exchange(
    channel: &lapin::channel::Channel<tokio::net::TcpStream>,
    config: &Arc<Configuration>,
) -> Box<Future<Item = (), Error = ()>> {
    Box::new(channel.exchange_declare(
        &config.amqp.exchange,
        "direct",
        &ExchangeDeclareOptions::default(),
        &FieldTable::new(),
    ).map_err(|_| ()))
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
        .for_each(|_| future::ok(()))
    )
}

fn amqp_process(
    config: Arc<Configuration>,
    receive_request: Receiver<AMQPRequest<GitlabHook>>,
) -> Box<Future<Item = (), Error = ()>> {
    let client = amqputils::create_client(&Handle::default(), &config.amqp);
    let publisher = client
        .and_then(|client| client.create_channel())
        .map_err(|_| ())
        .and_then(|channel| {
            amqp_declare_exchange(&channel, &config)
                .and_then(move |_| amqp_publisher(&channel, &config, receive_request))
        });
    Box::new(publisher)
}

fn run_amqp(config: Arc<Configuration>, receive_request: Receiver<AMQPRequest<GitlabHook>>) {
    current_thread::run(move |_| current_thread::spawn(amqp_process(config, receive_request)))
}

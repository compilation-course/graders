use failure::ResultExt;
use futures::channel::mpsc::{Receiver, Sender};
use futures::compat::{Future01CompatExt, Stream01CompatExt};
use futures::future;
use futures::sink::SinkExt;
use futures::stream::{StreamExt, TryStreamExt};
use graders_utils::amqputils::{self, AMQPRequest, AMQPResponse};
use lapin::options::{BasicConsumeOptions, BasicPublishOptions, BasicQosOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, Queue};
use lapin_futures as lapin;
use serde_json;
use std::mem;
use std::sync::Arc;

use crate::config::Configuration;

async fn amqp_receiver(
    channel: &Channel,
    config: &Arc<Configuration>,
    queue: Queue,
    send_request: Sender<AMQPRequest>,
) -> Result<(), failure::Error> {
    let prefetch_count = config.tester.parallelism as u16;
    channel
        .basic_qos(prefetch_count, BasicQosOptions { global: false })
        .compat()
        .await?;
    let stream = Box::new(
        channel
            .basic_consume(
                &queue,
                "amqp-to-test",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .compat()
            .await?,
    );
    let mut data = stream
        .compat()
        .map(|msg| {
            let msg = msg.with_context(|e| format!("incoming message error: {}", e))?;
            let s =
                String::from_utf8(msg.data).with_context(|e| format!("invalid UTF-8: {}", e))?;
            let request =
                serde_json::from_str(&s).with_context(|e| format!("cannot decode json: {}", e))?;
            Ok(AMQPRequest {
                delivery_tag: Some(msg.delivery_tag),
                ..request
            })
        })
        .filter(|e| future::ready(e.is_ok()));
    send_request
        .sink_map_err(|e| format_err!("error when receiving: {}", e))
        .send_all(&mut data)
        .await?;
    Ok(())
}

// Acks must be sent on the original channel. Sending concurrently
// is supposed to be compatible with basic_consume.
async fn amqp_sender(
    channel: &Channel,
    ack_channel: &Channel,
    receive_response: Receiver<AMQPResponse>,
) -> Result<(), failure::Error> {
    let channel = channel.clone();
    let ack_channel = ack_channel.clone();
    receive_response
        .map(move |e| Ok((e, channel.clone(), ack_channel.clone())))
        .try_for_each(move |(mut response, channel, ack_channel)| async move {
            info!(
                "sending response {} to queue {}",
                response.job_name, response.result_queue
            );
            let queue = mem::replace(&mut response.result_queue, "".to_owned());
            let delivery_tag = mem::replace(&mut response.delivery_tag, 0);
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
                .compat()
                .await?;
            ack_channel.basic_ack(delivery_tag, false).compat().await?;
            Ok(()) as Result<(), failure::Error>
        })
        .await?;
    Ok(())
}

pub async fn amqp_process(
    config: &Arc<Configuration>,
    send_request: Sender<AMQPRequest>,
    receive_response: Receiver<AMQPResponse>,
) -> Result<(), failure::Error> {
    let client = amqputils::create_client(&config.amqp).await?;
    let receiver_channel = client.create_channel().compat().await?;
    let queue = amqputils::declare_exchange_and_queue(&receiver_channel, &config.amqp).await?;
    let receiver = amqp_receiver(&receiver_channel, &config, queue, send_request);
    let sender_channel = client.create_channel().compat().await?;
    let sender = amqp_sender(&sender_channel, &receiver_channel, receive_response);
    futures::try_join!(receiver, sender)?;
    Ok(())
}

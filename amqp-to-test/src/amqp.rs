use amqp_utils::{self, AMQPChannel, AMQPConnection, AMQPError, AMQPRequest, AMQPResponse};
use futures::channel::mpsc::{Receiver, Sender};
use futures::future;
use futures::sink::SinkExt;
use futures::stream::{StreamExt, TryStreamExt};
use std::mem;
use std::sync::Arc;

use crate::config::Configuration;

async fn amqp_receiver(
    channel: &AMQPChannel,
    config: &Arc<Configuration>,
    send_request: Sender<AMQPRequest>,
) -> Result<(), AMQPError> {
    let prefetch_count = config.tester.parallelism as u16;
    channel.basic_qos(prefetch_count).await?;
    let stream = channel
        .basic_consume(&config.amqp.queue, "amqp-to-test")
        .await?;
    let mut data = stream
        .map(|msg| {
            let msg = msg?;
            let request = msg.decode_payload()?;
            Ok(AMQPRequest {
                delivery_tag: Some(msg.delivery_tag()),
                ..request
            })
        })
        .filter(|e| future::ready(e.is_ok()));
    send_request
        .sink_map_err(|e| {
            warn!("sink error: {}", e);
            AMQPError::from(e)
        })
        .send_all(&mut data)
        .await?;
    Ok(())
}

// Acks must be sent on the original channel. Sending concurrently
// is supposed to be compatible with basic_consume.
async fn amqp_sender(
    channel: &AMQPChannel,
    ack_channel: &AMQPChannel,
    receive_response: Receiver<AMQPResponse>,
    reports_routing_key: Option<String>,
) -> Result<(), AMQPError> {
    let channel = channel.clone();
    let ack_channel = ack_channel.clone();
    let reports_routing_key = reports_routing_key.clone();
    receive_response
        .map(move |e| {
            Ok((
                e,
                channel.clone(),
                ack_channel.clone(),
                reports_routing_key.clone(),
            ))
        })
        .try_for_each(
            move |(mut response, channel, ack_channel, reports_routing_key)| async move {
                info!(
                    "sending response {} to queue {}",
                    response.job_name, response.result_queue
                );
                let queue = mem::replace(&mut response.result_queue, "".to_owned());
                let delivery_tag = mem::replace(&mut response.delivery_tag, 0);
                channel.basic_publish("", &queue, &response).await?;
                ack_channel.basic_ack(delivery_tag).await?;
                if let Some(reports_routing_key) = reports_routing_key {
                    info!(
                        "additionnaly sending {} to queue {}",
                        response.job_name, reports_routing_key
                    );
                    channel
                        .basic_publish("", &reports_routing_key, &response)
                        .await?;
                }
                Ok(()) as Result<(), AMQPError>
            },
        )
        .await?;
    Ok(())
}

pub async fn amqp_process(
    config: &Arc<Configuration>,
    send_request: Sender<AMQPRequest>,
    receive_response: Receiver<AMQPResponse>,
) -> Result<(), AMQPError> {
    let conn = AMQPConnection::new(&config.amqp).await?;
    let receiver_channel = conn.create_channel().await?;
    receiver_channel
        .declare_exchange_and_queue(&config.amqp)
        .await?;
    let receiver = amqp_receiver(&receiver_channel, &config, send_request);
    let sender_channel = conn.create_channel().await?;
    let sender = amqp_sender(
        &sender_channel,
        &receiver_channel,
        receive_response,
        config.amqp.reports_routing_key.clone(),
    );
    futures::try_join!(receiver, sender)?;
    Ok(())
}

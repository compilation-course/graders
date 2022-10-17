use amqp_utils::{self, AmqpChannel, AmqpConnection, AmqpError, AmqpRequest, AmqpResponse};
use futures::channel::mpsc::{Receiver, Sender};
use futures::future;
use futures::sink::SinkExt;
use futures::stream::{StreamExt, TryStreamExt};
use std::convert::TryInto;
use std::mem;
use std::sync::Arc;

use crate::config::Configuration;

async fn amqp_receiver(
    channel: &AmqpChannel,
    config: &Arc<Configuration>,
    send_request: Sender<AmqpRequest>,
) -> Result<(), AmqpError> {
    let prefetch_count = if let Ok(p) = config.tester.parallelism.try_into() {
        p
    } else {
        log::warn!(
            "Value for config.tester.parallelism is too large ({}), using 1",
            config.tester.parallelism
        );
        1
    };
    channel.basic_qos(prefetch_count).await?;
    let stream = channel
        .basic_consume(&config.amqp.queue, "amqp-to-test")
        .await?;
    let mut data = stream
        .map(|msg| {
            let msg = msg?;
            let request = msg.decode_payload()?;
            Ok(AmqpRequest {
                delivery_tag: Some(msg.delivery_tag()),
                ..request
            })
        })
        .filter(|e| future::ready(e.is_ok()));
    send_request
        .sink_map_err(|e| {
            log::warn!("sink error: {}", e);
            AmqpError::from(e)
        })
        .send_all(&mut data)
        .await?;
    Ok(())
}

// Acks must be sent on the original channel. Sending concurrently
// is supposed to be compatible with basic_consume.
async fn amqp_sender(
    channel: &AmqpChannel,
    ack_channel: &AmqpChannel,
    receive_response: Receiver<AmqpResponse>,
    reports_routing_key: Option<String>,
) -> Result<(), AmqpError> {
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
                log::info!(
                    "sending response {} to queue {}",
                    response.job_name,
                    response.result_queue
                );
                let queue = mem::replace(&mut response.result_queue, "".to_owned());
                let delivery_tag = mem::replace(&mut response.delivery_tag, 0);
                channel.basic_publish("", &queue, &response).await?;
                ack_channel.basic_ack(delivery_tag).await?;
                if let Some(reports_routing_key) = reports_routing_key {
                    log::info!(
                        "additionaly sending {} to queue {}",
                        response.job_name,
                        reports_routing_key
                    );
                    channel
                        .basic_publish("", &reports_routing_key, &response)
                        .await?;
                }
                Ok(()) as Result<(), AmqpError>
            },
        )
        .await?;
    Ok(())
}

#[allow(clippy::module_name_repetitions)]
pub async fn amqp_process(
    config: &Arc<Configuration>,
    send_request: Sender<AmqpRequest>,
    receive_response: Receiver<AmqpResponse>,
) -> Result<(), AmqpError> {
    let conn = AmqpConnection::new(&config.amqp).await?;
    let receiver_channel = conn.create_channel().await?;
    receiver_channel
        .declare_exchange_and_queue(&config.amqp)
        .await?;
    let receiver = amqp_receiver(&receiver_channel, config, send_request);
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

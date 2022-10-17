use amqp_utils::{self, AmqpChannel, AmqpConnection, AmqpError, AmqpRequest, AmqpResponse};
use futures::channel::mpsc::{Receiver, Sender};
use futures::{future, try_join, FutureExt, SinkExt, StreamExt, TryFutureExt, TryStreamExt};
use std::sync::Arc;

use crate::config::Configuration;
use crate::gitlab;

async fn amqp_publisher(
    channel: AmqpChannel,
    config: &Arc<Configuration>,
    receive_request: Receiver<AmqpRequest>,
) -> Result<(), AmqpError> {
    receive_request
        .map(Ok)
        .try_for_each(|req| {
            let channel = channel.clone();
            let config = config.clone();
            async move {
                log::info!("publishing AMQP job request {}", req.job_name);
                channel
                    .basic_publish(&config.amqp.exchange, &config.amqp.routing_key, &req)
                    .await
            }
        })
        .await
}

async fn amqp_receiver(
    channel: AmqpChannel,
    send_response: Sender<AmqpResponse>,
) -> Result<(), AmqpError> {
    channel.queue_declare_durable(gitlab::RESULT_QUEUE).await?;
    let stream = channel
        .basic_consume(gitlab::RESULT_QUEUE, "gitlab-to-amqp")
        .await?;
    log::info!("listening onto the {} queue", gitlab::RESULT_QUEUE);
    let data = stream
        .err_into()
        .and_then(|msg| {
            let channel = channel.clone();
            async move {
                channel.basic_ack(msg.delivery_tag()).await?;
                let response = msg.decode_payload()?;
                Ok(response)
            }
        })
        .filter(|r| future::ready(r.is_ok()));
    pin_utils::pin_mut!(data);
    send_response
        .sink_map_err(|e| {
            log::warn!("sink error: {}", e);
            AmqpError::from(e)
        })
        .send_all(&mut data)
        .inspect(|_| {
            log::warn!(
                "terminating listening onto the {} queue",
                gitlab::RESULT_QUEUE
            );
        })
        .await?;
    Ok(())
}

#[allow(clippy::module_name_repetitions)]
pub async fn amqp_process(
    config: &Arc<Configuration>,
    receive_request: Receiver<AmqpRequest>,
    send_response: Sender<AmqpResponse>,
) -> Result<(), AmqpError> {
    let conn = AmqpConnection::new(&config.amqp)
        .await
        .map_err(AmqpError::from)?;
    let config = config.clone();
    let publisher = {
        let channel = conn.create_channel().await?;
        channel.declare_exchange_and_queue(&config.amqp).await?;
        amqp_publisher(channel, &config, receive_request)
    };
    let receiver = {
        let channel = conn.create_channel().await?;
        amqp_receiver(channel, send_response)
    };
    try_join!(publisher.err_into(), receiver)?;
    Ok(())
}

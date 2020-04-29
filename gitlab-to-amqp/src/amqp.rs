use amqp_utils::{self, AMQPChannel, AMQPConnection, AMQPError, AMQPRequest, AMQPResponse};
use futures::channel::mpsc::{Receiver, Sender};
use futures::{future, try_join, FutureExt, SinkExt, StreamExt, TryFutureExt, TryStreamExt};
use std::sync::Arc;

use crate::config::Configuration;
use crate::gitlab;

async fn amqp_publisher(
    channel: AMQPChannel,
    config: &Arc<Configuration>,
    receive_request: Receiver<AMQPRequest>,
) -> Result<(), AMQPError> {
    receive_request
        .map(Ok)
        .try_for_each(|req| {
            let channel = channel.clone();
            let config = config.clone();
            async move {
                info!("publishing AMQP job request {}", req.job_name);
                channel.basic_publish(
                    &config.amqp.exchange,
                    &config.amqp.routing_key,
                    &req,
                )
                .await
            }
        })
        .await
}

async fn amqp_receiver(
    channel: AMQPChannel,
    send_response: Sender<AMQPResponse>,
) -> Result<(), AMQPError> {
    channel.queue_declare_durable(gitlab::RESULT_QUEUE).await?;
    let stream =
        channel.basic_consume(gitlab::RESULT_QUEUE, "gitlab-to-amqp").await?;
    info!("listening onto the {} queue", gitlab::RESULT_QUEUE);
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
            warn!("sink error: {}", e);
            AMQPError::with("sink error", e)
        })
        .send_all(&mut data)
        .inspect(|_| {
            warn!(
                "terminating listening onto the {} queue",
                gitlab::RESULT_QUEUE
            )
        })
        .await?;
    Ok(())
}

pub async fn amqp_process(
    config: &Arc<Configuration>,
    receive_request: Receiver<AMQPRequest>,
    send_response: Sender<AMQPResponse>,
) -> Result<(), AMQPError> {
    let conn = AMQPConnection::new(&config.amqp)
        .await
        .map_err(|e| AMQPError::with("cannot connect to AMQP server", e))?;
    let config = config.clone();
    let publisher = {
        let channel = conn.create_channel().await?;
        let _queue = channel.declare_exchange_and_queue(&config.amqp).await?;
        amqp_publisher(channel, &config, receive_request)
    };
    let receiver = {
        let channel = conn.create_channel().await?;
        amqp_receiver(channel, send_response)
    };
    try_join!(publisher.err_into(), receiver)?;
    Ok(())
}

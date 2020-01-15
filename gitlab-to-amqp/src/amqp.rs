use failure::{format_err, ResultExt};
use futures::channel::mpsc::{Receiver, Sender};
use futures::{future, try_join, FutureExt, SinkExt, StreamExt, TryFutureExt, TryStreamExt};
use graders_utils::amqputils::{self, AMQPRequest, AMQPResponse};
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel};
use serde_json;
use std::sync::Arc;

use crate::config::Configuration;
use crate::gitlab;

async fn amqp_publisher(
    channel: Channel,
    config: &Arc<Configuration>,
    receive_request: Receiver<AMQPRequest>,
) -> Result<(), lapin::Error> {
    receive_request
        .map(Ok)
        .try_for_each(|req| {
            let channel = channel.clone();
            let config = config.clone();
            async move {
                info!("publishing AMQP job request {}", req.job_name);
                channel
                    .basic_publish(
                        &config.amqp.exchange,
                        &config.amqp.routing_key,
                        BasicPublishOptions::default(),
                        serde_json::to_string(&req).unwrap().as_bytes().to_vec(),
                        BasicProperties::default(),
                    )
                    .await
            }
        })
        .await
}

async fn amqp_receiver(
    channel: Channel,
    send_response: Sender<AMQPResponse>,
) -> Result<(), failure::Error> {
    let result_queue = channel
        .queue_declare(
            gitlab::RESULT_QUEUE,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;
    let stream = channel
        .basic_consume(
            &result_queue,
            "gitlab-to-amqp",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;
    info!("listening onto the {} queue", gitlab::RESULT_QUEUE);
    let mut data = stream
        .err_into()
        .and_then(|msg| async {
            channel
                .basic_ack(msg.delivery_tag, BasicAckOptions { multiple: false })
                .await?;
            let s =
                String::from_utf8(msg.data).with_context(|e| format!("invalid UTF-8: {}", e))?;
            let response = serde_json::from_str::<AMQPResponse>(&s)
                .with_context(|e| format!("cannot decode json: {}", e))?;
            Ok(response)
        })
        .filter(|r| future::ready(r.is_ok()))
        .boxed();
    send_response
        .sink_map_err(|e| {
            warn!("sink error: {}", e);
            format_err!("sink error: {}", e)
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
) -> Result<(), failure::Error> {
    let conn = amqputils::create_connection(&config.amqp).await?;
    let config = config.clone();
    let publisher = {
        let channel = conn.create_channel().await?;
        let _queue = amqputils::declare_exchange_and_queue(&channel, &config.amqp).await?;
        amqp_publisher(channel, &config, receive_request)
    };
    let receiver = {
        let channel = conn.create_channel().await?;
        amqp_receiver(channel, send_response)
    };
    try_join!(publisher.err_into(), receiver)?;
    Ok(())
}

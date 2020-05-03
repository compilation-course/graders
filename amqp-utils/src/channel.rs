use crate::{AMQPConfiguration, AMQPDelivery, AMQPError};
use futures::future::TryFutureExt;
use futures::stream::{Stream, TryStreamExt};
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, BasicQosOptions,
    ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, CloseOnDrop, ExchangeKind};
use serde::ser::Serialize;
use std::rc::Rc;

#[derive(Clone)]
pub struct AMQPChannel {
    pub(crate) inner: Rc<CloseOnDrop<Channel>>,
}

impl AMQPChannel {
    pub async fn declare_exchange_and_queue(
        &self,
        config: &AMQPConfiguration,
    ) -> Result<(), AMQPError> {
        self.inner
            .exchange_declare(
                &config.exchange,
                ExchangeKind::Direct,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .inspect_err(|e| {
                error!("cannot declare exchange {}: {}", config.exchange, e);
            })
            .await?;
        self.queue_declare_durable(&config.queue)
            .inspect_err(|e| {
                error!("could not declare queue {}: {}", &config.queue, e);
            })
            .await?;
        self.inner
            .queue_bind(
                &config.queue,
                &config.exchange,
                &config.routing_key,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .inspect_err(move |e| {
                error!(
                    "could not bind queue {} to exchange {} using routing key {}: {}",
                    config.queue, config.exchange, config.routing_key, e
                );
            })
            .await?;
        Ok(())
    }

    pub async fn basic_publish<T>(
        &self,
        exchange: &str,
        routing_key: &str,
        data: &T,
    ) -> Result<(), AMQPError>
    where
        T: ?Sized + Serialize,
    {
        self.inner
            .basic_publish(
                exchange,
                routing_key,
                BasicPublishOptions::default(),
                serde_json::to_string(data).unwrap().as_bytes().to_vec(),
                BasicProperties::default(),
            )
            .await?;
        Ok(())
    }

    pub async fn basic_ack(&self, delivery_tag: u64) -> Result<(), AMQPError> {
        self.inner
            .basic_ack(delivery_tag, BasicAckOptions { multiple: false })
            .await?;
        Ok(())
    }

    pub async fn basic_qos(&self, prefetch_count: u16) -> Result<(), AMQPError> {
        self.inner
            .basic_qos(prefetch_count, BasicQosOptions { global: false })
            .await?;
        Ok(())
    }

    pub async fn queue_declare_durable(&self, queue_name: &str) -> Result<(), AMQPError> {
        self.inner
            .queue_declare(
                queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;
        Ok(())
    }

    pub async fn basic_consume(
        &self,
        queue_name: &str,
        consumer_tag: &str,
    ) -> Result<impl Stream<Item = Result<AMQPDelivery, AMQPError>>, AMQPError> {
        let consumer = self
            .inner
            .basic_consume(
                queue_name,
                consumer_tag,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;
        Ok(consumer
            .map_ok(|d| AMQPDelivery { inner: d })
            .map_err(AMQPError::from))
    }
}

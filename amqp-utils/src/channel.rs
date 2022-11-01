#![allow(clippy::module_name_repetitions)]

use crate::{AmqpConfiguration, AmqpDelivery, AmqpError};
use futures::future::TryFutureExt;
use futures::stream::{Stream, TryStreamExt};
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, BasicQosOptions,
    ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, ExchangeKind};
use serde::ser::Serialize;
use std::rc::Rc;

#[derive(Clone)]
pub struct AmqpChannel {
    pub(crate) inner: Rc<Channel>,
}

impl AmqpChannel {
    pub async fn declare_exchange_and_queue(
        &self,
        config: &AmqpConfiguration,
    ) -> Result<(), AmqpError> {
        self.inner
            .exchange_declare(
                &config.exchange,
                ExchangeKind::Direct,
                ExchangeDeclareOptions {
                    durable: true,
                    ..ExchangeDeclareOptions::default()
                },
                FieldTable::default(),
            )
            .inspect_err(|e| {
                log::error!("cannot declare exchange {}: {}", config.exchange, e);
            })
            .await?;
        self.queue_declare_durable(&config.queue)
            .inspect_err(|e| {
                log::error!("could not declare queue {}: {}", &config.queue, e);
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
                log::error!(
                    "could not bind queue {} to exchange {} using routing key {}: {}",
                    config.queue,
                    config.exchange,
                    config.routing_key,
                    e
                );
            })
            .await?;
        Ok(())
    }

    /// Publish data
    ///
    /// # Panics
    ///
    /// Serialization can fail if `T`'s implementation of `Serialize` decides to
    /// fail, or if `T` contains a map with non-string keys.
    pub async fn basic_publish<T>(
        &self,
        exchange: &str,
        routing_key: &str,
        data: &T,
    ) -> Result<(), AmqpError>
    where
        T: ?Sized + Serialize,
    {
        self.inner
            .basic_publish(
                exchange,
                routing_key,
                BasicPublishOptions::default(),
                serde_json::to_string(data).unwrap().as_bytes(),
                BasicProperties::default(),
            )
            .await?;
        Ok(())
    }

    pub async fn basic_ack(&self, delivery_tag: u64) -> Result<(), AmqpError> {
        self.inner
            .basic_ack(delivery_tag, BasicAckOptions { multiple: false })
            .await?;
        Ok(())
    }

    pub async fn basic_qos(&self, prefetch_count: u16) -> Result<(), AmqpError> {
        self.inner
            .basic_qos(prefetch_count, BasicQosOptions { global: false })
            .await?;
        Ok(())
    }

    pub async fn queue_declare_durable(&self, queue_name: &str) -> Result<(), AmqpError> {
        self.inner
            .queue_declare(
                queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..QueueDeclareOptions::default()
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
    ) -> Result<impl Stream<Item = Result<AmqpDelivery, AmqpError>>, AmqpError> {
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
            .map_ok(|delivery| AmqpDelivery { inner: delivery })
            .map_err(AmqpError::from))
    }
}

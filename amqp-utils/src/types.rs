use crate::errors::*;
use crate::AmqpChannel;
use futures::future::TryFutureExt;
use lapin::{Connection, ConnectionProperties};
use serde::de::Deserialize;
use std::rc::Rc;

pub struct AmqpConnection {
    pub(crate) inner: Connection,
}

impl AmqpConnection {
    /// Return a client that will connect to a remote AMQP server.
    pub async fn new(config: &AmqpConfiguration) -> Result<AmqpConnection, AmqpError> {
        // TODO Check how to add heartbeat
        let dest = format!("amqp://{}:{}/%2f", config.host, config.port);
        let connection = Connection::connect(&dest, ConnectionProperties::default())
            .inspect_err(|e| {
                warn!("error when connecting AMQP client to {}: {}", dest, e);
            })
            .await?;
        Ok(AmqpConnection { inner: connection })
    }

    pub async fn create_channel(&self) -> Result<AmqpChannel, AmqpError> {
        let channel = self.inner.create_channel().await?;
        Ok(AmqpChannel {
            inner: Rc::new(channel),
        })
    }
}

pub struct AmqpDelivery {
    pub(crate) inner: lapin::message::Delivery,
}

impl AmqpDelivery {
    pub fn delivery_tag(&self) -> u64 {
        self.inner.delivery_tag
    }
    pub fn decode_payload<'de, T: Deserialize<'de>>(&'de self) -> Result<T, AmqpError> {
        let s = std::str::from_utf8(&self.inner.data)?;
        Ok(serde_json::from_str::<T>(&s)?)
    }
}

#[derive(Clone, Deserialize)]
pub struct AmqpConfiguration {
    pub host: String,
    pub port: u16,
    pub exchange: String,
    pub routing_key: String,
    pub queue: String,
    pub reports_routing_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AmqpRequest {
    pub job_name: String,
    pub lab: String,
    pub dir: String,
    pub zip_url: String,
    pub result_queue: String,
    pub opaque: String,
    /// The delivery tag will be set upon message reception
    pub delivery_tag: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AmqpResponse {
    pub job_name: String,
    pub lab: String,
    pub opaque: String,
    pub yaml_result: String,
    /// The delivery tag and result queue will be removed before message emission
    pub result_queue: String,
    pub delivery_tag: u64,
}

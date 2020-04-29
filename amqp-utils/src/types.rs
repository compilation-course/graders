use crate::AMQPChannel;
use futures::future::TryFutureExt;
use lapin::{Connection, ConnectionProperties};
use serde::de::Deserialize;
use std::error::Error;
use std::fmt;
use std::ops::Deref;

pub struct AMQPConnection {
    pub(crate) inner: lapin::Connection,
}

impl AMQPConnection {
    /// Return a client that will connect to a remote AMQP server.
    pub async fn new(config: &AMQPConfiguration) -> Result<AMQPConnection, AMQPError> {
        // TODO Check how to add heartbeat
        let dest = format!("amqp://{}:{}/%2f", config.host, config.port);
        let connection = Connection::connect(&dest, ConnectionProperties::default())
            .inspect_err(|e| {
                warn!("error when connecting AMQP client to {}: {}", dest, e);
            })
            .await?;
        Ok(AMQPConnection { inner: connection })
    }

    pub async fn create_channel(&self) -> Result<AMQPChannel, AMQPError> {
        let channel = self.inner.create_channel().await?;
        Ok(AMQPChannel { inner: channel })
    }
}

pub struct AMQPDelivery {
    pub(crate) inner: lapin::message::Delivery,
}

impl AMQPDelivery {
    pub fn delivery_tag(&self) -> u64 {
        self.inner.delivery_tag
    }
    pub fn decode_payload<'de, T: Deserialize<'de>>(&'de self) -> Result<T, AMQPError> {
        let s = std::str::from_utf8(&self.inner.data)?;
        Ok(serde_json::from_str::<T>(&s)?)
    }
}

#[derive(Clone, Deserialize)]
pub struct AMQPConfiguration {
    pub host: String,
    pub port: u16,
    pub exchange: String,
    pub routing_key: String,
    pub queue: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AMQPRequest {
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
pub struct AMQPResponse {
    pub job_name: String,
    pub lab: String,
    pub opaque: String,
    pub yaml_result: String,
    /// The delivery tag and result queue will be removed before message emission
    pub result_queue: String,
    pub delivery_tag: u64,
}

#[derive(Debug)]
pub struct AMQPError {
    message: String,
    source: Option<Box<dyn std::error::Error + 'static + Send + Sync>>,
}

impl AMQPError {
    pub fn new<S: Deref<Target = str>>(message: S) -> AMQPError {
        AMQPError {
            message: message.to_owned(),
            source: None,
        }
    }

    pub fn with<S: Deref<Target = str>, E: std::error::Error + 'static + Send + Sync>(
        message: S,
        error: E,
    ) -> AMQPError {
        AMQPError {
            message: message.to_owned(),
            source: Some(Box::new(error)),
        }
    }
}

impl fmt::Display for AMQPError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(formatter, "{}", self.message)
    }
}

impl Error for AMQPError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.source {
            Some(ref e) => Some(e.as_ref()),
            None => None,
        }
    }
}

impl From<lapin::Error> for AMQPError {
    fn from(error: lapin::Error) -> AMQPError {
        AMQPError::with("AMQP communication error", error)
    }
}

impl From<std::str::Utf8Error> for AMQPError {
    fn from(error: std::str::Utf8Error) -> AMQPError {
        AMQPError::with("UTF-8 decoding error", error)
    }
}

impl From<serde_json::error::Error> for AMQPError {
    fn from(error: serde_json::error::Error) -> AMQPError {
        AMQPError::with("JSON decoding error", error)
    }
}

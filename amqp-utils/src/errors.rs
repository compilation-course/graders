use futures::channel::mpsc::SendError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AmqpError {
    #[error("JSON decoding error")]
    Json(#[from] serde_json::error::Error),
    #[error("AMQP error")]
    Lapin(#[from] lapin::Error),
    #[error("Sink error")]
    SinkError(#[from] SendError),
    #[error("UTF-8 decoding error")]
    Utf8(#[from] std::str::Utf8Error),
}

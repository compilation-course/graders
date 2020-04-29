use std::error::Error;
use std::fmt;
use std::ops::Deref;

#[derive(Debug)]
pub enum AMQPError {
    Json(serde_json::error::Error),
    Simple(String),
    UTF8(std::str::Utf8Error),
}

impl AMQPError {
    pub fn new<S>(message: S) -> AMQPError
    where
        S: Deref<Target = str>,
    {
        AMQPError::Simple(message.to_owned())
    }
}

impl fmt::Display for AMQPError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            AMQPError::Json(e) => write!(formatter, "JSON decoding error: {}", e),
            AMQPError::Simple(s) => write!(formatter, "Error: {}", s),
            AMQPError::UTF8(e) => write!(formatter, "UTF-8 decoding error: {}", e),
        }
    }
}

impl Error for AMQPError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AMQPError::Json(ref e) => Some(e),
            AMQPError::Simple(_) => None,
            AMQPError::UTF8(ref e) => Some(e),
        }
    }
}

impl From<lapin::Error> for AMQPError {
    fn from(error: lapin::Error) -> AMQPError {
        AMQPError::Simple(format!("AMQP communication error: {}", error))
    }
}

impl From<std::str::Utf8Error> for AMQPError {
    fn from(error: std::str::Utf8Error) -> AMQPError {
        AMQPError::UTF8(error)
    }
}

impl From<serde_json::error::Error> for AMQPError {
    fn from(error: serde_json::error::Error) -> AMQPError {
        AMQPError::Json(error)
    }
}
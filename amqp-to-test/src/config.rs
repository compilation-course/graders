use amqp_utils::AmqpConfiguration;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;

use crate::tester::TesterConfiguration;

#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError {
    #[error("cannot decode configuration file {1}")]
    CannotDecode(#[source] serde_yaml::Error, String),
    #[error("cannot read configuration file {1}")]
    CannotRead(#[source] std::io::Error, String),
}

#[derive(Deserialize)]
pub struct Configuration {
    pub amqp: AmqpConfiguration,
    pub tester: TesterConfiguration,
}

pub fn load_configuration(file: &str) -> Result<Configuration, ConfigurationError> {
    let mut f = File::open(file).map_err(|e| ConfigurationError::CannotRead(e, file.to_owned()))?;
    let mut content = Vec::new();
    f.read_to_end(&mut content)
        .map_err(|e| ConfigurationError::CannotRead(e, file.to_owned()))?;
    serde_yaml::from_slice(&content)
        .map_err(|e| ConfigurationError::CannotDecode(e, file.to_owned()))
}

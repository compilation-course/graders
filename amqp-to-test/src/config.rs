use errors;
use graders_utils::amqputils::AMQPConfiguration;
use std::fs::File;
use std::io::Read;
use toml;

#[derive(Deserialize)]
pub struct Configuration {
    pub amqp: AMQPConfiguration,
}

pub fn load_configuration(file: &str) -> errors::Result<Configuration> {
    let mut f = File::open(file)?;
    let mut content = Vec::new();
    f.read_to_end(&mut content)?;
    Ok(toml::de::from_slice(&content)?)
}

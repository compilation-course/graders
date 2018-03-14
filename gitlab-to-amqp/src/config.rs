use errors;
use graders_utils::amqputils::AMQPConfiguration;
use serde_yaml;
use std::fs::{self, File};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::io::Read;
use url::Url;
use url_serde;

#[derive(Deserialize)]
pub struct Configuration {
    pub server: ServerConfiguration,
    pub gitlab: GitlabConfiguration,
    pub package: PackageConfiguration,
    pub labs: Vec<LabConfiguration>,
    pub amqp: AMQPConfiguration,
}

#[derive(Clone, Deserialize)]
pub struct ServerConfiguration {
    pub ip: IpAddr,
    pub port: u16,
    #[serde(with = "url_serde")]
    pub base_url: Url,
}

#[derive(Clone, Deserialize)]
pub struct GitlabConfiguration {
    pub token: String,
    #[serde(with = "url_serde")]
    pub base_url: Url,
}

#[derive(Clone, Deserialize)]
pub struct PackageConfiguration {
    pub threads: usize,
    pub zip_dir: PathBuf,
}

#[derive(Clone, Deserialize)]
pub struct LabConfiguration {
    pub name: String,
    pub base: PathBuf,
    pub dir: PathBuf,
    pub witness: Option<PathBuf>,
    pub enabled: Option<bool>,
}

impl LabConfiguration {
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }
}

pub fn load_configuration(file: &str) -> errors::Result<Configuration> {
    let mut f = File::open(file)?;
    let mut content = Vec::new();
    f.read_to_end(&mut content)?;
    Ok(serde_yaml::from_slice(&content)?)
}

pub fn setup_dirs(config: &Configuration) -> errors::Result<()> {
    let zip_dir = Path::new(&config.package.zip_dir);
    if !zip_dir.is_dir() {
        fs::create_dir(zip_dir)?;
    }
    Ok(())
}

use amqp_utils::AmqpConfiguration;
use failure::Error;
use std::fs::{self, File};
use std::io::Read;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Deserialize)]
pub struct Configuration {
    pub server: ServerConfiguration,
    pub gitlab: GitlabConfiguration,
    pub package: PackageConfiguration,
    pub labs: Vec<LabConfiguration>,
    pub amqp: AmqpConfiguration,
}

#[derive(Clone, Deserialize)]
pub struct ServerConfiguration {
    pub ip: IpAddr,
    pub port: u16,
    pub base_url: Url,
}

#[derive(Clone, Deserialize)]
pub struct GitlabConfiguration {
    pub token: String,
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
        self.enabled.unwrap_or(true)
    }
}

pub fn load_configuration(file: &str) -> Result<Configuration, Error> {
    let mut f = File::open(file)?;
    let mut content = Vec::new();
    f.read_to_end(&mut content)?;
    Ok(serde_yaml::from_slice(&content)?)
}

pub fn setup_dirs(config: &Configuration) -> Result<(), Error> {
    let zip_dir = Path::new(&config.package.zip_dir);
    if !zip_dir.is_dir() {
        fs::create_dir(zip_dir)?;
    }
    Ok(())
}

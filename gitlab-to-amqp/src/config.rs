use errors;
use std::fs::{self, File};
use std::net::IpAddr;
use std::path::Path;
use std::io::Read;
use toml;
use url::Url;
use url_serde;

#[derive(Clone, Deserialize)]
pub struct Configuration {
    pub server: ServerConfiguration,
    pub gitlab: GitlabConfiguration,
    pub package: PackageConfiguration,
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
}

#[derive(Clone, Deserialize)]
pub struct PackageConfiguration {
    pub threads: usize,
    pub zip_dir: String,
}

pub fn load_configuration(file: &str) -> errors::Result<Configuration> {
    let mut f = File::open(file)?;
    let mut content = Vec::new();
    f.read_to_end(&mut content)?;
    Ok(toml::de::from_slice(&content)?)
}

pub fn setup_dirs(config: &Configuration) -> errors::Result<()> {
    let zip_dir = Path::new(&config.package.zip_dir);
    if !zip_dir.is_dir() {
        fs::create_dir(zip_dir)?;
    }
    Ok(())
}

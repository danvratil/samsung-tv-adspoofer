use config::Config;
use std::path::PathBuf;

use serde::Deserialize;

use crate::cli::Args;

fn default_listen_address() -> String {
    "127.0.0.1".to_string()
}

fn default_listen_port() -> u16 {
    8080
}

fn default_database_path() -> PathBuf {
    std::env::current_dir().expect("CWD must exist").join("database.sqlite")
}

fn default_storage_path() -> PathBuf {
    std::env::current_dir().expect("CWD must exist").join("files")
}

fn default_base_url() -> String {
    format!("http://{}:{}", default_listen_address(), default_listen_port())
}

#[derive(Debug, Deserialize, Default)]
pub struct HttpServerConfig {
    #[serde(default = "default_listen_address")]
    pub listen_address: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
}

#[derive(Debug, Deserialize, Default)]
pub struct StorageConfig {
    #[serde(default = "default_database_path")]
    pub database_path: PathBuf,
    #[serde(default = "default_storage_path")]
    pub image_storage_path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub http_server: HttpServerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

pub fn load_config(args: Args) -> Result<AppConfig, config::ConfigError> {
    Config::builder()
        .add_source(config::Environment::with_prefix("ADSPOOFER"))
        .add_source(args)
        .build()?
        .try_deserialize::<AppConfig>()
}

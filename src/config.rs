use config::Config;
use std::path::PathBuf;
use tracing::level_filters::LevelFilter;

use serde::{Deserialize, Serialize};

use crate::cli::Args;

fn default_listen_address() -> String {
    "127.0.0.1".to_string()
}

fn default_listen_port() -> u16 {
    8080
}

fn default_database_path() -> PathBuf {
    std::env::current_dir()
        .expect("CWD must exist")
        .join("database.sqlite")
}

fn default_storage_path() -> PathBuf {
    std::env::current_dir()
        .expect("CWD must exist")
        .join("files")
}

fn default_base_url() -> String {
    format!(
        "http://{}:{}",
        default_listen_address(),
        default_listen_port()
    )
}

fn default_log_level() -> LogLevel {
    LogLevel::Info
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warning => "warning",
            LogLevel::Error => "error",
        })
    }
}

impl LogLevel {
    pub fn to_filter_level(&self) -> LevelFilter {
        match self {
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Warning => LevelFilter::WARN,
            LogLevel::Error => LevelFilter::ERROR,
        }
    }
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
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
}

pub fn load_config(args: Args) -> Result<AppConfig, config::ConfigError> {
    Config::builder()
        .add_source(config::Environment::with_prefix("ADSPOOFER"))
        .add_source(args)
        .build()?
        .try_deserialize::<AppConfig>()
}

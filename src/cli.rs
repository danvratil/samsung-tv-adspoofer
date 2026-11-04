use std::net::SocketAddr;

use clap::Parser;
use config::{ConfigError, Map, Value};

use crate::config::LogLevel;

#[derive(Clone, Debug, Parser)]
pub struct Args {
    #[arg(
        short,
        long,
        default_value = "127.0.0.1:8080",
        value_name = "host:port"
    )]
    pub listen: Option<String>,
    #[arg(short, long, default_value = "./database.sqlite", value_name = "path")]
    pub database: Option<String>,
    #[arg(short, long, default_value = "./files/", value_name = "path")]
    pub storage: Option<String>,
    #[arg(short, long, default_value = "http://127.0.0.1:8080")]
    pub base_url: Option<String>,
    #[arg(long, value_name = "level", value_enum)]
    pub log_level: Option<LogLevel>,
}

impl config::Source for Args {
    fn clone_into_box(&self) -> Box<dyn config::Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<Map<String, Value>, ConfigError> {
        let mut map = Map::new();
        if let Some(ref listen) = self.listen {
            let addr: SocketAddr = listen
                .parse()
                .map_err(|e| ConfigError::Message(format!("Invalid listen address: {e}")))?;
            map.insert(
                "http_server.listen_address".to_string(),
                Value::from(addr.ip().to_string()),
            );
            map.insert(
                "http_server.listen_port".to_string(),
                Value::from(addr.port()),
            );
        };
        if let Some(ref database) = self.database {
            map.insert(
                "storage.database_path".to_string(),
                Value::from(database.clone()),
            );
        }
        if let Some(ref storage) = self.storage {
            map.insert(
                "storage.image_storage_path".to_string(),
                Value::from(storage.clone()),
            );
        }
        if let Some(ref base_url) = self.base_url {
            map.insert("base_url".to_string(), Value::from(base_url.clone()));
        }
        if let Some(log_level) = self.log_level {
            map.insert("log_level".to_string(), Value::from(log_level.to_string()));
        }

        Ok(map)
    }
}

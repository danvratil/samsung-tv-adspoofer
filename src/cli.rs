// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

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
    #[arg(
        short,
        long,
        default_value = "127.0.0.1:8443",
        value_name = "host:port"
    )]
    pub tls_listen: Option<String>,
    #[arg(short, long, default_value = "./database.sqlite", value_name = "path")]
    pub database: Option<String>,
    #[arg(short, long, default_value = "./files/", value_name = "path")]
    pub storage: Option<String>,
    #[arg(short, long, default_value = "http://127.0.0.1:8080", help = "Base URL on which the service is accessible")]
    pub base_url: Option<String>,
    #[arg(long, value_name = "level", value_enum)]
    pub log_level: Option<LogLevel>,
    #[arg(long, value_name = "PATH", help = "PEM-formatted full certificate chain.")]
    pub tls_chain_path: Option<String>,
    #[arg(long, value_name = "PATH", help = "PEM-formatted private key.")]
    pub tls_key_path: Option<String>,
}

fn update_listen_addr(listen: &str, dest: &str) -> Result<Map<String, Value>, ConfigError> {
    let addr: SocketAddr = listen
        .parse()
        .map_err(|e| ConfigError::Message(format!("Invalid listen address: {e}")))?;

    Ok(Map::from_iter([
        (format!("{dest}.listen_address"),
        Value::from(addr.ip().to_string())),
        (format!("{dest}.listen_port"),
        Value::from(addr.port()),
    )]))
}

fn update_map<Val: ToString>(map: &mut Map<String, Value>, key: &str, value: &Option<Val>) {
    if let Some(val) = value {
        map.insert(key.to_string(), Value::from(val.to_string()));
    };
}


impl config::Source for Args {
    fn clone_into_box(&self) -> Box<dyn config::Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<Map<String, Value>, ConfigError> {
        let mut map = Map::new();
        if let Some(ref listen) = self.listen {
            map.extend(update_listen_addr(listen, "http_server")?);
        };
        if let Some(ref listen) = self.tls_listen {
            map.extend(update_listen_addr(listen, "https_server")?);
        };

        update_map(&mut map, "storage.database_path", &self.database);
        update_map(&mut map, "storage.image_storage_path", &self.storage);
        update_map(&mut map, "base_url", &self.base_url);
        update_map(&mut map, "log_level", &self.log_level);
        update_map(&mut map, "https_server.tls_chain_path", &self.tls_chain_path);
        update_map(&mut map, "https_server.tls_private_key_path", &self.tls_key_path);

        Ok(map)
    }
}

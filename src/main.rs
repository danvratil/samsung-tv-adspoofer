// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

use std::error::Error;
use tracing_subscriber::{filter, fmt, prelude::*, reload};

use clap::Parser;
use samsung_tv_adspoofer::{cli, config, create_app};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (filter, reload) =
        reload::Layer::new(fmt::Layer::default().with_filter(filter::LevelFilter::INFO));

    tracing_subscriber::registry().with(filter).init();

    tracing::info!("Starting Samsung TV Ad Spoofer...");

    let args = cli::Args::parse();
    let config = config::load_config(args).map_err(|e| {
        tracing::error!("Failed to load configuration: {:?}", e);
        e
    })?;

    _ = reload.modify(|layer| *layer.filter_mut() = config.log_level.to_filter_level());

    create_app(config).await
}

use std::error::Error;

use samsung_tv_adspoofer::{cli, config, create_app};
use tokio;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting Samsung TV Ad Spoofer...");

    let args = cli::Args::parse();
    let config = config::load_config(args).expect("Failed to load configuration");
    create_app(config).await
}

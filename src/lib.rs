// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response};
use axum::routing::{get, post};

use axum_server::tls_rustls::RustlsConfig;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use minijinja::Environment;

use tokio::net::TcpListener;

use tracing::{Span, info, warn};

use crate::config::{AppConfig, HttpsServerConfig};
use crate::database::Database;
use crate::pictures::Pictures;

pub mod cli;
pub mod config;
mod database;
mod handlers;
mod models;
mod pictures;

pub use models::{Crop, CroppedPicture, Picture, PictureId};

pub struct AppState<'a> {
    db: Database,
    pictures: Pictures<'a>,
    config: AppConfig,
    jinja: Environment<'a>,
}

async fn into_tls_config(
    config: &HttpsServerConfig,
) -> Result<Option<RustlsConfig>, std::io::Error> {
    match (&config.tls_private_key_path, &config.tls_chain_path) {
        (Some(tls_private_key_path), Some(tls_chain_path)) => {
            if !Path::new(tls_private_key_path).exists() {
                warn!("TLS private key PEM file doesn't exist, not running a TLS service");
                return Ok(None);
            }
            if !Path::new(tls_chain_path).exists() {
                warn!(
                    "TLS fullchain certificate PEM file doesn't exist, not running a TLS service"
                );
                return Ok(None);
            }

            Ok(Some(
                RustlsConfig::from_pem_chain_file(tls_chain_path, tls_private_key_path).await?,
            ))
        }
        _ => Ok(None),
    }
}

pub async fn create_app(config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut jinja = Environment::new();
    jinja.add_template("home.html", include_str!("../templates/home.html.jinja"))?;

    let db = Database::new(&config.storage.database_path)?;

    let shared_state = Arc::new_cyclic(|state| AppState {
        db,
        pictures: Pictures::new(state.clone()),
        config,
        jinja,
    });

    shared_state.db.init_database()?;

    let app = Router::new()
        // Frontend for managing the pictures
        .route("/", get(handlers::frontend::home_handler))
        .route("/images/", post(handlers::frontend::add_image_handler))
        .route(
            "/images/{image_id}/delete",
            post(handlers::frontend::delete_image_handler),
        )
        .route(
            "/images/{image_id}",
            get(handlers::frontend::get_image_handler),
        )
        .route(
            "/images/{image_id}/{kind}",
            get(handlers::frontend::get_image_kind_handler),
        )
        // This is the main route: the SmartHub calls this to get the top banner ad.
        .route(
            "/impressions/tile",
            get(handlers::adgrx::impressions_title_handler),
        )
        // Since we are redirecting all adgrx.com requests to this server, we want to graciously
        // handle all other paths that the TV seems to be hitting, just so the 404s don't prevent
        // the TV from loading the adverts.
        .route(
            "/impressions/{*path}",
            get(handlers::adgrx::blackhole_handler),
        )
        .route("/adopportunity", get(handlers::adgrx::blackhole_handler))
        .route("/loads/{*path}", get(handlers::adgrx::blackhole_handler))
        .route("/tracking/{*path}", get(handlers::adgrx::blackhole_handler))
        .layer(
            ServiceBuilder::new().layer(
                TraceLayer::new_for_http()
                    .make_span_with(|_: &Request<Body>| tracing::debug_span!("http-request"))
                    .on_request(|request: &Request<Body>, _span: &Span| {
                        tracing::debug!("started {} {}", request.method(), request.uri().path())
                    })
                    .on_response(
                        |response: &Response<Body>, _latency: Duration, _span: &Span| {
                            tracing::debug!("response status {}", response.status())
                        },
                    ),
            ),
        )
        .with_state(Arc::clone(&shared_state));

    // Plain listener
    let listen = &shared_state.config.http_server;
    let listen = SocketAddr::new(
        IpAddr::from_str(&listen.listen_address)?,
        listen.listen_port,
    );
    let listener = TcpListener::bind(listen).await?;
    let plain_app = app.clone();
    let plain_server = tokio::spawn(async move {
        info!("Starting AdSpoofer plain listener on {listen}");
        axum::serve(listener, plain_app).await
    });

    // TLS listener (optional)
    let tls_listen = &shared_state.config.https_server;
    if let Some(tls_config) = into_tls_config(tls_listen).await? {
        let tls_listen = SocketAddr::new(
            IpAddr::from_str(&tls_listen.listen_address)?,
            tls_listen.listen_port,
        );
        info!("Starting AdSpoofer TLS listener on {tls_listen}");
        axum_server::bind_rustls(tls_listen, tls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        _ = plain_server.await?;
    };

    Ok(())
}

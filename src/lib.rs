use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response};
use axum::routing::{get, post};

use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use minijinja::Environment;

use tokio::net::TcpListener;

use tracing::Span;

use crate::config::AppConfig;
use crate::database::Database;
use crate::pictures::Pictures;

pub mod cli;
pub mod config;
mod database;
mod handlers;
mod models;
mod pictures;

pub use models::{Crop, Picture, PictureId, CroppedPicture};

pub struct AppState<'a> {
    db: Database,
    pictures: Pictures<'a>,
    config: AppConfig,
    jinja: Environment<'a>,
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

    let listen = &shared_state.config.http_server;
    let listener =
        TcpListener::bind(format!("{}:{}", listen.listen_address, listen.listen_port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

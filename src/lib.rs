use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{Body, Bytes};
use axum::extract::{Multipart, Path, State};
use axum::http::{Response, StatusCode};
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{Json, Router};

use minijinja::{Environment, context};

use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;

use serde_json::{Value, json};
use tracing::error;

use crate::config::AppConfig;
use crate::database::{Database, PictureId};

pub mod cli;
pub mod config;
mod database;

pub struct AppState<'a> {
    db: Database,
    config: AppConfig,
    jinja: Environment<'a>,
}

pub async fn create_app(config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut env = Environment::new();
    env.add_template("home.html", include_str!("../templates/home.html.jinja"))
        .expect("Template must load successfully");

    let shared_state = Arc::new(AppState {
        db: Database::new(&config.storage.database_path)?,
        config,
        jinja: env,
    });

    shared_state.db.init_database()?;

    let app = Router::new()
        .route("/", get(home_handler))
        .route("/images/", post(add_image_handler))
        .route("/images/{image_id}/delete", post(delete_image_handler))
        .route("/images/{image_id}", get(get_image_handler))
        .route("/impressions/title", get(impressions_title_handler))
        .with_state(Arc::clone(&shared_state));

    let listen = &shared_state.config.http_server;
    let listener =
        TcpListener::bind(format!("{}:{}", listen.listen_address, listen.listen_port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn home_handler(State(state): State<Arc<AppState<'_>>>) -> Result<Body, StatusCode> {
    let pictures = state.db.get_pictures().map_err(|e| {
        error!("Failed to get pictures: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let template = state
        .jinja
        .get_template("home.html")
        .expect("Template home.html must exist");
    let content = template
        .render(context!(
            pictures => minijinja::value::Serde(pictures)
        ))
        .map_err(|e| {
            error!("Failed to render template: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Body::from(content))
}

async fn add_image_handler(
    State(state): State<Arc<AppState<'_>>>,
    mut multipart: Multipart,
) -> Result<Redirect, StatusCode> {
    let mut picture_name: Option<String> = None;
    let mut file_data: Option<Bytes> = None;
    let mut file_name: Option<String> = None;

    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        match name.as_str() {
            "name" => {
                picture_name = Some(field.text().await.unwrap());
            }
            "file" => {
                file_name = Some(field.file_name().unwrap().to_string());
                file_data = Some(field.bytes().await.unwrap());
            }
            _ => {}
        }
    }

    let Some(file_name) = file_name else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let Some(file_data) = file_data else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let picture_name = match picture_name {
        Some(picture_name) => picture_name,
        None => file_name,
    };

    let file_name = uuid::Uuid::new_v4().to_string();
    let path = state.config.storage.image_storage_path.join(file_name);

    {
        let mut file = File::create_new(&path).await.map_err(|e| {
            error!("Failed to create file {:?}: {:?}", path, e);
            StatusCode::CONFLICT
        })?;
        file.write(&file_data).await.map_err(|e| {
            error!("Failed to write data into file {:?}: {:?}", path, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    state
        .db
        .add_picture(&picture_name, path.to_str().unwrap())
        .map_err(|e| {
            error!("Failed to add picture to database: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Redirect::to("/"))
}

async fn delete_image_handler(
    Path(image_id): Path<PictureId>,
    State(state): State<Arc<AppState<'_>>>,
) -> Result<Redirect, StatusCode> {
    state
        .db
        .delete_picture(image_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Redirect::to("/"))
}

async fn get_image_handler(
    Path(image_id): Path<PictureId>,
    State(state): State<Arc<AppState<'_>>>,
) -> Result<Response<Body>, StatusCode> {
    let path = match state.db.get_picture(image_id) {
        Ok(Some(picture)) => picture.path,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let file = File::open(path).await.map_err(|_| StatusCode::NOT_FOUND)?;
    let stream = ReaderStream::new(file);

    Response::builder()
        .body(Body::from_stream(stream))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn impressions_title_handler(
    State(state): State<Arc<AppState<'_>>>,
) -> Result<Json<Value>, StatusCode> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Epoch must be valid");
    let expire = now + Duration::from_hours(1);
    let picture_id = match state.db.get_random_picture() {
        Ok(Some(picture_id)) => picture_id,
        Ok(None) => return Err(StatusCode::NO_CONTENT),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let image_url = format!(
        "{}/images/{}",
        state.config.base_url,
        picture_id.to_string()
    );

    let response = json!({
        "immersive_banner": [
            {
                "ad": {
                    "adinfo": {
                        "adid": "000000-000000-000000-000",
                        "adtype": "6",
                        "guid": "00000000-0000-0000-0000-000000000000-0",
                        "adResponseVersion": "1.0",
                        "responseTime": now.as_secs().to_string(),
                        "expireTime": expire.as_secs().to_string(),
                        "rotationInterval": "5000",
                        "refreshInterval": "30000"
                    },
                    "creative": {
                        "cid": "0",
                        "clicktype": "1",
                        "clickaction": {
                            "appid": "",
                            "runtype": "",
                            "appcontentid": "",
                            "category": ""
                        },
                        "adtext": "",
                        "colorPreset": {
                            "buttonBackground": "CP_Plane110",
                            "buttonLabel": "CP_Info1100"
                        },
                        "backgroundimage": {
                            "width": "1920",
                            "height": "1080",
                            "imageurl": image_url,
                            "position": "0"
                        },
                        "titleimage": {
                            "width": "560",
                            "height": "200",
                            "imageurl": image_url,
                        },
                        "tracking": {
                            "impressionreportingurl": [],
                            "clickreporturl": [],
                            "thirdpartyclickreporturl": [],
                            "thirdpartyimpressionreportingurl": [],
                            "exitdelayreporturl": [],
                            "blankreportingurl": [],
                            "replayreporturl": [],
                            "hoverreporturl": [],
                            "thirdpartyhoverreporturl": [],
                            "expansiontrackingurl": [],
                        },
                        /*
                        "additionalParams": {
                            "buttonAlign": "Left",
                            "buttonText": "Stáhnout aplikaci CANAL+"
                        }
                        */
                    }
                }
            }
        ]
    });

    Ok(Json(response))
}

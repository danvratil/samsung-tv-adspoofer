use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{Body, Bytes};
use axum::extract::{Multipart, Path, State};
use axum::http::{Request, Response, StatusCode};
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{Json, Router};

use rand::distr::Distribution;
use rand::distr::slice::Choose;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use minijinja::{Environment, context};

use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;

use serde_json::{Value, json};
use tracing::{Span, error};

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
        // Frontend for managing the pictures
        .route("/", get(home_handler))
        .route("/images/", post(add_image_handler))
        .route("/images/{image_id}/delete", post(delete_image_handler))
        .route("/images/{image_id}", get(get_image_handler))
        // This is the main route: the SmartHub calls this to get the top banner ad.
        .route("/impressions/tile", get(impressions_title_handler))
        // Since we are redirecting all adgrx.com requests to this server, we want to graciously
        // handle all other paths that the TV seems to be hitting, just so the 404s don't prevent
        // the TV from loading the adverts.
        .route("/impressions/{*path}", get(blackhole_handler))
        .route("/adopportunity", get(blackhole_handler))
        .route("/loads/{*path}", get(blackhole_handler))
        .route("/tracking/{*path}", get(blackhole_handler))
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
        error!("Missing file name in form request");
        return Err(StatusCode::BAD_REQUEST);
    };

    let Some(file_data) = file_data else {
        error!("Missing file data in form request");
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
    state.db.delete_picture(image_id).map_err(|e| {
        error!("Failed to delete picture from database: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Redirect::to("/"))
}

async fn get_image_handler(
    Path(image_id): Path<PictureId>,
    State(state): State<Arc<AppState<'_>>>,
) -> Result<Response<Body>, StatusCode> {
    let path = match state.db.get_picture(image_id) {
        Ok(Some(picture)) => picture.path,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!("Failed to get picture from database: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let file = File::open(&path).await.map_err(|e| {
        error!("Could not open image file at path {:?}: {:?}", path, e);
        StatusCode::NOT_FOUND
    })?;
    let stream = ReaderStream::new(file);

    Response::builder()
        .header("Content-Type", "image/jpeg")
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
    let picture = match state.db.get_random_picture() {
        Ok(Some(picture)) => picture,
        Ok(None) => return Err(StatusCode::NO_CONTENT),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let image_url = format!(
        "http://192.168.0.100:8081/images/{}",
        picture.id.to_string()
    );

    let adid = format!(
        "{}-{}-{}-{}",
        rand_adid(6),
        rand_adid(6),
        rand_adid(6),
        rand_adid(3)
    );

    let guid = format!("{}-1", uuid::Uuid::new_v4().to_string());

    let response = json!({
        "immersive_banner": [
            {
                "ad": {
                    "adinfo": {
                        "adid": adid,
                        "adtype": "6",
                        "guid": guid,
                        "adResponseVersion": "1.0",
                        "responseTime": now.as_secs().to_string(),
                        "expireTime": expire.as_secs().to_string(),
                        "rotationInterval": "60",
                        "refreshInterval": "50"
                    },
                    "creative": {
                        "cid": "1222896",
                        "clicktype": "6",
                        "clickaction": {
                            "appid": "0",
                            "runtype": "GER",
                            "appcontentid": "",
                            "category": "0000008926"
                        },
                        "adtext": picture.name,
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
                        /*
                        "autoplay": {
                            "video": {
                                "vast": "https:\\/\\/rtb.adgrx.com\\/loads\\/6hq-aTIRt5KhZKCWChCldCRd6wBKNKZR2qkOwpOeEgExGAYgVigBMPPDKTj49ydABkiY8gFgAWgHeALAAQHIAYID0gELQURHMTAzNDdTQU3aAQMxMDPlAYOlUW3oAQDwAQCKAhFjb20uc2Ftc3VuZ3R2LmFkc9ICOHJ0Yi1iaWRkZXItcHJvZC1ldXcxLWRzcC1wcm9kLWV1LXdlc3QtMWEtOWQ5ZGZkOTc4LThxdzh62gIoCAMSJGQ3ODVlMmRiLWQ2ODctMzkzNy1hNjk3LTIwYjUxMzhkYzI1OOgC4cGy1waIAwGQAwKYA2eoAwG4A_HOBsAD5g_KAwVOSUtFTOIDCTIyLjYuMzEyOKIEAkNaqgQEdGlsZbIEDDIyX05JS0VMX1VIRLoEE1QtSU5GT0xJTksyMDIyLTEwMjXCBAEg-AQAkAUCmAUAsgUDMjYwugUCODbABQHIBQLaBXNNb3ppbGxhLzUuMCAoU01BUlQtVFY7IExpbnV4OyBUaXplbiA2LjUpIEFwcGxlV2ViS2l0LzUzOC4xIChLSFRNTCwgbGlrZSBHZWNrbykgVmVyc2lvbi82LjUgTmF0aXZlVFZBZHMgU2FmYXJpLzUzOC4x4gURVC1OS0xCREVVQy0xNzIwLjfoBQLxBc3MzMzMzBJAigYNUUU2NVE2MEJBVVhYSA==\\/654328.xml?AG_WP=0.014003&AG_IT=0&AG_VV=3.0&AG_TID=1222896&AG_APP=3202203026693&IMVID=1&tile_pos=1",
                                "videourl": "https://d3188bz1xcsvdh.cloudfront.net/trader/eb185c0f-44c4-45e1-9a75-31b841525d06/transcoded_1920x1080_Samsung_TV_16x9_CZ_1920x1080MP4.mp4"
                            }
                        },
                        */
                        // We can't keep those entirely empty, otherwise the ad doesn't display
                        "tracking": {
                            "impressionreportingurl": "https://adgrx.com/tracking/impressionreportingurl",
                            "clickreporturl": "https://adgrx.com/tracking/clickreporturl",
                            "thirdpartyclickreporturl": [],
                            "thirdpartyimpressionreportingurl": [],
                            "exitdelayreporturl": "https://adgrx.com/tracking/exitdelayreporturl",
                            "blankreportingurl": "https://adgrx.com/tracking/blankreportingurl",
                            "replayreporturl": "https://adgrx.com/tracking/replayreporturl",
                            "hoverreporturl": "https://adgrx.com/tracking/hoverreporturl",
                            "thirdpartyhoverreporturl": [],
                            "expansiontrackingurl": "https://adgrx.com/tracking/expansiontrackingurl",
                        },
                    }
                }
            }
        ]
    });

    Ok(Json(response))
}

async fn blackhole_handler() -> Json<Value> {
    return Json(Value::Object(serde_json::Map::default()));
}

fn rand_adid(len: usize) -> String {
    Choose::new(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"])
        .expect("Valid alphabet")
        .sample_iter(rand::rng())
        .take(len)
        .copied()
        .collect()
}

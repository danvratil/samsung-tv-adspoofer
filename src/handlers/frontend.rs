use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{Multipart, Path, State};
use axum::http::{Response, StatusCode};
use axum::response::Redirect;

use minijinja::context;

use tokio_util::io::ReaderStream;
use tracing::error;

use tokio::fs::File;

use crate::AppState;
use crate::database::PictureId;

pub async fn home_handler(State(state): State<Arc<AppState<'_>>>) -> Result<Body, StatusCode> {
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

pub async fn add_image_handler(
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

    if state
        .pictures
        .add_image(&picture_name, file_data.to_vec())
        .await
        .is_err()
    {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Redirect::to("/"))
}

pub async fn delete_image_handler(
    Path(image_id): Path<PictureId>,
    State(state): State<Arc<AppState<'_>>>,
) -> Result<Redirect, StatusCode> {
    match state.pictures.delete_picture(image_id).await {
        Ok(_) => Ok(Redirect::to("/")),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_image_handler(
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

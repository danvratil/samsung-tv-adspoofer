// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Response, StatusCode};
use axum::response::Redirect;

use axum_typed_multipart::{FieldData, TryFromMultipart, TypedMultipart};
use minijinja::context;

use tokio_util::io::ReaderStream;
use tracing::error;

use tokio::fs::File;

use crate::AppState;
use crate::{Crop, PictureId};

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

#[derive(TryFromMultipart)]
pub struct AddImageForm {
    name: String,
    file: FieldData<tempfile::NamedTempFile>,
    collapsed_crop: Crop,
    fullscreen_crop: Crop,
}

pub async fn add_image_handler(
    State(state): State<Arc<AppState<'_>>>,
    TypedMultipart(form): TypedMultipart<AddImageForm>,
) -> Result<Redirect, StatusCode> {
    let picture = match state
        .pictures
        .add_picture(&form.name, form.file.contents)
        .await
    {
        Ok(picture) => picture,
        Err(err) => {
            error!("Failed to upload picture: {:?}", err);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if let Err(err) = state
        .pictures
        .crop_picture(&picture, form.collapsed_crop, form.fullscreen_crop)
        .await
    {
        error!("Failed to crop uploaded pictures: {:?}", err);
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

pub async fn get_image_kind_handler(
    Path((image_id, kind)): Path<(PictureId, String)>,
    State(state): State<Arc<AppState<'_>>>,
) -> Result<Response<Body>, StatusCode> {
    let path = match state.db.get_picture(image_id) {
        Ok(Some(picture)) => PathBuf::from(picture.path),
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!("Failed to get picture from database: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let final_path = path
        .parent()
        .expect("Path must have a parent")
        .join("crop")
        .join(format!(
            "{}_{}",
            path.file_name()
                .expect("path must have a filename")
                .to_str()
                .expect("Filename must be valid string"),
            kind
        ));

    let file = File::open(&final_path).await.map_err(|e| {
        error!(
            "Could not open image file at path {:?}: {:?}",
            final_path, e
        );
        StatusCode::NOT_FOUND
    })?;
    let stream = ReaderStream::new(file);

    let mime_type = if kind == "background" {
        "image/jpeg"
    } else {
        "video/mp4"
    };

    Response::builder()
        .header("Content-Type", mime_type)
        .body(Body::from_stream(stream))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

use std::io;
use std::sync::Weak;
use std::{fs::File, path::Path};

use tokio::fs;

use tempfile::NamedTempFile;
use thiserror::Error;
use tracing::error;

use crate::{AppState, Crop, CroppedPicture, Picture, PictureId, database::DatabaseError};

mod postprocessing;

#[derive(Debug, Error)]
pub enum PictureError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("IO Error: {0}")]
    IO(#[from] io::Error),

    #[error("Failed to post-process image: {0}")]
    PostProcessing(#[from] postprocessing::PostProcessingError),
}

pub struct Pictures<'a> {
    state: Weak<AppState<'a>>,
}

impl<'a> Pictures<'a> {
    pub fn new(state: Weak<AppState<'_>>) -> Pictures<'_> {
        Pictures { state }
    }

    pub async fn add_picture(
        &self,
        name: &str,
        content: NamedTempFile<File>,
    ) -> Result<Picture, PictureError> {
        let Some(state) = self.state.upgrade() else {
            return state_unavailable_error();
        };

        let file_name = uuid::Uuid::new_v4().to_string();
        let path = state.config.storage.image_storage_path.join(file_name);

        tokio::fs::copy(content.path(), &path).await?;

        Ok(state
            .db
            .add_picture(name, path.to_str().unwrap())
            .inspect_err(|err| {
                error!("Failed to add picture to database: {:?}", err);
            })?)
    }

    pub async fn get_random_picture(&self) -> Result<Option<Picture>, PictureError> {
        let Some(state) = self.state.upgrade() else {
            return state_unavailable_error();
        };
        Ok(state.db.get_random_picture()?)
    }

    pub async fn delete_picture(&self, picture_id: PictureId) -> Result<(), PictureError> {
        let Some(state) = self.state.upgrade() else {
            return state_unavailable_error();
        };
        let Some(picture) = state.db.get_picture(picture_id)? else {
            return Ok(());
        };

        state.db.delete_picture(picture_id)?;

        fs::remove_file(picture.path).await?;

        Ok(())
    }

    pub async fn crop_picture(
        &self,
        picture: &Picture,
        title_crop: Crop,
        fullscreen_crop: Crop,
    ) -> Result<CroppedPicture, PictureError> {
        let Some(state) = self.state.upgrade() else {
            return state_unavailable_error();
        };

        let orig_path = Path::new(&picture.path);
        let Some(orig_filename) = orig_path.file_stem().and_then(|stem| stem.to_str()) else {
            return Err(PictureError::IO(io::Error::new(
                io::ErrorKind::InvalidFilename,
                "Storaged picture filename is not valid",
            )));
        };

        let crop_dir = state.config.storage.image_storage_path.join("crop");
        fs::create_dir_all(&crop_dir).await?;
        let title_path = crop_dir.join(format!("{}_background", orig_filename));
        let video_path = crop_dir.join(format!("{}_video", orig_filename));

        postprocessing::process_picture(
            orig_path,
            title_crop,
            &title_path,
            fullscreen_crop,
            &video_path,
        )
        .await
        .inspect_err(|err| error!("Failed to post-process picture: {:?}", err))?;

        Ok(CroppedPicture {
            title_background: title_path,
            fullscreen_video: video_path,
        })
    }
}

fn state_unavailable_error<T>() -> Result<T, PictureError> {
    Err(PictureError::IO(io::Error::new(
        io::ErrorKind::InvalidData,
        "Application state unavailable",
    )))
}

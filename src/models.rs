// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

use std::path::PathBuf;

use anyhow::anyhow;
use axum::body::Bytes;
use axum_typed_multipart::{FieldMetadata, TryFromChunks, TypedMultipartError};
use futures_util::Stream;
use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Crop {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub source_width: usize,
    pub source_height: usize,
}

#[async_trait::async_trait]
impl TryFromChunks for Crop {
    async fn try_from_chunks(
        chunks: impl Stream<Item = Result<Bytes, TypedMultipartError>> + Send + Sync + Unpin,
        metadata: FieldMetadata,
    ) -> Result<Self, TypedMultipartError> {
        let body = String::try_from_chunks(chunks, metadata).await?;
        serde_json::from_str(&body).map_err(|err| TypedMultipartError::Other {
            source: anyhow!("Failed to deserialize field value as Crop: {:?}", err),
        })
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct PictureId(i64);

impl From<i64> for PictureId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<PictureId> for i64 {
    fn from(val: PictureId) -> Self {
        val.0
    }
}

impl std::fmt::Display for PictureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Picture {
    pub id: PictureId,
    pub path: String,
    pub name: String,
}

impl TryFrom<sqlite::Row> for Picture {
    type Error = sqlite::Error;

    fn try_from(value: sqlite::Row) -> Result<Self, Self::Error> {
        Ok(Self {
            id: PictureId(value.read::<i64, _>("id")),
            path: value.read::<&str, _>("path").to_string(),
            name: value.read::<&str, _>("name").to_string(),
        })
    }
}

impl TryFrom<sqlite::Statement<'_>> for Picture {
    type Error = sqlite::Error;

    fn try_from(value: sqlite::Statement<'_>) -> Result<Self, Self::Error> {
        Ok(Self {
            id: PictureId(value.read("id")?),
            path: value.read("path")?,
            name: value.read("name")?,
        })
    }
}

pub struct CroppedPicture {
    pub title_background: PathBuf,
    pub fullscreen_video: PathBuf
}

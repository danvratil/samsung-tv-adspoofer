use std::sync::Weak;

use thiserror::Error;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tracing::error;

use crate::{AppState, database::{DatabaseError, Picture, PictureId}};

#[derive(Debug, Error)]
pub enum PictureError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("IO Error: {0}")]
    IO(#[from] std::io::Error),
}

pub struct Pictures<'a> {
    state: Weak<AppState<'a>>,
}

impl<'a> Pictures<'a> {
    pub fn new(state: Weak<AppState<'_>>) -> Pictures<'_> {
        return Pictures { state };
    }

    pub async fn add_image(&self, name: &str, data: Vec<u8>) -> Result<(), PictureError> {
        let file_name = uuid::Uuid::new_v4().to_string();
        let Some(state) = self.state.upgrade() else { return Ok(()); };
        let path = state.config.storage.image_storage_path.join(file_name);

        {
            let mut file = File::create_new(&path).await.inspect_err(|err| {
                error!("Failed to create file {:?}: {:?}", path, err);
            })?;
            file.write(&data).await.inspect_err(|err| {
                error!("Failed to write data into file {:?}: {:?}", path, err);
            })?;
        }

        state
            .db
            .add_picture(name, path.to_str().unwrap())
            .inspect_err(|err| {
                error!("Failed to add picture to database: {:?}", err);
            })?;

        Ok(())
    }

    pub async fn get_random_picture(&self) -> Result<Option<Picture>, PictureError> {
        let Some(state) = self.state.upgrade() else { return Ok(None); };
        Ok(state.db.get_random_picture()?)
    }

    pub async fn delete_picture(&self, picture_id: PictureId) -> Result<(), PictureError> {
        let Some(state) = self.state.upgrade() else { return Ok(()); };
        let Some(picture) = state.db.get_picture(picture_id)? else {
            return Ok(())
        };

        state.db.delete_picture(picture_id)?;

        fs::remove_file(picture.path).await?;


        Ok(())
    }
}

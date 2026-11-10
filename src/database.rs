use std::path::Path;
use thiserror::Error;
use tracing::error;

use sqlite::{Connection, ConnectionThreadSafe, State};

use crate::{Picture, PictureId};

pub struct Database {
    conn: ConnectionThreadSafe,
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite error: {0}")]
    SqliteError(#[from] sqlite::Error),
    #[error("Insert failed")]
    InsertFailed,
}

impl Database {
    pub fn new(db_path: &Path) -> Result<Self, DatabaseError> {
        Ok(Self {
            conn: Connection::open_thread_safe(db_path)?,
        })
    }

    pub fn init_database(&self) -> Result<(), DatabaseError> {
        self.conn.execute(
            "
            CREATE TABLE IF NOT EXISTS pictures (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE
            );
            ",
        )?;

        Ok(())
    }

    pub fn add_picture(&self, name: &str, path: &str) -> Result<Picture, DatabaseError> {
        let mut stmt = self
            .conn
            .prepare("INSERT INTO pictures (name, path) VALUES (:name, :path) RETURNING id")?;
        stmt.bind::<&[(_, &str)]>(&[(":name", name), (":path", path)])?;
        match stmt.next()? {
            State::Done => Err(DatabaseError::InsertFailed),
            State::Row => Ok(Picture {
                id: stmt.read::<i64, _>(0)?.into(),
                name: name.to_string(),
                path: path.to_string(),
            }),
        }
    }

    pub fn delete_picture(&self, id: PictureId) -> Result<(), DatabaseError> {
        let mut stmt = self.conn.prepare("DELETE FROM pictures WHERE id = :id")?;
        stmt.bind::<(_, i64)>((":id", id.into()))?;
        stmt.next()?;

        Ok(())
    }

    pub fn get_picture(&self, picture_id: PictureId) -> Result<Option<Picture>, DatabaseError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, path FROM pictures WHERE id = :id")?;
        stmt.bind::<(_, i64)>((":id", picture_id.into()))?;
        match stmt.next()? {
            State::Done => Ok(None),
            State::Row => Ok(Some(Picture {
                id: stmt.read::<i64, _>(0)?.into(),
                name: stmt.read(1)?,
                path: stmt.read(2)?,
            })),
        }
    }

    pub fn get_pictures(&self) -> Result<Vec<Picture>, DatabaseError> {
        Ok(self
            .conn
            .prepare("SELECT id, name, path FROM pictures ORDER BY id ASC")?
            .into_iter()
            .map(|row| match row {
                Ok(row) => Picture::try_from(row),
                Err(err) => {
                    error!("Failed to read row from table: {:?}", err);
                    Err(err)
                }
            })
            .collect::<Result<_, _>>()?)
    }

    pub fn get_random_picture(&self) -> Result<Option<Picture>, DatabaseError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, path FROM pictures ORDER BY RANDOM() LIMIT 1")?;
        match stmt.next()? {
            State::Done => Ok(None),
            State::Row => Ok(Some(
                stmt.try_into()
                    .inspect_err(|err| error!("Invalid picture: {:?}", err))?,
            )),
        }
    }
}

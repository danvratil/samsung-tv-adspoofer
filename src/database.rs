use serde::{Serialize, Deserialize};
use std::path::Path;
use thiserror::Error;
use tracing::error;

use sqlite::{Connection, ConnectionThreadSafe, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct PictureId(i64);

impl ToString for PictureId {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Picture {
    pub id: PictureId,
    pub path: String,
    pub name: String,
}

impl Picture {
    pub fn from_row(row: sqlite::Row) -> Result<Picture, sqlite::Error> {
        Ok(Picture {
            id: PictureId(row.read::<i64, _>("id")),
            path: row.read::<&str, _>("path").to_string(),
            name: row.read::<&str, _>("name").to_string(),
        })
    }

    pub fn from_statement(stmt: sqlite::Statement<'_>) -> Result<Picture, sqlite::Error> {
        Ok(Picture {
            id: PictureId(stmt.read("id")?),
            path: stmt.read("path")?,
            name: stmt.read("name")?,
        })
    }
}

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
                id: PictureId(stmt.read(0)?),
                name: name.to_string(),
                path: path.to_string(),
            }),
        }
    }

    pub fn delete_picture(&self, id: PictureId) -> Result<(), DatabaseError> {
        let mut stmt = self.conn.prepare("DELETE FROM pictures WHERE id = :id")?;
        stmt.bind::<(_, i64)>((":id", id.0))?;
        stmt.next()?;

        Ok(())
    }

    pub fn get_picture(&self, picture_id: PictureId) -> Result<Option<Picture>, DatabaseError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, path FROM pictures WHERE id = :id")?;
        stmt.bind::<(_, i64)>((":id", picture_id.0))?;
        match stmt.next()? {
            State::Done => Ok(None),
            State::Row => Ok(Some(Picture {
                id: PictureId(stmt.read(0)?),
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
            .filter_map(|row| row.map(Picture::from_row).map_err(|e| { error!("Invalid picture: {:?}", e); e }).ok())
            .collect::<Result<_, _>>()?)
    }

    pub fn get_random_picture(&self) -> Result<Option<Picture>, DatabaseError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, path FROM pictures ORDER BY RANDOM() LIMIT 1")?;
        match stmt.next()? {
            State::Done => Ok(None),
            State::Row => Ok(Some(Picture::from_statement(stmt).map_err(|e| { error!("Invalid picture: {:?}", e); e })?)),
        }
    }
}

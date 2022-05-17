use std::ffi::OsString;

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use actix_files::NamedFile;
use actix_multipart::Multipart;
use actix_web::web::{Data, Json};
use actix_web::{web, Either, Error, HttpResponse};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tempfile::NamedTempFile;

pub(crate) type Id = String;

#[derive(Deserialize)]
pub struct Image {
    id: Id,
}

pub async fn fetch_raw(
    data: Data<Arc<SQLiteDatabase>>,
    params: web::Query<Image>,
) -> Result<Either<NamedFile, HttpResponse>, Error> {
    let image = params.into_inner();
    match data.get_path(&image.id).await {
        Ok(path) => Ok(Either::Left(NamedFile::open_async(path).await?)),
        Err(_) => Ok(Either::Right(
            HttpResponse::NotFound().body("image with id not found"),
        )),
    }
}

#[derive(Serialize)]
pub struct UploadRawResponse {
    ids: Vec<String>,
}

// TODO: File size limits
// TODO: Auth with file size limits
// TODO: Want to report exif information for use elsewhere
pub async fn upload_raw(
    data: Data<Arc<SQLiteDatabase>>,
    payload: Multipart,
) -> Either<HttpResponse, Json<UploadRawResponse>> {
    match files::save_payload(payload).await {
        Ok(files) => {
            // TODO: time between read and use error
            match data.store_images(files).await {
                Ok(Some(ids)) => Either::Right(Json(UploadRawResponse { ids })),
                _ => Either::Left(
                    HttpResponse::InternalServerError()
                        .content_type("text/plain")
                        .body("upload failed"),
                ),
            }
        }
        _ => Either::Left(
            HttpResponse::BadRequest()
                .content_type("text/plain")
                .body("upload failed"),
        ),
    }
}

pub mod files {
    use std::io::Write;

    use actix_multipart::Multipart;
    use actix_web::Error;
    use futures::{StreamExt, TryStreamExt};

    use tempfile::NamedTempFile;

    pub async fn save_payload(
        mut payload: Multipart,
    ) -> Result<Vec<(NamedTempFile, String)>, Error> {
        // iterate over multipart stream
        let mut files = vec![];
        while let Some(mut field) = payload.try_next().await? {
            let mut file = NamedTempFile::new()?;

            // Field in turn is stream of *Bytes* object
            while let Some(chunk) = field.next().await {
                file.write_all(&chunk?)?;
            }
            files.push((file, field.name().to_string()));
        }

        Ok(files)
    }
}

pub struct SQLiteDatabase {
    connection: SqlitePool,
    image_upload_dir: PathBuf,
    path: PathBuf,
}

impl SQLiteDatabase {
    async fn store_images(
        &self,
        files: Vec<(NamedTempFile, String)>,
    ) -> Result<Option<Vec<Id>>, sqlx::Error> {
        use std::os::unix::ffi::OsStrExt;

        let mut tx = self.connection.begin().await?;

        let mut filenames = vec![];
        let remove_persisted = |filenames: Vec<(Id, PathBuf)>| {
            filenames
                .into_iter()
                .for_each(|(_, p)| drop(std::fs::remove_file(p)))
        };

        for (file, name) in files.into_iter() {
            // TODO: Handle collisions (very important, can't risk overlap)
            let id = uuid::Uuid::new_v4().to_string();
            let name = match name.rsplit_once(".") {
                None => continue,
                // TODO: replace with correct mounted dir
                Some((_, ext)) => {
                    let mut root = std::path::PathBuf::from("./images");
                    root.push(format!("{}.{}", id, ext));
                    root
                }
            };

            match file.persist(&name) {
                Ok(_) => {}
                Err(_) => {
                    remove_persisted(filenames);
                    return Ok(None);
                }
            }

            let name_bytes = name.as_os_str().as_bytes();
            match sqlx::query!("INSERT INTO files (id, path) VALUES(?, ?);", id, name_bytes)
                .execute(&mut tx)
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    remove_persisted(filenames);
                    return Err(e);
                }
            }

            filenames.push((id, name));
        }

        match tx.commit().await {
            Ok(_) => Ok(Some(filenames.into_iter().map(|(id, _)| id).collect())),
            Err(e) => {
                remove_persisted(filenames);
                Err(e)
            }
        }
    }

    async fn num_rows(&self) -> Result<u32, sqlx::Error> {
        struct Count {
            count: i32,
        }
        sqlx::query_as!(Count, "SELECT COUNT(id) as count FROM files")
            .fetch_one(&self.connection)
            .await
            .map(|x| x.count as u32)
    }

    pub(crate) async fn get_path(&self, id: &str) -> Result<PathBuf, sqlx::Error> {
        use std::os::unix::ffi::OsStringExt;
        struct SqlxPath {
            path: Vec<u8>,
        }
        sqlx::query_as!(SqlxPath, "SELECT path FROM files WHERE id = ?", id)
            .fetch_one(&self.connection)
            .await
            .map(|x| OsString::from_vec(x.path).into())
    }

    pub(crate) async fn open<P>(file_path: P, image_upload_dir: PathBuf) -> Result<Self, ()>
    where
        P: AsRef<std::path::Path> + Send + Sync,
        Self: Sized,
    {
        let db_exists = file_path.as_ref().exists();
        if !db_exists {
            if let Some(path) = file_path.as_ref().parent() {
                std::fs::create_dir_all(path).map_err(|_| ())?;
            }
        }
        let database = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&file_path)
                    .create_if_missing(true),
            )
            .await
            .map_err(|_| ())?;

        let db = Self {
            connection: database,
            path: file_path.as_ref().to_path_buf(),
            image_upload_dir,
        };

        for query in [
            "CREATE TABLE IF NOT EXISTS `files` (`id` TEXT NOT NULL UNIQUE, `path` BLOB NOT NULL);",
            "CREATE INDEX IF NOT EXISTS file_ids ON files(id)",
        ] {
            sqlx::query(query)
                .execute(&db.connection)
                .await
                .map_err(|_| ())?;
        }

        Ok(db)
    }
}

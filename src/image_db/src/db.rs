use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroU32;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;

use image::codecs::jpeg::JpegEncoder;
use image::ImageEncoder;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use actix_files::NamedFile;
use actix_multipart::Multipart;
use actix_web::web::{Data, Json};
use actix_web::{web, Either, Error, HttpResponse};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::images::resize;

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
        Ok(path) => {
            println!("Successfully serving image with id {}", image.id);
            Ok(Either::Left(NamedFile::open_async(path).await?))
        }
        Err(_) => Ok(Either::Right(
            HttpResponse::NotFound().body(format!("image with id {} not found", image.id)),
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

fn generate_preview(file: &mut std::fs::File) -> String {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_end(&mut bytes).unwrap();
    let image = resize(
        &bytes,
        NonZeroU32::try_from(600).unwrap(),
        NonZeroU32::try_from(400).unwrap(),
    )
    .unwrap();

    let mut buf = Vec::new();

    JpegEncoder::new_with_quality(&mut buf, 70)
        .write_image(
            image.buffer(),
            u32::from(image.width()),
            u32::from(image.height()),
            image::ColorType::Rgb8,
        )
        .unwrap();

    base64::encode(buf)
}

impl SQLiteDatabase {
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

    async fn add_entries(&self, entries: &[(Id, PathBuf)]) -> sqlx::Result<()> {
        let mut tx = self.connection.begin().await?;

        for (id, path) in entries.iter() {
            let path_bytes = path.as_os_str().as_bytes();
            sqlx::query!("INSERT INTO files (id, path) VALUES(?, ?);", id, path_bytes)
                .execute(&mut tx)
                .await?;
        }

        tx.commit().await
    }

    async fn store_images(
        &self,
        files: Vec<(NamedTempFile, String)>,
    ) -> Result<Option<Vec<Id>>, sqlx::Error> {
        #[derive(Serialize)]
        struct WeaviateInput {
            class: String,
            properties: HashMap<String, String>,
            id: Id,
        }

        #[derive(Serialize)]
        struct WeaviateBatchInput {
            objects: Vec<WeaviateInput>,
        }

        let client = reqwest::Client::new();
        let mut image_files = vec![];
        let mut entries = vec![];
        for (file, name) in files.into_iter() {
            // TODO: Handle collisions (very important, can't risk overlap)
            let id = uuid::Uuid::new_v4().to_string();
            let name = match name.rsplit_once(".") {
                None => continue,
                Some((_, ext)) => {
                    let mut root = self.image_upload_dir.clone();
                    root.push(format!("{}.{}", id, ext));
                    root
                }
            };
            let file = match file.persist(&name) {
                Ok(file) => file,
                Err(_) => {
                    drop(image_files);
                    entries
                        .into_iter()
                        .for_each(|(_, file_name)| drop(std::fs::remove_file(file_name)));
                    return Ok(None);
                }
            };
            entries.push((id, name));
            image_files.push(file);
        }

        match self.add_entries(&entries).await {
            Ok(_) => {}
            Err(e) => {
                drop(image_files);
                entries
                    .into_iter()
                    .for_each(|(_, file_name)| drop(std::fs::remove_file(file_name)));
                return Err(e);
            }
        }

        let previews: Vec<_> = image_files.par_iter_mut().map(generate_preview).collect();

        let objects = entries
            .iter()
            .zip(previews.into_iter())
            .map(|((id, _), preview)| {
                let mut properties = HashMap::new();

                properties.insert("image".to_string(), preview);

                WeaviateInput {
                    class: "ClipImage".to_string(),
                    properties,
                    id: id.clone(),
                }
            })
            .collect();

        client
            .post("http://weaviate:8080/v1/batch/objects")
            .json(&WeaviateBatchInput { objects })
            .send()
            .await
            .unwrap();

        Ok(Some(entries.into_iter().map(|(id, _)| id).collect()))
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
}

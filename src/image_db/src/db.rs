use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroU32;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;

use image::codecs::jpeg::JpegEncoder;
use image::ImageEncoder;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use actix_files::NamedFile;
use actix_multipart::Multipart;
use actix_web::web::{Data, Json};
use actix_web::{web, Either, Error, HttpResponse};
use reqwest::Client;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tokio::task;

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

async fn insert_image(
    id: String,
    name: PathBuf,
    mut file: NamedTempFile,
    pool: SqlitePool,
    client: Client,
) -> sqlx::Result<Option<(String, PathBuf)>> {
    #[derive(Serialize)]
    struct WeaviateInput {
        class: String,
        properties: HashMap<String, String>,
    }

    let input = {
        let mut properties = HashMap::new();
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

        properties.insert("image".to_string(), base64::encode(buf));

        WeaviateInput {
            class: "ClipImage".to_string(),
            properties,
        }
    };

    match file.persist(&name) {
        Ok(_) => {}
        Err(_) => {
            return Ok(None);
        }
    }

    client
        .post("http://weaviate:8080/v1/objects")
        .json(&input)
        .send()
        .await
        .unwrap();

    let name_bytes = name.as_os_str().as_bytes();
    match sqlx::query!("INSERT INTO files (id, path) VALUES(?, ?);", id, name_bytes)
        .execute(&pool)
        .await
    {
        Ok(_) => Ok(Some((id, name))),
        Err(e) => Err(e),
    }
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

    async fn store_images(
        &self,
        files: Vec<(NamedTempFile, String)>,
    ) -> Result<Option<Vec<Id>>, sqlx::Error> {
        let client = reqwest::Client::new();
        let mut tasks = vec![];
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

            tasks.push(task::spawn(insert_image(
                id,
                name,
                file,
                self.connection.clone(),
                client.clone(),
            )))
        }

        let results: Vec<Result<Result<Option<(String, PathBuf)>, _>, _>> =
            futures::future::join_all(tasks).await;
        let target_len = results.len();
        let successful: Vec<(Id, PathBuf)> = results
            .into_iter()
            .filter_map(Result::ok)
            .filter_map(Result::ok)
            .filter_map(|x| x)
            .collect();
        if successful.len() != target_len {
            successful
                .into_iter()
                .for_each(|(_, p)| drop(std::fs::remove_file(p)));

            Ok(None)
        } else {
            Ok(Some(successful.into_iter().map(|(id, _)| id).collect()))
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
}

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
use crate::weaviate_graphql::{
    WeaviateBatchDelete, WeaviateBatchInput, WeaviateInput, WeaviateMatch,
};
use crate::{MultiOperator, Operator, WeaviateWhere, WhereValue};

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
    client: reqwest::Client,
}

fn generate_preview(file: &mut std::fs::File) -> Option<String> {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_to_end(&mut bytes).ok()?;
    let image = resize(
        &bytes,
        NonZeroU32::try_from(600).unwrap(),
        NonZeroU32::try_from(400).unwrap(),
    )?;

    let mut buf = Vec::new();

    JpegEncoder::new_with_quality(&mut buf, 70)
        .write_image(
            image.buffer(),
            u32::from(image.width()),
            u32::from(image.height()),
            image::ColorType::Rgb8,
        )
        .ok()?;

    Some(base64::encode(buf))
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
            client: reqwest::Client::new(),
        };

        for query in [
            "CREATE TABLE IF NOT EXISTS `files` (`id` TEXT NOT NULL UNIQUE, `path` BLOB NOT NULL);",
            "CREATE INDEX IF NOT EXISTS file_ids ON files(id);",
            // TODO: This should be replaced with a file hash
            "CREATE INDEX IF NOT EXISTS paths ON files(path);",
        ] {
            sqlx::query(query)
                .execute(&db.connection)
                .await
                .map_err(|_| ())?;
        }

        Ok(db)
    }

    /// Deletes all items with
    pub(crate) async fn remove_paths(&self, paths: &[PathBuf]) -> sqlx::Result<()> {
        struct SqlxId {
            id: Id,
        }

        let mut tx = self.connection.begin().await?;

        let mut ids = vec![];
        for path in paths.iter() {
            let path_bytes = path.as_os_str().as_bytes();
            let id = sqlx::query_as!(SqlxId, "SELECT id FROM files WHERE path = ?", path_bytes)
                .fetch_optional(&mut tx)
                .await?;
            if let Some(id) = id {
                ids.push(id.id);
                sqlx::query!("DELETE FROM files WHERE path = ?", path_bytes)
                    .execute(&mut tx)
                    .await?;
            }
        }

        self.client
            .delete("http://weaviate:8080/v1/batch/objects")
            .json(&WeaviateBatchDelete::new(WeaviateMatch {
                class: "ClipImage".to_string(),
                where_: WeaviateWhere::Multiple {
                    operator: MultiOperator::Or,
                    operands: ids
                        .into_iter()
                        .map(|id| WeaviateWhere::Single {
                            path: vec!["id".to_string()],
                            operator: Operator::Equal,
                            value: WhereValue::String(id),
                        })
                        .collect(),
                },
            }))
            .send()
            .await
            .unwrap();

        tx.commit().await
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
    ) -> sqlx::Result<Option<Vec<Id>>> {
        let mut image_files = vec![];
        let mut entries = vec![];
        let num_files = files.len();
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

        // TODO: Return (successful file & uuid) and unsuccessful files for more detailed user feedback
        //  and for partial progress (because uploads are time-consuming)
        match self.add_files(entries.clone(), image_files).await {
            Ok(Some(ids)) if ids.len() == num_files => {
                Ok(Some(ids))
            },
            x @ Ok(None) | x @ Err(_) => {
                entries
                    .into_iter()
                    .for_each(|(_, file_name)| drop(std::fs::remove_file(file_name)));
                return x;
            }
            _ => {
                unimplemented!("")
            }
        }
    }

    pub(crate) async fn add_paths(
        &self,
        paths: &[PathBuf],
    ) -> sqlx::Result<Option<Vec<Id>>> {
        let mut image_files = vec![];
        let mut entries = vec![];
        for path in paths.iter() {
            // TODO: Handle collisions (very important, can't risk overlap)
            let id = uuid::Uuid::new_v4().to_string();

            let file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(_) => continue,
            };

            entries.push((id, path.clone()));
            image_files.push(file);
        }

        self.add_files(entries, image_files).await
    }

    async fn add_files(&self, entries: Vec<(String, PathBuf)>, mut image_files: Vec<std::fs::File>) -> sqlx::Result<Option<Vec<Id>>> {
        let start = std::time::Instant::now();
        let previews: Vec<Option<String>> = image_files.par_iter_mut().map(generate_preview).collect();
        let entries: Vec<_> = entries
            .into_iter()
            .zip(previews.iter())
            .flat_map(|(entry, preview)| preview.as_ref().map(|_| entry))
            .collect();

        println!("Generated {} previews in {}s", entries.len(), start.elapsed().as_secs_f32());
        self.add_entries(&entries).await?;

        let objects = entries
            .iter()
            .zip(previews.into_iter().flatten())
            .map(|((id, _), preview)| {
                WeaviateInput::class("ClipImage".to_string())
                    .id(id.clone())
                    .property("image".to_string(), preview)
            })
            .collect();

        self.client
            .post("http://weaviate:8080/v1/batch/objects")
            .json(&WeaviateBatchInput::new(objects))
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

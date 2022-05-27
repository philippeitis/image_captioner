#![deny(unused_must_use)]
#![deny(unused_imports)]
#![deny(unused_attributes)]
#![deny(unused_mut)]

mod db;
mod fs;
mod images;
mod weaviate_graphql;

use actix_cors::Cors;
use std::ffi::OsString;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use crate::db::{fetch_raw, upload_raw, SQLiteDatabase};
use crate::images::{fetch_jpg, fetch_png};
use crate::weaviate_graphql::{MultiOperator, Operator, WeaviateWhere, WhereValue};
use actix_web::{get, web, App, HttpResponse, HttpServer};

// TODO: GET /supported_ext: get supported file formats

#[get("/health")]
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().body("success")
}

async fn mount_images(
    database: Arc<SQLiteDatabase>,
    mount_dir: OsString,
    data_dir: OsString,
) -> std::io::Result<()> {
    if !mount_dir.is_empty() {
        let fs_fingerprint_path = {
            let mut fs_fingerprint_path = PathBuf::from(&data_dir);
            fs_fingerprint_path.push("fs_fingerprint.txt");
            fs_fingerprint_path
        };

        let before = if let Ok(fs_fingerprint) = std::fs::read_to_string(&fs_fingerprint_path) {
            ron::from_str(&fs_fingerprint).unwrap_or_default()
        } else {
            fs::FileSystem::default()
        };
        let after = fs::FileSystem::deep_scan(&mount_dir).unwrap();

        let diff = before.diff(&after, PathBuf::from(&mount_dir).parent().unwrap());

        database.remove_paths(&diff.removed).await.unwrap();

        for chunk in diff.added.chunks(100) {
            database.add_paths(chunk).await.unwrap();
        }
        println!(
            "Removed {} images, added {} images.",
            diff.removed.len(),
            diff.added.len()
        );

        let _ = std::fs::create_dir_all(&fs_fingerprint_path.parent().unwrap());
        std::fs::write(fs_fingerprint_path, ron::to_string(&after).unwrap())?;
    }

    println!("All images mounted.");
    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _ = dotenvy::dotenv();
    let address = std::env::var("IMAGE_DB_ADDR").unwrap_or(String::from("127.0.0.1:8081"));
    let db_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");
    let data_dir = std::env::var_os("DATA_DIR").expect("Missing DATA_DIR");
    let mount_dir = std::env::var_os("MOUNTED_IMAGE_DIR").unwrap_or_else(|| OsString::from(""));
    let upload_dir = std::env::var_os("UPLOAD_DIR").expect("Missing UPLOAD_DIR");

    for dir in [&data_dir, &mount_dir, &upload_dir] {
        let _ = std::fs::create_dir_all(dir);
    }

    let data = web::Data::new(Arc::new(
        SQLiteDatabase::open(db_url, upload_dir.into())
            .await
            .expect("Opening database failed"),
    ));

    println!("Database opened.");
    tokio::task::spawn(mount_images(
        data.deref().deref().clone(),
        mount_dir.clone(),
        data_dir.clone(),
    ));
    println!("Opening application on {}", address);
    HttpServer::new(move || {
        App::new()
            .wrap(Cors::permissive().expose_headers(["Content-Disposition"]))
            .service(health)
            .service(
                web::resource("/upload_raw")
                    .app_data(data.clone())
                    .route(web::post().to(upload_raw)),
            )
            .service(
                web::resource("/fetch_jpg")
                    .app_data(data.clone())
                    .route(web::get().to(fetch_jpg)),
            )
            .service(
                web::resource("/fetch_png")
                    .app_data(data.clone())
                    .route(web::get().to(fetch_png)),
            )
            .service(
                web::resource("/fetch_raw")
                    .app_data(data.clone())
                    .route(web::get().to(fetch_raw)),
            )
    })
    .bind(address)?
    .run()
    .await
}

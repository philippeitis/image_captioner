#![deny(unused_must_use)]
#![deny(unused_imports)]
#![deny(unused_attributes)]
#![deny(unused_mut)]

mod db;
mod images;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

// Need to provide the following:
// GET /supported_ext: get supported file formats
// POST /upload_image: upload image to db
// GET /fetch_raw_image: get raw image
// GET /fetch_image_png: get png format, with dims
// GET /fetch_image_jpg: get jpg format, with dims and compression level
// SQLite backed DB

use crate::db::{fetch_raw, upload_raw, SQLiteDatabase};
use crate::images::{fetch_jpg, fetch_png};
use actix_web::{web, App, HttpServer};

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    let uri = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL.");
    let port = u16::from_str(&std::env::var("IMAGE_DB_PORT").expect("Missing DB port")).expect("DB not u16");
    let data = web::Data::new(Arc::new(
        SQLiteDatabase::open(uri, PathBuf::from("./images"))
            .await
            .expect("Opening database failed"),
    ));
    println!("Opening application on 127.0.0.8:{}", port);
    HttpServer::new(move || {
        App::new()
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
    .bind(("127.0.0.8", port))?
    .run()
    .await
}

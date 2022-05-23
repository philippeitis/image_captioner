#![deny(unused_must_use)]
#![deny(unused_imports)]
#![deny(unused_attributes)]
#![deny(unused_mut)]

mod db;
mod images;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use actix_cors::Cors;

use crate::db::{fetch_raw, upload_raw, SQLiteDatabase};
use crate::images::{fetch_jpg, fetch_png};
use actix_web::{get, web, App, HttpResponse, HttpServer};

// TODO: GET /supported_ext: get supported file formats

#[get("/health")]
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().body("success")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let uri = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL.");
    let ip = std::env::var("IMAGE_DB_IP").unwrap_or(String::from("127.0.0.1"));
    let port = u16::from_str(&std::env::var("IMAGE_DB_PORT").expect("Missing DB port"))
        .expect("DB not u16");
    let data = web::Data::new(Arc::new(
        SQLiteDatabase::open(uri, PathBuf::from("./images"))
            .await
            .expect("Opening database failed"),
    ));
    println!("Opening application on {}:{}", ip, port);
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
    .bind((ip, port))?
    .run()
    .await
}

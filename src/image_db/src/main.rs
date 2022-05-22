#![deny(unused_must_use)]
#![deny(unused_imports)]
#![deny(unused_attributes)]
#![deny(unused_mut)]

mod db;
mod images;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

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
    {
        std::thread::sleep(std::time::Duration::from_millis(15000));
        let mut dst = Vec::new();
        let mut easy = curl::easy::Easy::new();
        easy.url("http://weaviate:8080/v1/.well-known/openid-configuration").unwrap();
        easy.get(true).unwrap();
        {
            let mut transfer = easy.transfer();
            transfer.write_function(|data| {
                dst.extend_from_slice(data);
                Ok(data.len())
            }).unwrap();
            transfer.perform().unwrap();
        }
        println!("{}", String::from_utf8(dst.clone()).unwrap());
    }

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

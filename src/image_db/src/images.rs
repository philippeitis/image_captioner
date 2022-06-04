use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;

use serde::Deserialize;

use actix_web::web::Data;
use actix_web::{web, HttpResponse};
use libraw::{Processor, ThumbnailFormat};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::ImageEncoder;

use fast_image_resize as fr;

use crate::db::Id;
use crate::SQLiteDatabase;

#[derive(Deserialize)]
pub struct ImageResize {
    id: Id,
    height: NonZeroU32,
    width: NonZeroU32,
}

#[derive(Deserialize)]
pub struct ImageRequestJpg {
    id: Id,
    height: NonZeroU32,
    width: NonZeroU32,
    quality: u8,
}

// TODO: more granular errors, restriction on image dimensions
// TODO: Use image preview to reduce computation time
async fn fetch_and_resize<'a>(
    data: Data<Arc<SQLiteDatabase>>,
    params: ImageResize,
) -> Option<fast_image_resize::Image<'a>> {
    let path = data.get_path(&params.id).await.ok()?;
    let buf = std::fs::read(path).expect("read in");
    resize(&buf, params.width, params.height)
}

pub fn preview<'b>(buf: &[u8]) -> Option<Vec<u8>> {
    let processor = Processor::new();
    match processor.thumbnail(buf) {
        Ok(thumbnail) if thumbnail.format() == ThumbnailFormat::Jpeg => {
            Some(thumbnail.deref().to_vec())
        }
        thumbnail => {
            if let Ok(thumbnail) = thumbnail {
                if thumbnail.format() != ThumbnailFormat::Unknown {
                    println!(
                        "Image had unsupported thumbnail format: {:?}",
                        thumbnail.format()
                    );
                }
            }
            let image = resize(
                buf,
                NonZeroU32::try_from(1200).unwrap(),
                NonZeroU32::try_from(800).unwrap(),
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
            Some(buf)
        }
    }
}

pub fn resize<'a>(
    buf: &[u8],
    width: NonZeroU32,
    height: NonZeroU32,
) -> Option<fast_image_resize::Image<'a>> {
    let processor = Processor::new();
    let decoded = processor.process_8bit(buf).ok()?;

    let src_image = fr::Image::from_vec_u8(
        NonZeroU32::try_from(decoded.width()).expect("zero width"),
        NonZeroU32::try_from(decoded.height()).expect("zero height"),
        decoded.deref().to_vec(),
        fr::PixelType::U8x3,
    )
    .ok()?;

    let mut dst_image = fast_image_resize::Image::new(width, height, src_image.pixel_type());

    // Get mutable view of destination image data
    let mut dst_view = dst_image.view_mut();

    // Create Resizer instance and resize source image
    // into buffer of destination image
    let mut resizer = fr::Resizer::new(fr::ResizeAlg::Convolution(fr::FilterType::CatmullRom));
    resizer.resize(&src_image.view(), &mut dst_view).ok()?;
    Some(dst_image)
}

pub async fn fetch_png(
    data: Data<Arc<SQLiteDatabase>>,
    params: web::Query<ImageResize>,
) -> HttpResponse {
    match fetch_and_resize(data, params.into_inner()).await {
        None => HttpResponse::NotFound().body("image with id not found"),
        Some(resized_im) => {
            let mut buf = Vec::new();
            PngEncoder::new(&mut buf)
                .write_image(
                    resized_im.buffer(),
                    u32::from(resized_im.width()),
                    u32::from(resized_im.height()),
                    image::ColorType::Rgb8,
                )
                .unwrap();
            HttpResponse::Ok().content_type("image/png").body(buf)
        }
    }
}

pub async fn fetch_jpg(
    data: Data<Arc<SQLiteDatabase>>,
    params: web::Query<ImageRequestJpg>,
) -> HttpResponse {
    let params = params.into_inner();
    match fetch_and_resize(
        data,
        ImageResize {
            id: params.id,
            width: params.width,
            height: params.height,
        },
    )
    .await
    {
        None => HttpResponse::NotFound().body("image with id not found"),
        Some(resized_im) => {
            let mut buf = Vec::new();
            JpegEncoder::new_with_quality(&mut buf, params.quality)
                .write_image(
                    resized_im.buffer(),
                    u32::from(resized_im.width()),
                    u32::from(resized_im.height()),
                    image::ColorType::Rgb8,
                )
                .unwrap();
            HttpResponse::Ok().content_type("image/jpeg").body(buf)
        }
    }
}

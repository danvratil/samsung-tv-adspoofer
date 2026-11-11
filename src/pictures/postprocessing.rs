// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

use std::io;
use std::path::Path;

use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, RgbImage};
use thiserror::Error;
use tokio::task::JoinError;
use tracing::{Level, event, span};

use crate::Crop;

// Maximum dimensions of the generated title (home preview) picture.
const TITLE_MAX_WIDTH: u32 = 1920;
const TITLE_MAX_HEIGHT: u32 = 1080;


#[derive(Debug, Error)]
pub enum PostProcessingError {
    #[error("image error: {0}")]
    Image(#[from] ImageError),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("background task failed: {0}")]
    Task(#[from] JoinError),
}

pub async fn process_picture(
    src: &Path,
    title_crop: Crop,
    title_dst: &Path,
) -> Result<(), PostProcessingError> {
    let src = src.to_path_buf();
    let title_dst = title_dst.to_path_buf();

    let span = span!(Level::DEBUG, "process_picture");
    tokio::task::spawn_blocking(move || -> Result<(), PostProcessingError> {
        let _ = span.enter();
        let image = load_oriented_image(&src)?;

        event!(Level::DEBUG, "Rendering title picture");
        render_title_picture(&image, &title_crop)
            .save_with_format(&title_dst, ImageFormat::Jpeg)?;

        Ok(())
    })
    .await??;

    Ok(())
}

/// Axis-aligned rectangle in pixel coordinates. The origin may be negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: i64,
    y: i64,
    width: u32,
    height: u32,
}

impl Rect {
    fn new(x: i64, y: i64, width: u32, height: u32) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Converts a fractional crop into pixel coordinates of an image of the given size.
    fn from_crop(crop: &Crop, image_width: u32, image_height: u32) -> Self {
        let (w, h) = (f64::from(image_width), f64::from(image_height));
        Rect {
            x: (crop.x * w).round() as i64,
            y: (crop.y * h).round() as i64,
            width: (crop.width * w).round().max(1.0) as u32,
            height: (crop.height * h).round().max(1.0) as u32,
        }
    }

    fn right(&self) -> i64 {
        self.x + i64::from(self.width)
    }

    fn bottom(&self) -> i64 {
        self.y + i64::from(self.height)
    }

    fn intersection(&self, other: &Rect) -> Option<Rect> {
        let (left, top) = (self.x.max(other.x), self.y.max(other.y));
        let (right, bottom) = (
            self.right().min(other.right()),
            self.bottom().min(other.bottom()),
        );
        (right > left && bottom > top)
            .then(|| Rect::new(left, top, (right - left) as u32, (bottom - top) as u32))
    }
}

fn load_oriented_image(src: &Path) -> Result<RgbImage, ImageError> {
    let mut decoder = ImageReader::open(src)?
        .with_guessed_format()?
        .into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image.into_rgb8())
}

/// Renders `region` of `image` scaled to `out_width`x`out_height`. Parts of the region
/// outside of the image are black.
fn render_region(image: &RgbImage, region: Rect, out_width: u32, out_height: u32) -> RgbImage {
    let mut canvas = RgbImage::new(out_width, out_height);
    let bounds = Rect::new(0, 0, image.width(), image.height());
    let Some(visible) = region.intersection(&bounds) else {
        return canvas;
    };

    let scale_x = f64::from(out_width) / f64::from(region.width);
    let scale_y = f64::from(out_height) / f64::from(region.height);
    let dst_x = ((visible.x - region.x) as f64 * scale_x).round() as i64;
    let dst_y = ((visible.y - region.y) as f64 * scale_y).round() as i64;
    let dst_width = (f64::from(visible.width) * scale_x).round().max(1.0) as u32;
    let dst_height = (f64::from(visible.height) * scale_y).round().max(1.0) as u32;

    let part = imageops::crop_imm(
        image,
        visible.x as u32,
        visible.y as u32,
        visible.width,
        visible.height,
    );
    if (dst_width, dst_height) == (visible.width, visible.height) {
        imageops::replace(&mut canvas, &*part, dst_x, dst_y);
    } else {
        let scaled = imageops::resize(&*part, dst_width, dst_height, FilterType::CatmullRom);
        imageops::replace(&mut canvas, &scaled, dst_x, dst_y);
    }
    canvas
}

/// Scales `width`x`height` down  to fit within `max_width`x`max_height`.
fn fit_within(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let scale = (f64::from(max_width) / f64::from(width))
        .min(f64::from(max_height) / f64::from(height))
        .min(1.0);
    (
        (f64::from(width) * scale).round().max(1.0) as u32,
        (f64::from(height) * scale).round().max(1.0) as u32,
    )
}

fn render_title_picture(image: &RgbImage, crop: &Crop) -> RgbImage {
    let region = Rect::from_crop(crop, image.width(), image.height());
    let (width, height) = fit_within(
        region.width,
        region.height,
        TITLE_MAX_WIDTH,
        TITLE_MAX_HEIGHT,
    );
    render_region(image, region, width, height)
}


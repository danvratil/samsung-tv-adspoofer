// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, RgbImage};
use thiserror::Error;
use tokio::process::Command;
use tokio::task::JoinError;
use tracing::{Level, event, span};

use crate::Crop;

// Maximum dimensions of the generated title (home preview) picture.
const TITLE_MAX_WIDTH: u32 = 1920;
const TITLE_MAX_HEIGHT: u32 = 1080;

// Output video parameters.
const VIDEO_WIDTH: u32 = 1920;
const VIDEO_HEIGHT: u32 = 1080;
const VIDEO_FPS: u32 = 30;
const TRANSITION_FRAMES: u32 = 60;
const TOTAL_FRAMES: u32 = 150;

#[derive(Debug, Error)]
pub enum PostProcessingError {
    #[error("image error: {0}")]
    Image(#[from] ImageError),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("background task failed: {0}")]
    Task(#[from] JoinError),

    #[error("ffmpeg failed ({status}): {stderr}")]
    Ffmpeg { status: ExitStatus, stderr: String },
}

pub async fn process_picture(
    src: &Path,
    title_crop: Crop,
    title_dst: &Path,
    fullscreen_crop: Crop,
    video_dst: &Path,
) -> Result<(), PostProcessingError> {
    let src = src.to_path_buf();
    let title_dst = title_dst.to_path_buf();
    let frame_dir = video_dst
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);

    let span = span!(Level::DEBUG, "process_picture");
    let (frame_file, target) =
        tokio::task::spawn_blocking(move || -> Result<_, PostProcessingError> {
            let _ = span.enter();
            let image = load_oriented_image(&src)?;

            event!(Level::DEBUG, "Rendering title picture");
            render_title_picture(&image, &title_crop)
                .save_with_format(&title_dst, ImageFormat::Jpeg)?;

            let transition = prepare_transition(&image, &fullscreen_crop);
            let frame_file = tempfile::Builder::new()
                .suffix(".png")
                .tempfile_in(&frame_dir)?;
            event!(Level::DEBUG, "Writing picture");
            transition
                .frame
                .save_with_format(frame_file.path(), ImageFormat::Png)?;
            Ok((frame_file, transition.target))
        })
        .await??;

    // `frame_file` is kept alive until ffmpeg is done with it.
    render_transition_video(frame_file.path(), target, video_dst).await
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

    fn union(&self, other: &Rect) -> Rect {
        let (left, top) = (self.x.min(other.x), self.y.min(other.y));
        let (right, bottom) = (
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        );
        Rect::new(left, top, (right - left) as u32, (bottom - top) as u32)
    }

    /// Grows the rectangle symmetrically along one axis to the `aspect_w:aspect_h` ratio.
    fn expand_to_aspect(&self, aspect_w: u32, aspect_h: u32) -> Rect {
        let (w, h) = (u64::from(self.width), u64::from(self.height));
        let (aw, ah) = (u64::from(aspect_w), u64::from(aspect_h));
        let (new_w, new_h) = if w * ah >= h * aw {
            (w, (w * ah).div_ceil(aw))
        } else {
            ((h * aw).div_ceil(ah), h)
        };
        Rect::new(
            self.x - ((new_w - w) / 2) as i64,
            self.y - ((new_h - h) / 2) as i64,
            new_w as u32,
            new_h as u32,
        )
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

struct Transition {
    /// The starting frame of the video: the whole image, letterboxed to 16:9.
    frame: RgbImage,
    /// The final framing (the fullscreen crop), relative to `frame`.
    target: Rect,
}

fn prepare_transition(image: &RgbImage, crop: &Crop) -> Transition {
    let image_rect = Rect::new(0, 0, image.width(), image.height());
    let crop_rect = Rect::from_crop(crop, image.width(), image.height());
    let frame_rect = image_rect
        .union(&crop_rect)
        .expand_to_aspect(VIDEO_WIDTH, VIDEO_HEIGHT);

    Transition {
        frame: render_region(image, frame_rect, frame_rect.width, frame_rect.height),
        target: Rect::new(
            crop_rect.x - frame_rect.x,
            crop_rect.y - frame_rect.y,
            crop_rect.width,
            crop_rect.height,
        ),
    }
}

fn transition_filter(frame_width: u32, target: Rect) -> String {
    // Cosine ease-in-out progress from 0 to 1 over the transition, then held at 1.
    let t = format!("(0.5-0.5*cos(PI*min(on/{},1)))", TRANSITION_FRAMES - 1);
    // zoompan shows a window of iw/zoom pixels; interpolate the window width linearly.
    let (fw, tw) = (frame_width, target.width);
    format!(
        "zoompan=z='{fw}/({fw}+({tw}-{fw})*{t})':x='{x}*{t}':y='{y}*{t}':d=1:s={VIDEO_WIDTH}x{VIDEO_HEIGHT}:fps={VIDEO_FPS},format=yuv420p",
        x = target.x,
        y = target.y,
    )
}

async fn render_transition_video(
    frame: &Path,
    target: Rect,
    dst: &Path,
) -> Result<(), PostProcessingError> {
    let _ = span!(Level::DEBUG, "render_transition_video");

    let (frame_width, _) = image::image_dimensions(frame)?;
    let dst: PathBuf = dst.to_path_buf();

    event!(Level::DEBUG, "Running ffmpeg");
    let output = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error"])
        .args(["-loop", "1", "-framerate", &VIDEO_FPS.to_string()])
        .arg("-i")
        .arg(frame)
        .args(["-vf", &transition_filter(frame_width, target)])
        .args(["-frames:v", &TOTAL_FRAMES.to_string()])
        .args(["-c:v", "libx264", "-crf", "20", "-preset", "medium"])
        .args(["-movflags", "+faststart", "-an", "-f", "mp4"])
        .arg(&dst)
        .output()
        .await?;

    if !output.status.success() {
        event!(Level::WARN, "ffmpeg finished with error");
        return Err(PostProcessingError::Ffmpeg {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    event!(Level::DEBUG, "ffmpeg finished successfully");
    Ok(())
}

// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

use std::io;
use std::path::Path;
use std::process::Output;

use image::{ImageError, ImageFormat, ImageReader};
use tokio::process::Command;
use thiserror::Error;

use crate::Crop;

pub async fn crop_title_picture(src: &Path, dst: &Path, crop: Crop) -> Result<(), ImageError> {
    let image = ImageReader::open(src)?.decode()?;
    let cropped = image.crop_imm(
        crop.x.try_into().unwrap_or(0),
        crop.y.try_into().unwrap_or(0),
        crop.width.try_into().unwrap_or(image.width()),
        crop.height.try_into().unwrap_or(image.height()),
    );

    cropped.save_with_format(dst, ImageFormat::Jpeg)
}

#[derive(Debug, Error)]
pub enum FfmpegError {
    #[error("command error: {0}")]
    Command(#[from] io::Error),

    #[error("ffmpeg error")]
    Ffmpeg(Output)
}

pub async fn render_transition_video(src: &Path, dst: &Path, crop: Crop) -> Result<(), FfmpegError> {
    // ffmpeg -loop 1 -framerate 30 -i WhatsApp\ Image\ 2026-09-27\ at\ 16.55.23.jpeg \
    //      -vf "pad=3004:2048:699:0:black,zoompan=z='1+0.960784*(0.5-0.5*cos(PI*min(on/59,1)))':x='699*(0.5-0.5*cos(PI*min(on/59,1)))':y=0:d=1:s=1920x1080:fps=30,format=yuv420p" \
    //      -frames:v 150 -c:v libx264 -crf 20 -preset medium \
    //      -movflags +faststart -an output.mp4

    let command = Command::new("ffmpeg")
        .args([
            "-loop", "1",
            "-framerate", "30",
            "-i", src.to_str().expect("Failed to convert path to utf-8"),
            "-vf", &format!("pad={x}:{y}:{width}:{height}:black,zoompan=z='1+0.960784*(0.5-0.5*cos(PI*min(on/59,1)))':x='{width}*(0.5-0.5*cos(PI*/min(on/59,1)))':y=0:d=1:s=1920x1080:fps=30,format=yuv420p",
                x=crop.x, y=crop.y, width=crop.width, height=crop.height
            ),
            "-frames:v", "150",
            "-c:v", "libx264",
            "-crf", "20",
            "-preset", "medium",
            "-movflags",
            "+faststart",
            "-an", dst.to_str().expect("Failed to convert path to utf-8")
        ])
        .output()
        .await?;

    if command.status.success() {
        return Err(FfmpegError::Ffmpeg(command));
    }

    Ok(())
}

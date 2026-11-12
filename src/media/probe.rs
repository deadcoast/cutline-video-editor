use super::process;
use crate::model::FrameRate;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Media {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub rate: FrameRate,
    pub variable_rate: bool,
    pub codec: String,
    pub audio_tracks: usize,
    pub hdr: bool,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub color_space: Option<String>,
    pub color_range: Option<String>,
    pub hdr_metadata: bool,
    pub video_index: u32,
}

impl Media {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
    pub fn dimensions(&self) -> [u32; 2] {
        [self.width, self.height]
    }
    pub fn snap(&self, seconds: f64) -> f64 {
        if self.variable_rate {
            seconds
        } else {
            self.rate.snap(seconds)
        }
        .clamp(0.0, self.duration)
    }
}

pub fn inspect(path: &Path) -> Result<Media> {
    if !path.is_file() {
        bail!("Select a local video file.");
    }
    let path = path.canonicalize().context("Cannot read this file")?;
    let mut command = process::command(&process::tool("ffprobe")?);
    command
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(&path);
    let bytes = process::capture(command, Duration::from_secs(30), 4 * 1024 * 1024)?;
    parse(&bytes, path)
}

pub fn parse(bytes: &[u8], path: PathBuf) -> Result<Media> {
    let data: Value = serde_json::from_slice(bytes).context("Unreadable video metadata")?;
    let streams = data["streams"]
        .as_array()
        .context("No streams in this file")?;
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .context("This file has no playable video stream")?;
    let dimension = |key: &str| {
        video[key]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v >= 2)
    };
    let mut width = dimension("width").context("Missing video width")?;
    let mut height = dimension("height").context("Missing video height")?;
    if width > 32768 || height > 32768 {
        bail!("Video dimensions exceed 32768 pixels.");
    }
    let sar = video["sample_aspect_ratio"]
        .as_str()
        .and_then(|s| {
            let (n, d) = s.split_once(':')?;
            let n = n.parse::<f64>().ok()?;
            let d = d.parse::<f64>().ok()?;
            (n > 0.0 && d > 0.0).then_some(n / d)
        })
        .unwrap_or(1.0);
    width = ((width as f64 * sar / 2.0).round() as u32 * 2).max(2);
    let side = video["side_data_list"].as_array();
    let rotation = side
        .and_then(|a| a.iter().find_map(|s| s["rotation"].as_f64()))
        .or_else(|| {
            video["tags"]["rotate"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0.0);
    if ((rotation.round() as i64).rem_euclid(180) - 90).abs() < 1 {
        std::mem::swap(&mut width, &mut height);
    }
    width = width / 2 * 2;
    height = height / 2 * 2;
    if width > 32768 {
        bail!("Display width exceeds 32768 pixels.");
    }
    let seconds = |value: &Value| {
        value
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| value.as_f64())
    };
    let duration = seconds(&video["duration"])
        .or_else(|| seconds(&data["format"]["duration"]))
        .filter(|v| v.is_finite() && *v > 0.0)
        .context("This file has no finite video duration")?;
    let average = video["avg_frame_rate"].as_str().and_then(FrameRate::parse);
    let nominal = video["r_frame_rate"].as_str().and_then(FrameRate::parse);
    let rate = average
        .or(nominal)
        .context("This file does not report a usable video frame rate")?;
    let variable_rate = nominal.is_some_and(|n| (n.fps() - rate.fps()).abs() > 0.02);
    let text = |key: &str| {
        video[key]
            .as_str()
            .filter(|v| !matches!(*v, "unknown" | "unspecified" | "reserved"))
            .map(str::to_owned)
    };
    let color_transfer = text("color_transfer");
    let hdr = matches!(
        color_transfer.as_deref(),
        Some("smpte2084" | "arib-std-b67")
    );
    let hdr_metadata = side.is_some_and(|a| {
        a.iter().any(|s| {
            let kind = s["side_data_type"].as_str().unwrap_or("");
            kind.contains("DOVI") || kind.contains("HDR Dynamic")
        })
    });
    Ok(Media {
        path,
        width,
        height,
        duration,
        rate,
        variable_rate,
        codec: text("codec_name").unwrap_or_else(|| "video".into()),
        audio_tracks: streams
            .iter()
            .filter(|s| s["codec_type"] == "audio")
            .count(),
        hdr,
        color_primaries: text("color_primaries"),
        color_transfer,
        color_space: text("color_space"),
        color_range: text("color_range"),
        hdr_metadata,
        video_index: video["index"].as_u64().unwrap_or(0) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotated_phone_dimensions_match_display_coordinates() {
        let json = br#"{"streams":[{"index":0,"codec_type":"video","width":3840,"height":2160,"avg_frame_rate":"60000/1001","r_frame_rate":"60000/1001","duration":"12.34","side_data_list":[{"rotation":-90}]}]}"#;
        let media = parse(json, "phone.mov".into()).unwrap();
        assert_eq!(media.dimensions(), [2160, 3840]);
        assert!(!media.variable_rate);
    }
    #[test]
    fn cover_art_is_not_the_video() {
        let json = br#"{"streams":[{"codec_type":"video","disposition":{"attached_pic":1}},{"index":2,"codec_type":"video","width":1920,"height":1080,"duration":"2","avg_frame_rate":"120/1"}]}"#;
        assert_eq!(parse(json, "a.mkv".into()).unwrap().video_index, 2);
    }
}

use super::{
    probe::{self, Media},
    process,
};
use crate::model::Edit;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Codec {
    H264,
    Hevc,
    Vp9,
}
impl Codec {
    pub fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264 · widely compatible",
            Self::Hevc => "HEVC · smaller / HDR",
            Self::Vp9 => "VP9 · WebM",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoder {
    Auto,
    Cpu,
    Nvidia,
    Intel,
    Amd,
}
impl Encoder {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Cpu => "CPU · consistent quality",
            Self::Nvidia => "NVIDIA",
            Self::Intel => "Intel Quick Sync",
            Self::Amd => "AMD",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    Mp4,
    Mkv,
    Mov,
    Webm,
}
impl Container {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::Mov => "mov",
            Self::Webm => "webm",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Settings {
    pub codec: Codec,
    pub encoder: Encoder,
    pub container: Container,
    pub quality: u8,
    pub mute: bool,
}
impl Settings {
    pub fn for_media(media: &Media) -> Self {
        Self {
            codec: if media.hdr { Codec::Hevc } else { Codec::H264 },
            encoder: Encoder::Auto,
            container: Container::Mp4,
            quality: 20,
            mute: false,
        }
    }
}
#[derive(Debug)]
pub enum Event {
    Status(String),
    Progress { fraction: f32, speed: String },
    Complete(PathBuf),
    Failed(String),
    Cancelled,
}
pub struct Job {
    pub events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}
impl Job {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn encoder_name(codec: Codec, encoder: Encoder) -> &'static str {
    match (codec, encoder) {
        (Codec::Vp9, _) => "libvpx-vp9",
        (Codec::H264, Encoder::Nvidia) => "h264_nvenc",
        (Codec::Hevc, Encoder::Nvidia) => "hevc_nvenc",
        (Codec::H264, Encoder::Intel) => "h264_qsv",
        (Codec::Hevc, Encoder::Intel) => "hevc_qsv",
        (Codec::H264, Encoder::Amd) => "h264_amf",
        (Codec::Hevc, Encoder::Amd) => "hevc_amf",
        (Codec::H264, _) => "libx264",
        (Codec::Hevc, _) => "libx265",
    }
}

pub fn arguments(
    media: &Media,
    edit: &Edit,
    settings: &Settings,
    encoder: Encoder,
    output: &Path,
) -> Result<Vec<OsString>> {
    if !edit.valid(media.duration, media.width, media.height) {
        bail!("The trim or crop is outside this video.");
    }
    if settings.container == Container::Webm && settings.codec != Codec::Vp9 {
        bail!("WebM requires VP9.");
    }
    if settings.codec == Codec::Vp9
        && !matches!(settings.container, Container::Webm | Container::Mkv)
    {
        bail!("Choose WebM or MKV for VP9.");
    }
    if media.hdr && settings.codec == Codec::H264 {
        bail!("Choose HEVC or VP9 to retain HDR and 10-bit color.");
    }
    if media.hdr_metadata {
        bail!(
            "This clip has dynamic HDR metadata. Export is disabled to prevent silently discarding it. Use an HDR mastering tool for this file."
        );
    }
    let mut args: Vec<OsString> = Vec::new();
    macro_rules! add { ($($s:expr),+ $(,)?) => { $(args.push(OsString::from($s));)+ }; }
    add!(
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-ss",
        format!("{:.9}", edit.trim.start),
        "-i"
    );
    args.push(media.path.as_os_str().to_owned());
    add!(
        "-t",
        format!("{:.9}", edit.trim.duration()),
        "-map",
        format!("0:{}", media.video_index)
    );
    if !settings.mute {
        add!("-map", "0:a?");
    }
    let c = edit.crop;
    let filter = format!(
        "scale={}:{}:flags=lanczos,setsar=1,crop={}:{}:{}:{}",
        media.width, media.height, c.width, c.height, c.x, c.y
    );
    add!(
        "-vf",
        filter,
        "-fps_mode",
        "passthrough",
        "-c:v",
        encoder_name(settings.codec, encoder)
    );
    let q = settings.quality.clamp(14, 30).to_string();
    if settings.codec == Codec::Vp9 {
        add!("-crf", &q, "-b:v", "0", "-row-mt", "1", "-cpu-used", "2");
    } else {
        match encoder {
            Encoder::Nvidia => {
                add!("-preset", "p5", "-rc", "vbr", "-cq", &q, "-b:v", "0");
            }
            Encoder::Intel => {
                add!("-global_quality", &q, "-preset", "medium");
            }
            Encoder::Amd => {
                add!(
                    "-rc", "cqp", "-qp_i", &q, "-qp_p", &q, "-quality", "quality"
                );
            }
            _ => {
                add!("-preset", "medium", "-crf", &q);
            }
        }
    }
    add!(
        "-pix_fmt",
        if media.hdr {
            if matches!(encoder, Encoder::Nvidia | Encoder::Intel | Encoder::Amd) {
                "p010le"
            } else {
                "yuv420p10le"
            }
        } else {
            "yuv420p"
        }
    );
    if settings.codec == Codec::Hevc
        && matches!(settings.container, Container::Mp4 | Container::Mov)
    {
        add!("-tag:v", "hvc1");
    }
    for (key, value) in [
        ("-color_primaries", &media.color_primaries),
        ("-color_trc", &media.color_transfer),
        ("-colorspace", &media.color_space),
        ("-color_range", &media.color_range),
    ] {
        if let Some(value) = value {
            add!(key, value);
        }
    }
    if !settings.mute {
        add!(
            "-c:a",
            if settings.container == Container::Webm {
                "libopus"
            } else {
                "aac"
            },
            "-b:a",
            "256k"
        );
    }
    add!(
        "-map_metadata",
        "0",
        "-map_chapters",
        "-1",
        "-metadata:s:v:0",
        "rotate=0"
    );
    if matches!(settings.container, Container::Mp4 | Container::Mov) {
        add!("-movflags", "+faststart");
    }
    add!("-progress", "pipe:1", "-nostats");
    args.push(output.as_os_str().to_owned());
    Ok(args)
}

pub fn start(
    media: Media,
    edit: Edit,
    settings: Settings,
    output: PathBuf,
    repaint: impl Fn() + Send + 'static,
) -> Job {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    std::thread::spawn(move || {
        let send = |event| {
            let _ = tx.send(event);
            repaint();
        };
        let result = run(&media, &edit, &settings, &output, &worker_cancel, &send);
        match result {
            Ok(()) => send(Event::Complete(output)),
            Err(_) if worker_cancel.load(Ordering::Acquire) => send(Event::Cancelled),
            Err(error) => send(Event::Failed(format!("{error:#}"))),
        }
    });
    Job { events: rx, cancel }
}

fn run(
    media: &Media,
    edit: &Edit,
    settings: &Settings,
    output: &Path,
    cancel: &AtomicBool,
    send: &impl Fn(Event),
) -> Result<()> {
    if output.exists() {
        bail!(
            "That file already exists. Choose a new name; Cutline never replaces an existing video."
        );
    }
    if output
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        != Some(settings.container.extension())
    {
        bail!(
            "Use a .{} filename for the selected format.",
            settings.container.extension()
        );
    }
    let parent = output.parent().context("Choose an export folder")?;
    let temp = tempfile::Builder::new()
        .prefix(".cutline-")
        .suffix(&format!(".{}", settings.container.extension()))
        .tempfile_in(parent)
        .context("Cannot write in the export folder")?;
    let ffmpeg = process::tool("ffmpeg")?;
    let choices: Vec<Encoder> = if settings.encoder == Encoder::Auto && settings.codec != Codec::Vp9
    {
        vec![Encoder::Nvidia, Encoder::Intel, Encoder::Amd, Encoder::Cpu]
    } else {
        vec![settings.encoder]
    };
    let mut last_error = None;
    for encoder in choices {
        if cancel.load(Ordering::Acquire) {
            bail!("Export cancelled");
        }
        send(Event::Status(format!(
            "Exporting with {}",
            encoder_name(settings.codec, encoder)
        )));
        let args = arguments(media, edit, settings, encoder, temp.path())?;
        match encode(&ffmpeg, &args, edit.trim.duration(), cancel, send) {
            Ok(()) => {
                last_error = None;
                break;
            }
            Err(error) => {
                last_error = Some(error);
                if settings.encoder == Encoder::Auto {
                    send(Event::Status(
                        "Encoder unavailable; trying the next device…".into(),
                    ));
                }
            }
        }
    }
    if let Some(error) = last_error {
        return Err(error);
    }
    if cancel.load(Ordering::Acquire) {
        bail!("Export cancelled");
    }
    send(Event::Status("Verifying the finished video…".into()));
    let verified = probe::inspect(temp.path()).context("Export did not pass media verification")?;
    if verified.dimensions() != [edit.crop.width, edit.crop.height] {
        bail!("Output dimensions did not match the crop. The incomplete file was removed.");
    }
    let tolerance = media.rate.frame_seconds() * 2.0 + 0.05;
    if (verified.duration - edit.trim.duration()).abs() > tolerance {
        bail!("Output duration did not match the trim. The incomplete file was removed.");
    }
    if !media.variable_rate && (verified.rate.fps() - media.rate.fps()).abs() > 0.02 {
        bail!("Output frame rate changed unexpectedly. The incomplete file was removed.");
    }
    if !settings.mute && verified.audio_tracks != media.audio_tracks {
        bail!("Not all audio tracks survived export. The incomplete file was removed.");
    }
    if media.hdr && (!verified.hdr || verified.color_transfer != media.color_transfer) {
        bail!("HDR color tags did not survive export. The incomplete file was removed.");
    }
    if cancel.load(Ordering::Acquire) {
        bail!("Export cancelled");
    }
    temp.as_file().sync_all()?;
    temp.persist_noclobber(output)
        .map_err(|e| e.error)
        .context("Could not save the finished video; the destination may now exist")?;
    Ok(())
}

fn encode(
    program: &Path,
    args: &[OsString],
    duration: f64,
    cancel: &AtomicBool,
    send: &impl Fn(Event),
) -> Result<()> {
    let child = process::command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not start the video encoder")?;
    let mut child = process::ManagedChild(child);
    let stdout = child.stdout.take().context("Missing progress pipe")?;
    let mut stderr = child.stderr.take().context("Missing encoder diagnostics")?;
    let (tx, rx) = mpsc::channel();
    let progress = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let diagnostics = std::thread::spawn(move || {
        let mut tail = Vec::new();
        let mut chunk = [0u8; 4096];
        while let Ok(n) = stderr.read(&mut chunk) {
            if n == 0 {
                break;
            }
            tail.extend_from_slice(&chunk[..n]);
            if tail.len() > 32768 {
                tail.drain(..tail.len() - 32768);
            }
        }
        tail
    });
    let mut fraction = 0.0;
    let mut speed = String::new();
    let mut last_report = Instant::now();
    let status = loop {
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            break child.wait()?;
        }
        for line in rx.try_iter() {
            if let Some(value) = line
                .strip_prefix("out_time_us=")
                .and_then(|v| v.parse::<f64>().ok())
            {
                fraction = (value / 1_000_000.0 / duration).clamp(0.0, 0.995) as f32;
            }
            if let Some(value) = line.strip_prefix("speed=") {
                speed = value.trim().into();
            }
        }
        if last_report.elapsed() > Duration::from_millis(100) {
            send(Event::Progress {
                fraction,
                speed: speed.clone(),
            });
            last_report = Instant::now();
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    let _ = progress.join();
    let error = diagnostics.join().unwrap_or_default();
    if cancel.load(Ordering::Acquire) {
        bail!("Export cancelled");
    }
    if !status.success() {
        bail!("Encoder failed: {}", String::from_utf8_lossy(&error).trim());
    }
    Ok(())
}

pub fn suggested_path(media: &Media, container: Container) -> PathBuf {
    let stem = media.path.file_stem().unwrap_or_default().to_string_lossy();
    let parent = media.path.parent().unwrap_or(Path::new("."));
    for number in 1.. {
        let suffix = if number == 1 {
            String::new()
        } else {
            format!("-{number}")
        };
        let path = parent.join(format!("{stem}-cut{suffix}.{}", container.extension()));
        if !path.exists() {
            return path;
        }
    }
    unreachable!()
}

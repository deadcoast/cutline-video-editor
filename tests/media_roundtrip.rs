use cutline::{
    media::{
        export::{self, Codec, Container, Encoder, Event, Settings},
        probe,
    },
    model::{Crop, Edit},
};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

fn fixture(path: &Path, size: &str, rate: &str, seconds: &str, audio: bool) {
    let mut command = Command::new("ffmpeg");
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc2=size={size}:rate={rate}:duration={seconds}"),
    ]);
    if audio {
        command.args([
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=440:sample_rate=48000:duration={seconds}"),
        ]);
    }
    command
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "15",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ])
        .arg(path);
    assert!(command.status().unwrap().success());
}
fn finish(job: export::Job) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(120) {
        if let Ok(event) = job.events.recv_timeout(Duration::from_secs(1)) {
            match event {
                Event::Complete(_) => return Ok(()),
                Event::Failed(error) => return Err(error),
                Event::Cancelled => return Err("cancelled".into()),
                _ => {}
            }
        }
    }
    panic!("export did not complete within two minutes");
}
fn count_frames(path: &Path) -> usize {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn decoded_gray_frame(path: &Path, index: usize, crop: Option<Crop>) -> Vec<u8> {
    let mut filter = format!("select=eq(n\\,{index})");
    if let Some(c) = crop {
        filter.push_str(&format!(",crop={}:{}:{}:{}", c.width, c.height, c.x, c.y));
    }
    filter.push_str(",format=gray");
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-vf", &filter, "-frames:v", "1", "-f", "rawvideo", "pipe:1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    output.stdout
}

#[test]
fn high_frame_rate_trim_crop_and_audio_survive_real_encoding() {
    for rate in ["60", "120000/1001"] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source with spaces.mp4");
        fixture(&input, "640x360", rate, "2", true);
        let media = probe::inspect(&input).unwrap();
        let mut edit = Edit::new(media.duration, media.width, media.height);
        edit.trim.start = media.rate.frame_seconds() * 17.0;
        edit.trim.end = media.rate.frame_seconds() * 77.0;
        edit.crop = Crop {
            x: 80,
            y: 40,
            width: 480,
            height: 270,
        };
        let output = dir.path().join("cropped.mp4");
        let mut settings = Settings::for_media(&media);
        settings.encoder = Encoder::Cpu;
        finish(export::start(
            media.clone(),
            edit.clone(),
            settings,
            output.clone(),
            || {},
        ))
        .unwrap();
        let actual = probe::inspect(&output).unwrap();
        assert_eq!(actual.dimensions(), [480, 270]);
        assert_eq!(count_frames(&output), 60);
        assert_eq!(actual.audio_tracks, 1);
        assert!((actual.rate.fps() - media.rate.fps()).abs() < 0.001);
        // Decode the content, not only its headers: the first exported image must
        // be frame 17, not the preceding keyframe or either adjacent frame.
        let first = decoded_gray_frame(&output, 0, None);
        let error = |index| {
            let source = decoded_gray_frame(&input, index, Some(edit.crop));
            assert_eq!(first.len(), source.len());
            first
                .iter()
                .zip(source)
                .map(|(a, b)| a.abs_diff(b) as u64)
                .sum::<u64>()
        };
        let exact = error(17);
        assert!(
            exact < error(16) && exact < error(18),
            "trim selected the wrong source frame"
        );
        assert!(input.is_file());
        assert!(!dir.path().read_dir().unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".cutline-")
        }));
    }
}

#[test]
fn full_4k_sixty_fps_is_never_resized_or_capped() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("4k.mp4");
    fixture(&input, "3840x2160", "60", "0.5", false);
    let media = probe::inspect(&input).unwrap();
    let mut edit = Edit::new(media.duration, media.width, media.height);
    edit.trim.start = 0.1;
    edit.trim.end = 0.4;
    let mut settings = Settings::for_media(&media);
    settings.encoder = Encoder::Cpu;
    let output = dir.path().join("4k-cut.mp4");
    finish(export::start(media, edit, settings, output.clone(), || {})).unwrap();
    let actual = probe::inspect(&output).unwrap();
    assert_eq!(actual.dimensions(), [3840, 2160]);
    assert_eq!(count_frames(&output), 18);
    assert_eq!(actual.rate.fps(), 60.0);
}

#[test]
fn existing_outputs_are_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.mp4");
    fixture(&input, "320x180", "60", "1", false);
    let media = probe::inspect(&input).unwrap();
    let edit = Edit::new(media.duration, media.width, media.height);
    let settings = Settings::for_media(&media);
    let output = dir.path().join("existing.mp4");
    std::fs::write(&output, b"existing data").unwrap();
    assert!(
        finish(export::start(media, edit, settings, output.clone(), || {}))
            .unwrap_err()
            .contains("already exists")
    );
    assert_eq!(std::fs::read(output).unwrap(), b"existing data");
}

#[test]
fn cancel_removes_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.mp4");
    fixture(&input, "640x360", "120", "3", false);
    let media = probe::inspect(&input).unwrap();
    let edit = Edit::new(media.duration, media.width, media.height);
    let mut settings = Settings::for_media(&media);
    settings.encoder = Encoder::Cpu;
    settings.codec = Codec::Vp9;
    settings.container = Container::Webm;
    let output = dir.path().join("cancel.webm");
    let job = export::start(media, edit, settings, output.clone(), || {});
    assert!(job.events.recv_timeout(Duration::from_secs(5)).is_ok());
    job.cancel();
    assert_eq!(finish(job).unwrap_err(), "cancelled");
    assert!(!output.exists());
    assert!(!dir.path().read_dir().unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".cutline-")
    }));
}

#[test]
fn source_rate_is_not_replaced_by_an_output_rate_argument() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.mp4");
    fixture(&input, "320x180", "240", "0.25", false);
    let media = probe::inspect(&input).unwrap();
    let edit = Edit::new(media.duration, media.width, media.height);
    let args = export::arguments(
        &media,
        &edit,
        &Settings::for_media(&media),
        Encoder::Cpu,
        Path::new("out.mp4"),
    )
    .unwrap();
    assert!(!args.iter().any(|s| s == "-r" || s == "-vsync"));
    assert!(
        args.windows(2)
            .any(|p| p[0] == "-fps_mode" && p[1] == "passthrough")
    );
}

#[test]
fn portrait_rotation_is_baked_before_pixel_crop() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("landscape.mp4");
    fixture(&original, "640x360", "60", "1", false);
    let input = dir.path().join("phone.mov");
    assert!(
        Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-display_rotation", "90", "-i"])
            .arg(&original)
            .args(["-c", "copy"])
            .arg(&input)
            .status()
            .unwrap()
            .success()
    );
    let media = probe::inspect(&input).unwrap();
    assert_eq!(media.dimensions(), [360, 640]);
    let mut edit = Edit::new(media.duration, media.width, media.height);
    edit.crop = Crop {
        x: 20,
        y: 40,
        width: 300,
        height: 500,
    };
    let output = dir.path().join("portrait.mp4");
    let mut settings = Settings::for_media(&media);
    settings.encoder = Encoder::Cpu;
    finish(export::start(media, edit, settings, output.clone(), || {})).unwrap();
    assert_eq!(probe::inspect(&output).unwrap().dimensions(), [300, 500]);
    assert_eq!(count_frames(&output), 60);
}

#[test]
fn webm_and_hevc_exports_keep_sixty_fps() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.mp4");
    fixture(&input, "320x180", "60", "0.5", true);
    let media = probe::inspect(&input).unwrap();
    let edit = Edit::new(media.duration, media.width, media.height);
    for (codec, container) in [(Codec::Vp9, Container::Webm), (Codec::Hevc, Container::Mp4)] {
        let mut settings = Settings::for_media(&media);
        settings.encoder = Encoder::Cpu;
        settings.codec = codec;
        settings.container = container;
        let output = dir
            .path()
            .join(format!("encoded.{}", container.extension()));
        finish(export::start(
            media.clone(),
            edit.clone(),
            settings,
            output.clone(),
            || {},
        ))
        .unwrap();
        assert_eq!(count_frames(&output), 30);
        assert_eq!(probe::inspect(&output).unwrap().audio_tracks, 1);
    }
}

#[test]
fn variable_frame_timestamps_survive_crop_without_duplication() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("vfr.mp4");
    assert!(
        Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=120:duration=2",
                "-vf",
                "select='if(lt(n,120),1,not(mod(n,2)))'",
                "-fps_mode",
                "vfr",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast"
            ])
            .arg(&input)
            .status()
            .unwrap()
            .success()
    );
    let media = probe::inspect(&input).unwrap();
    assert!(media.variable_rate);
    let mut edit = Edit::new(media.duration, media.width, media.height);
    edit.crop = Crop {
        x: 0,
        y: 0,
        width: 300,
        height: 160,
    };
    let mut settings = Settings::for_media(&media);
    settings.encoder = Encoder::Cpu;
    let output = dir.path().join("vfr-cut.mp4");
    finish(export::start(media, edit, settings, output.clone(), || {})).unwrap();
    assert_eq!(count_frames(&output), count_frames(&input));
    let timestamps = |p: &Path| {
        let out = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "frame=best_effort_timestamp_time",
                "-of",
                "csv=p=0",
            ])
            .arg(p)
            .output()
            .unwrap();
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .filter_map(|s| s.trim_end_matches(',').parse::<f64>().ok())
            .collect::<Vec<_>>()
    };
    let a = timestamps(&input);
    let b = timestamps(&output);
    assert_eq!(a.len(), b.len());
    assert!(a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 0.00001));
}

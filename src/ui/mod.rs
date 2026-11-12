mod cell;
mod crop;
mod kit;
mod theme;
mod timeline;

use cell::{Canvas, Font, R};
use cutline::{
    media::{
        export::{self, Codec, Container, Encoder, Settings},
        player::{Playback, Player},
        probe::{self, Media},
        session,
    },
    model::{Crop, Edit, History, Trim, parse_timecode, timecode},
};
use eframe::egui::{self, Key, Rect, Sense, ViewportCommand, WindowLevel};
use kit::{Event, Icon, Item, Line, Tone};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};
use timeline::Timeline;

enum Message {
    Picked(Option<PathBuf>),
    Probed(u64, Result<Media, String>),
    SavePath(Option<PathBuf>, Media, Edit, Settings),
}
#[derive(PartialEq)]
enum Dialog {
    None,
    Export,
    Help,
    Close,
}

/// Everything a key can do. Bars and sheets are built as data carrying these; `App::act`
/// is the one place they are carried out.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Open,
    Keys,
    Pip,
    Full,
    Minimize,
    Maximize,
    Close,
    Play,
    Back,
    Forward,
    MarkIn,
    MarkOut,
    Trim,
    Crop,
    Source,
    Loop,
    Mute,
    More,
    Export,
    Speed(usize),
    ZoomOut,
    ZoomIn,
    ZoomFit,
    Undo,
    Redo,
    ResetTrim,
    Pin,
    Aspect(usize),
    CropReset,
    CropCancel,
    CropDone,
    Dismiss,
    OpenExport,
    ShowExport,
    CancelExport,
    Container(Container),
    Codec(Codec),
    Encoder(Encoder),
    Audio(bool),
    Destination,
    CloseDialog,
    KeepExporting,
    CancelAndClose,
}
/// A value the person can set directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    In,
    Out,
    X,
    Y,
    W,
    H,
    Volume,
    Quality,
}

const SPEEDS: [(f64, &str); 4] = [(0.5, "0.5×"), (1.0, "1×"), (1.5, "1.5×"), (2.0, "2×")];
const ASPECTS: [(&str, Option<f64>); 6] = [
    ("FREE", None),
    ("16:9", Some(16.0 / 9.0)),
    ("9:16", Some(9.0 / 16.0)),
    ("1:1", Some(1.0)),
    ("4:3", Some(4.0 / 3.0)),
    ("21:9", Some(21.0 / 9.0)),
];
const KEYS: [(&str, &str); 19] = [
    ("SPACE", "Play / pause"),
    ("← →", "Previous / next frame"),
    ("SHIFT + ← →", "Seek five seconds"),
    ("I  O", "Set start / end at the displayed frame"),
    ("HOME  END", "Go to the first / last selected frame"),
    ("T", "Type exact start and end times"),
    ("C", "Crop; Enter commits, Escape cancels"),
    ("V", "Show the source frame / the cropped frame"),
    ("L", "Loop the selection"),
    ("M", "Mute / unmute"),
    ("F  F11", "Fullscreen"),
    ("P", "Picture-in-picture"),
    ("CTRL + O  E", "Open / export"),
    ("CTRL + Z", "Undo"),
    ("CTRL + Y", "Redo (also Ctrl + Shift + Z)"),
    ("CTRL + WHEEL", "Zoom the timeline around the playhead"),
    (
        "WHEEL",
        "Pan a zoomed timeline; step a value under the pointer",
    ),
    ("SHIFT + DRAG", "Move both trim brackets together"),
    ("TAB", "Keep the controls on screen and step through them"),
];

pub struct App {
    player: Option<Player>,
    media: Option<Media>,
    edit: Option<Edit>,
    history: History,
    playback: Playback,
    timeline: Timeline,
    crop: crop::CropEditor,
    crop_before: Option<Edit>,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    ticket: u64,
    loading: bool,
    picker: bool,
    error: Option<String>,
    notice: Option<String>,
    dialog: Dialog,
    last_pointer: Option<egui::Pos2>,
    last_activity: Instant,
    pin: bool,
    fullscreen: bool,
    pip: bool,
    previous_rect: Option<Rect>,
    volume: f64,
    /// Level to return to when M un-mutes.
    audible_volume: f64,
    speed: f64,
    loop_selection: bool,
    original: bool,
    settings: Option<Settings>,
    job: Option<export::Job>,
    progress: f32,
    export_status: String,
    export_speed: String,
    completed: Option<PathBuf>,
    last_poll: Instant,
    last_seek: Instant,
    pending_seek: Option<(f64, bool)>,
    save_at: Option<Instant>,
    loop_applied: Option<(f64, f64, bool)>,
    allow_close: bool,
    /// The bitmap face for the current display scale; rebuilt when the scale changes.
    font: Option<Font>,
    io: kit::Io,
    /// The second deck line (level, speed, zoom, history) is open.
    more: bool,
    /// 0 when the bars are fully retracted, 1 when fully out.
    reveal: f32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let (player, error) = match Player::new(cc) {
            Ok(p) => (Some(p), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        let mut app = Self {
            player,
            media: None,
            edit: None,
            history: History::default(),
            playback: Playback {
                paused: true,
                ..Default::default()
            },
            timeline: Timeline::default(),
            crop: crop::CropEditor::default(),
            crop_before: None,
            tx,
            rx,
            ticket: 0,
            loading: false,
            picker: false,
            error,
            notice: None,
            dialog: Dialog::None,
            last_pointer: None,
            last_activity: Instant::now(),
            pin: false,
            fullscreen: false,
            pip: false,
            previous_rect: None,
            volume: 75.0,
            audible_volume: 75.0,
            speed: 1.0,
            loop_selection: false,
            original: false,
            settings: None,
            job: None,
            progress: 0.0,
            export_status: String::new(),
            export_speed: String::new(),
            completed: None,
            last_poll: Instant::now(),
            last_seek: Instant::now(),
            pending_seek: None,
            save_at: None,
            loop_applied: None,
            allow_close: false,
            font: None,
            io: kit::Io {
                entry: None,
                events: Vec::new(),
                live: true,
            },
            more: false,
            reveal: 1.0,
        };
        if let Some(path) = std::env::args_os().nth(1) {
            app.open(PathBuf::from(path), &cc.egui_ctx);
        }
        app
    }
    fn run(&mut self, f: impl FnOnce(&Player) -> anyhow::Result<()>) {
        if let Some(player) = &self.player
            && let Err(error) = f(player)
        {
            self.error = Some(format!("{error:#}"));
        }
    }
    fn remember(&mut self, before: Edit) {
        if let Some(edit) = &self.edit {
            self.history.record(before, edit);
        }
        self.save_at = Some(Instant::now() + Duration::from_millis(400));
        self.loop_applied = None;
    }
    fn save(&mut self) {
        // A crop stays a reversible draft until Done, export, or a viewing-mode switch.
        // Persist the last committed edit if the application closes during a crop gesture.
        let committed = self.crop_before.as_ref().or(self.edit.as_ref());
        if let (Some(media), Some(edit)) = (&self.media, committed)
            && let Err(error) = session::save(media, edit)
        {
            self.error = Some(format!(
                "Your source is safe, but edit recovery could not be saved: {error}"
            ));
        }
        self.save_at = None;
    }
    fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.player.is_none() {
            self.error = Some(
                "Playback engine unavailable. Extract the entire portable ZIP and restart Cutline."
                    .into(),
            );
            return;
        }
        self.finish_crop(false);
        self.save();
        self.ticket += 1;
        let ticket = self.ticket;
        self.loading = true;
        self.error = None;
        self.notice = None;
        self.completed = None;
        self.crop_before = None;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = probe::inspect(&path).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Message::Probed(ticket, result));
            ctx.request_repaint();
        });
    }
    fn pick(&mut self, ctx: &egui::Context) {
        if self.picker {
            return;
        }
        self.picker = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("Open a video")
                .add_filter(
                    "Video",
                    &[
                        "mp4", "mkv", "mov", "webm", "avi", "m4v", "mts", "m2ts", "ts", "mpg",
                        "mpeg", "wmv", "flv", "ogv",
                    ],
                )
                .add_filter("All files", &["*"])
                .pick_file();
            let _ = tx.send(Message::Picked(path));
            ctx.request_repaint();
        });
    }
    fn toggle_play(&mut self) {
        if self.media.is_none() {
            return;
        }
        if self.playback.paused
            && let Some(edit) = &self.edit
            && (self.playback.position < edit.trim.start
                || self.playback.position >= edit.trim.end - 0.005
                || self.playback.eof)
        {
            let start = edit.trim.start;
            self.run(|p| p.seek(start, true));
        }
        let pause = !self.playback.paused;
        self.run(|p| p.pause(pause));
        self.playback.paused = pause;
    }
    fn seek(&mut self, time: f64, exact: bool) {
        self.pending_seek = Some((time, exact));
        if exact || self.last_seek.elapsed() > Duration::from_millis(55) {
            self.run(|p| p.seek(time, exact));
            self.pending_seek = None;
            self.last_seek = Instant::now();
        }
        self.playback.position = time;
    }
    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        if self.pip {
            self.toggle_pip(ctx);
        }
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }
    fn toggle_pip(&mut self, ctx: &egui::Context) {
        if self.fullscreen {
            self.fullscreen = false;
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        }
        self.pip = !self.pip;
        if self.pip {
            self.previous_rect = ctx.input(|i| i.viewport().outer_rect);
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(egui::vec2(360.0, 220.0)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(480.0, 292.0)));
            self.finish_crop(true);
        } else {
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(egui::vec2(720.0, 480.0)));
            if let Some(rect) = self.previous_rect.take() {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(rect.size()));
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(rect.min));
            }
        }
        self.last_activity = Instant::now();
    }
    fn toggle_crop(&mut self) {
        if self.crop_before.is_some() {
            self.finish_crop(true);
            return;
        }
        if let Some(edit) = &self.edit {
            self.crop_before = Some(edit.clone());
            self.run(|p| p.pause(true));
            self.original = false;
        }
    }
    fn finish_crop(&mut self, accept: bool) {
        if let Some(before) = self.crop_before.take() {
            if accept {
                self.remember(before);
            } else {
                self.edit = Some(before);
            }
        }
    }
    fn export_dialog(&mut self) {
        if self.job.is_some() || self.media.is_none() {
            return;
        }
        self.finish_crop(true);
        self.io.entry = None;
        self.dialog = Dialog::Export;
    }
    fn can_export(&self) -> bool {
        self.media.as_ref().is_some_and(|m| !m.hdr_metadata) && !self.picker
    }
    fn choose_destination(&mut self, ctx: &egui::Context) {
        if !self.can_export() {
            return;
        }
        let (Some(media), Some(edit), Some(settings)) =
            (self.media.clone(), self.edit.clone(), self.settings.clone())
        else {
            return;
        };
        self.picker = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let suggestion = export::suggested_path(&media, settings.container);
            let path = rfd::FileDialog::new()
                .set_title("Export video — choose a new filename")
                .set_directory(suggestion.parent().unwrap_or(std::path::Path::new(".")))
                .set_file_name(suggestion.file_name().unwrap_or_default().to_string_lossy())
                .add_filter("Video", &[settings.container.extension()])
                .save_file();
            let _ = tx.send(Message::SavePath(path, media, edit, settings));
            ctx.request_repaint();
        });
    }
    fn step_history(&mut self, redo: bool) {
        if self.crop_before.is_some() {
            return;
        }
        if let Some(edit) = &mut self.edit {
            if redo {
                self.history.redo(edit);
            } else {
                self.history.undo(edit);
            }
        }
        self.save_at = Some(Instant::now());
        self.loop_applied = None;
    }
    fn toggle_mute(&mut self) {
        if self.volume > 0.0 {
            self.audible_volume = self.volume;
            self.volume = 0.0;
        } else {
            self.volume = self.audible_volume;
        }
        let v = self.volume;
        self.run(|p| p.volume(v));
    }
    /// Sets the start (or the end) of the selection at the displayed frame.
    fn mark(&mut self, start: bool) {
        if let (Some(edit), Some(media)) = (&mut self.edit, &self.media) {
            let before = edit.clone();
            let t = media.snap(self.playback.position);
            if start {
                edit.trim.start = t.min(edit.trim.end - media.rate.frame_seconds()).max(0.0);
            } else {
                edit.trim.end = (t + media.rate.frame_seconds())
                    .max(edit.trim.start + media.rate.frame_seconds())
                    .min(media.duration);
            }
            self.remember(before);
        }
    }
    fn slot_text(&self, slot: Slot) -> Option<String> {
        let edit = self.edit.as_ref()?;
        Some(match slot {
            Slot::In => timecode(edit.trim.start),
            Slot::Out => timecode(edit.trim.end),
            Slot::X => edit.crop.x.to_string(),
            Slot::Y => edit.crop.y.to_string(),
            Slot::W => edit.crop.width.to_string(),
            Slot::H => edit.crop.height.to_string(),
            Slot::Volume | Slot::Quality => return None,
        })
    }
    /// Opens a value for typing, with its current text selected.
    fn begin_entry(&mut self, slot: Slot) {
        if let Some(text) = self.slot_text(slot) {
            self.io.entry = Some(kit::Entry {
                slot,
                text,
                fresh: true,
            });
            self.last_activity = Instant::now();
        }
    }
    /// Applies typed text to its value. A rejected value leaves the edit untouched and says why.
    fn commit(&mut self, slot: Slot, text: &str) -> bool {
        match slot {
            Slot::In | Slot::Out => {
                let (Some(media), Some(edit)) = (&self.media, &self.edit) else {
                    return false;
                };
                let Some(value) = parse_timecode(text) else {
                    self.error = Some("Enter valid start and end times.".into());
                    return false;
                };
                let mut trim = edit.trim;
                if slot == Slot::In {
                    trim.start = media.snap(value);
                } else {
                    trim.end = media.snap(value);
                }
                if value > media.duration + 0.001 || !trim.valid(media.duration) {
                    self.error = Some("End must follow start, within the video duration.".into());
                    return false;
                }
                if trim == edit.trim {
                    return true;
                }
                let target = if slot == Slot::In {
                    trim.start
                } else {
                    (trim.end - media.rate.frame_seconds()).max(trim.start)
                };
                let before = edit.clone();
                if let Some(edit) = &mut self.edit {
                    edit.trim = trim;
                }
                self.remember(before);
                self.seek(target, true);
                true
            }
            Slot::X | Slot::Y | Slot::W | Slot::H => {
                let Ok(value) = text.parse::<u32>() else {
                    self.error = Some("Enter a whole number of pixels.".into());
                    return false;
                };
                if let (Some(edit), Some(media)) = (&mut self.edit, &self.media) {
                    let mut c = edit.crop;
                    match slot {
                        Slot::X => c.x = value,
                        Slot::Y => c.y = value,
                        Slot::W => c.width = value,
                        _ => c.height = value,
                    }
                    if let Some(ratio) = self.crop.ratio {
                        if slot == Slot::W {
                            c.height = (c.width as f64 / ratio).round() as u32;
                        } else if slot == Slot::H {
                            c.width = (c.height as f64 * ratio).round() as u32;
                        }
                    }
                    edit.crop = Crop::constrained(
                        c.x as f64,
                        c.y as f64,
                        c.width as f64,
                        c.height as f64,
                        media.dimensions(),
                    );
                }
                true
            }
            Slot::Volume | Slot::Quality => false,
        }
    }
    /// Steps a value: one frame for a time, two pixels for a crop coordinate.
    fn nudge(&mut self, slot: Slot, steps: i32) {
        let (Some(edit), Some(media)) = (&self.edit, &self.media) else {
            return;
        };
        let frame = media.rate.frame_seconds() * steps as f64;
        let pixels = |value: u32| (value as i64 + 2 * steps as i64).max(0).to_string();
        let text = match slot {
            Slot::In => timecode((edit.trim.start + frame).max(0.0)),
            Slot::Out => timecode((edit.trim.end + frame).clamp(0.0, media.duration)),
            Slot::X => pixels(edit.crop.x),
            Slot::Y => pixels(edit.crop.y),
            Slot::W => pixels(edit.crop.width),
            Slot::H => pixels(edit.crop.height),
            Slot::Volume | Slot::Quality => return,
        };
        // A time below one frame of change would be rejected as "end before start"; that is
        // the model's rule, so a nudge past the other bracket simply does nothing.
        let quiet = self.error.take();
        self.commit(slot, &text);
        self.error = quiet;
        if self
            .io
            .entry
            .as_ref()
            .is_some_and(|entry| entry.slot == slot)
        {
            self.begin_entry(slot);
        }
    }
    fn act(&mut self, ctx: &egui::Context, act: Act) {
        self.last_activity = Instant::now();
        match act {
            Act::Open => self.pick(ctx),
            Act::Keys => self.dialog = Dialog::Help,
            Act::Pip => self.toggle_pip(ctx),
            Act::Full => self.toggle_fullscreen(ctx),
            Act::Minimize => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
            Act::Maximize => {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            }
            Act::Close => ctx.send_viewport_cmd(ViewportCommand::Close),
            Act::Play => self.toggle_play(),
            Act::Back => self.run(|p| p.command(&["frame-back-step"])),
            Act::Forward => self.run(|p| p.command(&["frame-step"])),
            Act::MarkIn => self.mark(true),
            Act::MarkOut => self.mark(false),
            Act::Trim => self.begin_entry(Slot::In),
            Act::Crop => self.toggle_crop(),
            Act::Source => self.original = !self.original,
            Act::Loop => self.loop_selection = !self.loop_selection,
            Act::Mute => self.toggle_mute(),
            Act::More => self.more = !self.more,
            Act::Export => self.export_dialog(),
            Act::Speed(index) => {
                let speed = SPEEDS[index].0;
                self.speed = speed;
                self.run(|p| p.rate(speed));
            }
            Act::ZoomOut | Act::ZoomIn => {
                if let Some(media) = &self.media {
                    self.timeline.zoom_at(
                        if act == Act::ZoomIn { 2.0 } else { 0.5 },
                        self.playback.position,
                        media.duration,
                    );
                }
            }
            Act::ZoomFit => self.timeline.fit(),
            Act::Undo => self.step_history(false),
            Act::Redo => self.step_history(true),
            Act::ResetTrim => {
                if let (Some(edit), Some(media)) = (&mut self.edit, &self.media) {
                    let before = edit.clone();
                    edit.trim = Trim {
                        start: 0.0,
                        end: media.duration,
                    };
                    self.remember(before);
                }
            }
            Act::Pin => self.pin = !self.pin,
            Act::Aspect(index) => {
                let ratio = ASPECTS[index].1;
                self.crop.ratio = ratio;
                if let (Some(ratio), Some(edit), Some(media)) = (ratio, &mut self.edit, &self.media)
                {
                    edit.crop = edit.crop.fit_ratio(ratio, media.dimensions());
                }
            }
            Act::CropReset => {
                if let (Some(edit), Some(media)) = (&mut self.edit, &self.media) {
                    edit.crop = Crop::full(media.width, media.height);
                    self.crop.ratio = None;
                }
            }
            Act::CropCancel => self.finish_crop(false),
            Act::CropDone => self.finish_crop(true),
            Act::Dismiss => {
                self.error = None;
                self.notice = None;
                self.completed = None;
            }
            Act::OpenExport => {
                if let Some(path) = self.completed.take() {
                    self.open(path, ctx);
                }
            }
            Act::ShowExport => {
                if let Some(path) = &self.completed {
                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("explorer.exe")
                            .arg(format!("/select,{}", path.display()))
                            .spawn();
                    }
                    #[cfg(not(windows))]
                    {
                        let _ = std::process::Command::new("xdg-open")
                            .arg(path.parent().unwrap_or(std::path::Path::new(".")))
                            .spawn();
                    }
                }
            }
            Act::CancelExport => {
                if let Some(job) = &self.job {
                    job.cancel();
                }
                self.export_status = "Cancelling…".into();
            }
            Act::Container(container) => {
                if let (Some(settings), Some(media)) = (&mut self.settings, &self.media) {
                    settings.container = container;
                    if container == Container::Webm {
                        settings.codec = Codec::Vp9;
                    } else if settings.codec == Codec::Vp9 && container != Container::Mkv {
                        settings.codec = if media.hdr { Codec::Hevc } else { Codec::H264 };
                    }
                }
            }
            Act::Codec(codec) => {
                if let Some(settings) = &mut self.settings {
                    settings.codec = codec;
                }
            }
            Act::Encoder(encoder) => {
                if let Some(settings) = &mut self.settings {
                    settings.encoder = encoder;
                }
            }
            Act::Audio(mute) => {
                if let Some(settings) = &mut self.settings {
                    settings.mute = mute;
                }
            }
            Act::Destination => self.choose_destination(ctx),
            Act::CloseDialog | Act::KeepExporting => self.dialog = Dialog::None,
            Act::CancelAndClose => {
                if let Some(job) = &self.job {
                    job.cancel();
                }
                self.allow_close = true;
                self.dialog = Dialog::None;
            }
        }
    }
    /// Carries out what the bars and sheets reported this frame.
    fn events(&mut self, ctx: &egui::Context) {
        let mut events = std::mem::take(&mut self.io.events);
        // A click that leaves one field for another reports both; close before opening.
        events.sort_by_key(|event| matches!(event, Event::Edit(_)));
        for event in events {
            match event {
                Event::Act(act) => self.act(ctx, act),
                Event::Meter(Slot::Volume, level) => {
                    self.volume = level as f64 * 5.0;
                    let v = self.volume;
                    self.run(|p| p.volume(v));
                }
                Event::Meter(Slot::Quality, level) => {
                    if let Some(settings) = &mut self.settings {
                        settings.quality = (31 - level.clamp(1, 17)) as u8;
                    }
                }
                Event::Meter(..) => {}
                Event::Edit(slot) => self.begin_entry(slot),
                Event::Commit(slot, text, advance) => {
                    self.io.entry = None;
                    if self.commit(slot, &text) && advance != 0 {
                        let order: &[Slot] = match slot {
                            Slot::In | Slot::Out => &[Slot::In, Slot::Out],
                            _ => &[Slot::X, Slot::Y, Slot::W, Slot::H],
                        };
                        let at = order.iter().position(|s| *s == slot).unwrap_or(0) as i32;
                        let next = (at + advance as i32).rem_euclid(order.len() as i32);
                        self.begin_entry(order[next as usize]);
                    }
                }
                Event::Cancel => self.io.entry = None,
                Event::Nudge(slot, steps) => self.nudge(slot, steps),
            }
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
        }) {
            self.last_activity = Instant::now();
        }
        // An open field owns the keyboard.
        if self.io.entry.is_some() {
            return;
        }
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let enter = ctx.input(|i| i.key_pressed(Key::Enter));
        if self.dialog != Dialog::None {
            if escape {
                self.dialog = Dialog::None;
            } else if enter {
                match self.dialog {
                    Dialog::Export => self.choose_destination(ctx),
                    _ => self.dialog = Dialog::None,
                }
            }
            return;
        }
        if escape {
            if self.crop_before.is_some() {
                self.finish_crop(false);
            } else if self.fullscreen {
                self.toggle_fullscreen(ctx);
            } else if self.pip {
                self.toggle_pip(ctx);
            }
        }
        // A control holding keyboard focus takes Enter and Space itself.
        let focused = ctx.memory(|m| m.focused()).is_some();
        if enter && !focused && self.crop_before.is_some() {
            self.finish_crop(true);
        }
        // Read modifiers attached to each key event. The current modifier state
        // can already be released by the time a paused video's next UI frame runs.
        let matching_key = |key, command, shifted: Option<bool>| {
            ctx.input(|i| {
                i.events.iter().any(|event| {
                    matches!(event, egui::Event::Key { key: k, pressed: true, modifiers, .. }
                if *k == key && modifiers.command == command && !modifiers.alt
                    && shifted.is_none_or(|shift| modifiers.shift == shift))
                })
            })
        };
        let pressed = |key| matching_key(key, false, None);
        let command_pressed = |key, shift| matching_key(key, true, Some(shift));
        let shift = matching_key(Key::ArrowLeft, false, Some(true))
            || matching_key(Key::ArrowRight, false, Some(true));
        if pressed(Key::Tab) {
            self.pin = true;
        }
        if command_pressed(Key::O, false) {
            self.pick(ctx);
        }
        if command_pressed(Key::E, false) {
            self.export_dialog();
        }
        if pressed(Key::F1) {
            self.dialog = Dialog::Help;
        }
        if command_pressed(Key::Z, false) {
            self.step_history(false);
        }
        if command_pressed(Key::Z, true) || command_pressed(Key::Y, false) {
            self.step_history(true);
        }
        if pressed(Key::Space) && !focused {
            self.toggle_play();
        }
        if pressed(Key::F) || pressed(Key::F11) {
            self.toggle_fullscreen(ctx);
        }
        if pressed(Key::P) {
            self.toggle_pip(ctx);
        }
        if pressed(Key::C) && !self.pip {
            self.toggle_crop();
        }
        if pressed(Key::V) {
            self.original = !self.original;
        }
        if pressed(Key::L) {
            self.loop_selection = !self.loop_selection;
        }
        if pressed(Key::T) && !self.pip {
            self.begin_entry(Slot::In);
        }
        if pressed(Key::M) {
            self.toggle_mute();
        }
        if pressed(Key::ArrowLeft) || pressed(Key::ArrowRight) {
            let right = pressed(Key::ArrowRight);
            if shift {
                let time = (self.playback.position + if right { 5.0 } else { -5.0 })
                    .clamp(0.0, self.media.as_ref().map_or(0.0, |m| m.duration));
                self.seek(time, true);
            } else if !focused {
                self.run(|p| {
                    p.command(&[if right {
                        "frame-step"
                    } else {
                        "frame-back-step"
                    }])
                });
            }
        }
        if pressed(Key::Home)
            && let Some(edit) = &self.edit
        {
            self.seek(edit.trim.start, true);
        }
        if pressed(Key::End)
            && let (Some(edit), Some(media)) = (&self.edit, &self.media)
        {
            self.seek(
                (edit.trim.end - media.rate.frame_seconds()).max(edit.trim.start),
                true,
            );
        }
        if pressed(Key::I) {
            self.mark(true);
        }
        if pressed(Key::O) {
            self.mark(false);
        }
    }
    fn messages(&mut self, ctx: &egui::Context) {
        for message in self.rx.try_iter().collect::<Vec<_>>() {
            match message {
                Message::Picked(path) => {
                    self.picker = false;
                    if let Some(path) = path {
                        self.open(path, ctx);
                    }
                }
                Message::Probed(ticket, result) if ticket == self.ticket => {
                    self.loading = false;
                    match result {
                        Ok(media) => {
                            let edit = session::restore(&media).unwrap_or_else(|| {
                                Edit::new(media.duration, media.width, media.height)
                            });
                            if let Some(player) = &mut self.player
                                && let Err(error) = player.load(&media.path)
                            {
                                self.error = Some(format!("{error:#}"));
                                continue;
                            }
                            ctx.send_viewport_cmd(ViewportCommand::Title(format!(
                                "{} — Cutline",
                                media.name()
                            )));
                            self.settings = Some(Settings::for_media(&media));
                            self.media = Some(media);
                            self.edit = Some(edit);
                            self.history = History::default();
                            self.timeline.fit();
                            self.playback = Playback {
                                paused: true,
                                ..Default::default()
                            };
                            self.loop_applied = None;
                            self.original = false;
                            self.last_activity = Instant::now();
                            let volume = self.volume;
                            self.run(|p| p.volume(volume));
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Message::Probed(_, _) => {}
                Message::SavePath(path, media, edit, settings) => {
                    self.picker = false;
                    if let Some(output) = path {
                        let context = ctx.clone();
                        self.progress = 0.0;
                        self.export_status = "Starting export…".into();
                        self.completed = None;
                        self.job = Some(export::start(media, edit, settings, output, move || {
                            context.request_repaint()
                        }));
                        self.dialog = Dialog::None;
                    }
                }
            }
        }
        if let Some(job) = &self.job {
            let events = job.events.try_iter().collect::<Vec<_>>();
            let mut finished = false;
            for event in events {
                match event {
                    export::Event::Status(text) => self.export_status = text,
                    export::Event::Progress { fraction, speed } => {
                        self.progress = fraction;
                        self.export_speed = speed;
                    }
                    export::Event::Complete(path) => {
                        self.notice = Some("Export complete. Your source is unchanged.".into());
                        self.completed = Some(path);
                        finished = true;
                    }
                    export::Event::Failed(error) => {
                        self.completed = None;
                        self.error = Some(error);
                        finished = true;
                    }
                    export::Event::Cancelled => {
                        self.notice =
                            Some("Export cancelled. The partial file was removed.".into());
                        finished = true;
                    }
                }
            }
            if finished {
                self.job = None;
            }
        }
        if self.last_poll.elapsed() > Duration::from_millis(16) {
            self.last_poll = Instant::now();
            if let Some(player) = &mut self.player {
                let was_loaded = player.loaded;
                match player.poll() {
                    Ok(state) => self.playback = state,
                    Err(error) => self.error = Some(format!("Playback failed: {error:#}")),
                }
                if !was_loaded
                    && player.loaded
                    && let Some(edit) = &self.edit
                {
                    let _ = player.seek(edit.trim.start, true);
                }
            }
        }
        if let Some((time, exact)) = self.pending_seek
            && self.last_seek.elapsed() > Duration::from_millis(55)
        {
            self.run(|p| p.seek(time, exact));
            self.pending_seek = None;
            self.last_seek = Instant::now();
        }
        if self.save_at.is_some_and(|at| Instant::now() >= at) {
            self.save();
        }
        if let Some(edit) = &self.edit {
            let range = (edit.trim.start, edit.trim.end, self.loop_selection);
            if self.loop_applied != Some(range) && self.player.as_ref().is_some_and(|p| p.loaded) {
                self.run(|p| {
                    let a = range.0.to_string();
                    let b = range.1.to_string();
                    p.command(&["set", "ab-loop-a", if range.2 { &a } else { "no" }])?;
                    p.command(&["set", "ab-loop-b", if range.2 { &b } else { "no" }])?;
                    Ok(())
                });
                self.loop_applied = Some(range);
            }
            if !range.2 && !self.playback.paused && self.playback.position >= range.1 {
                self.run(|p| p.pause(true));
            }
        }
    }
    // ── What the bars say ────────────────────────────────────────────────────────────────

    fn top_items(&self) -> Vec<Item> {
        let close = Item::Icon {
            icon: Icon::Close,
            act: Act::Close,
        };
        if self.pip {
            return vec![kit::key("P", "RETURN", Act::Pip), close];
        }
        vec![
            kit::key("O", "OPEN", Act::Open),
            kit::key("F1", "KEYS", Act::Keys),
            kit::key("P", "PIP", Act::Pip),
            kit::key(
                "F",
                if self.fullscreen { "WINDOW" } else { "FULL" },
                Act::Full,
            ),
            Item::Gap(1),
            Item::Icon {
                icon: Icon::Minimize,
                act: Act::Minimize,
            },
            Item::Icon {
                icon: Icon::Maximize,
                act: Act::Maximize,
            },
            close,
        ]
    }
    /// The deck's lines above the timeline, and its key line below it.
    fn deck_lines(&self) -> (Vec<Line>, Vec<Line>) {
        let (Some(media), Some(edit)) = (&self.media, &self.edit) else {
            return (Vec::new(), Vec::new());
        };
        let play = kit::key(
            "SPACE",
            if self.playback.paused {
                "PLAY"
            } else {
                "PAUSE"
            },
            Act::Play,
        );
        let position = kit::colored(timecode(self.playback.position), theme::INK, true);
        if self.pip {
            let line = vec![play, Item::Gap(2), position];
            return (Vec::new(), vec![Line::row(line)]);
        }
        let cropping = self.crop_before.is_some();
        let mut upper = Vec::new();
        if cropping {
            let mut line = vec![kit::tag("CROP", theme::CROP), Item::Gap(2)];
            for (label, slot, value) in [
                ("X ", Slot::X, edit.crop.x),
                ("Y ", Slot::Y, edit.crop.y),
                ("W ", Slot::W, edit.crop.width),
                ("H ", Slot::H, edit.crop.height),
            ] {
                line.push(kit::dim(label));
                line.push(Item::Field {
                    slot,
                    text: value.to_string(),
                    cells: 5,
                    color: theme::CROP,
                });
                line.push(Item::Gap(2));
            }
            line.push(Item::Rule);
            line.push(Item::Gap(1));
            for (index, (label, ratio)) in ASPECTS.into_iter().enumerate() {
                line.push(kit::latch(
                    "",
                    label,
                    self.crop.ratio == ratio,
                    Act::Aspect(index),
                ));
            }
            line.extend([
                Item::Gap(1),
                Item::Rule,
                Item::Gap(1),
                kit::key("", "RESET", Act::CropReset),
                Item::Spring,
                kit::key("ESC", "CANCEL", Act::CropCancel),
                Item::Gap(1),
                kit::toned("ENTER", "DONE", Tone::Primary, Act::CropDone),
            ]);
            upper.push(Line::row(line));
        }
        if self.more {
            let level = (self.volume / 5.0).round() as usize;
            let mut line = vec![
                kit::dim("LEVEL "),
                Item::Meter {
                    slot: Slot::Volume,
                    value: level,
                    steps: 20,
                },
                kit::text(format!(" {:>3.0}", self.volume)),
                Item::Rule,
                kit::dim("SPEED"),
            ];
            for (index, (speed, label)) in SPEEDS.into_iter().enumerate() {
                line.push(kit::latch(
                    "",
                    label,
                    self.speed == speed,
                    Act::Speed(index),
                ));
            }
            let history = |able: bool| {
                if able && !cropping {
                    Tone::Plain
                } else {
                    Tone::Off
                }
            };
            line.extend([
                Item::Rule,
                kit::dim("ZOOM"),
                kit::key("", "−", Act::ZoomOut),
                kit::key("", "+", Act::ZoomIn),
                kit::key("", "FIT", Act::ZoomFit),
                Item::Rule,
                kit::toned(
                    "CTRL+Z",
                    "UNDO",
                    history(self.history.can_undo()),
                    Act::Undo,
                ),
                kit::toned(
                    "CTRL+Y",
                    "REDO",
                    history(self.history.can_redo()),
                    Act::Redo,
                ),
                kit::key("", "RESET TRIM", Act::ResetTrim),
                Item::Rule,
                kit::latch("TAB", "PIN", self.pin, Act::Pin),
            ]);
            upper.push(Line::row(line));
        }
        let field = |slot, seconds: f64| Item::Field {
            slot,
            text: timecode(seconds),
            cells: 12,
            color: theme::TRIM,
        };
        let mut registers = vec![
            kit::dim("POS "),
            position,
            Item::Gap(1),
            Item::Rule,
            Item::Gap(1),
            kit::dim("IN "),
            field(Slot::In, edit.trim.start),
            Item::Gap(2),
            kit::dim("OUT "),
            field(Slot::Out, edit.trim.end),
            Item::Gap(2),
            kit::dim("DUR "),
            kit::text(timecode(edit.trim.duration())),
            Item::Spring,
        ];
        if self.timeline.zoom > 1.0 {
            registers.push(kit::dim(format!("ZOOM {:.0}×", self.timeline.zoom)));
        }
        if media.variable_rate {
            registers.push(kit::dim("  VFR"));
        }
        upper.push(Line::row(registers));
        let keys = vec![
            play,
            kit::key("←", "", Act::Back),
            kit::key("→", "", Act::Forward),
            Item::Gap(1),
            Item::Rule,
            Item::Gap(1),
            kit::key("I", "IN", Act::MarkIn),
            kit::key("O", "OUT", Act::MarkOut),
            kit::key("T", "TRIM", Act::Trim),
            Item::Gap(1),
            Item::Rule,
            Item::Gap(1),
            kit::latch("C", "CROP", cropping, Act::Crop),
            Item::Gap(1),
            kit::latch("V", "SOURCE", self.original, Act::Source),
            Item::Gap(1),
            kit::latch("L", "LOOP", self.loop_selection, Act::Loop),
            Item::Gap(1),
            kit::latch("M", "MUTE", self.volume == 0.0, Act::Mute),
            Item::Spring,
            kit::latch("", "MORE", self.more, Act::More),
            Item::Gap(1),
            kit::toned(
                "CTRL+E",
                "EXPORT",
                if self.job.is_none() && !self.loading {
                    Tone::Primary
                } else {
                    Tone::Off
                },
                Act::Export,
            ),
        ];
        (upper, vec![Line::row(keys)])
    }
    /// Lines docked under the top bar: reading, exporting, and messages that wait for the
    /// person. `cells` is the width available for text.
    fn status_lines(&self, cells: usize, time: f64) -> Vec<Line> {
        let mut lines = Vec::new();
        // With nothing open, the empty window itself shows the reading indicator.
        if self.loading && self.media.is_some() {
            lines.push(Line::row(vec![
                kit::tag("READING", theme::DIM),
                Item::Gap(2),
                kit::text(Self::stepping(time)),
            ]));
        }
        if self.job.is_some() {
            lines.push(Line::row(vec![
                kit::tag("EXPORT", theme::TRIM),
                Item::Gap(2),
                Item::Bar {
                    fraction: self.progress,
                    cells: 24,
                },
                kit::text(format!(" {:>3.0}%", self.progress * 100.0)),
                Item::Gap(2),
                kit::dim(self.export_speed.clone()),
                Item::Gap(2),
                kit::dim(self.export_status.clone()),
                Item::Spring,
                kit::key("", "CANCEL", Act::CancelExport),
            ]));
        }
        let message = self
            .error
            .as_ref()
            .map(|text| ("ERROR", theme::DANGER, text))
            .or(self.notice.as_ref().map(|text| ("NOTE", theme::INK, text)));
        if let Some((label, color, text)) = message {
            let indent = label.chars().count() as i32 + 4;
            let wrapped = cell::wrap(text, cells.saturating_sub(indent as usize));
            let last = wrapped.len() - 1;
            for (number, part) in wrapped.into_iter().enumerate() {
                let mut line = if number == 0 {
                    vec![kit::tag(label, color), Item::Gap(2)]
                } else {
                    vec![Item::Text {
                        text: " ".repeat(indent as usize),
                        color,
                        bold: false,
                    }]
                };
                line.push(kit::text(part));
                if number == last {
                    line.push(Item::Spring);
                    if self.completed.is_some() {
                        line.push(kit::key("", "OPEN EXPORT", Act::OpenExport));
                        line.push(kit::key("", "SHOW IN FOLDER", Act::ShowExport));
                    }
                    line.push(kit::key("", "DISMISS", Act::Dismiss));
                }
                lines.push(Line::row(line));
            }
        }
        lines
    }
    fn sheet_lines(&self, cells: usize) -> (&'static str, Vec<Line>) {
        let blank = || Line::tight(Vec::new());
        let prose = |lines: &mut Vec<Line>, text: &str, color| {
            for part in cell::wrap(text, cells) {
                lines.push(Line::tight(vec![kit::colored(part, color, false)]));
            }
        };
        let mut lines = Vec::new();
        match self.dialog {
            Dialog::None => {}
            Dialog::Help => {
                for (keys, action) in KEYS {
                    lines.push(Line::tight(vec![
                        kit::dim(format!("{keys:<15}")),
                        kit::text(action),
                    ]));
                }
                lines.push(blank());
                lines.push(Line::row(vec![
                    Item::Spring,
                    kit::toned(
                        "ENTER",
                        "BACK TO THE VIDEO",
                        Tone::Primary,
                        Act::CloseDialog,
                    ),
                ]));
                return ("KEYS", lines);
            }
            Dialog::Close => {
                prose(
                    &mut lines,
                    "Closing now cancels the export and removes its partial file. Your source stays unchanged.",
                    theme::INK,
                );
                lines.push(blank());
                lines.push(Line::row(vec![
                    Item::Spring,
                    kit::key("", "CANCEL EXPORT AND CLOSE", Act::CancelAndClose),
                    Item::Gap(1),
                    kit::toned("ENTER", "KEEP EXPORTING", Tone::Primary, Act::KeepExporting),
                ]));
                return ("AN EXPORT IS STILL RUNNING", lines);
            }
            Dialog::Export => {
                let (Some(media), Some(edit), Some(settings)) =
                    (&self.media, &self.edit, &self.settings)
                else {
                    return ("EXPORT", lines);
                };
                lines.push(Line::tight(vec![kit::colored(
                    format!(
                        "{}×{}   {:.3} FPS   {}",
                        edit.crop.width,
                        edit.crop.height,
                        media.rate.fps(),
                        timecode(edit.trim.duration())
                    ),
                    theme::TRIM,
                    true,
                )]));
                prose(
                    &mut lines,
                    "Source frame timing is preserved. No frame-rate cap.",
                    theme::DIM,
                );
                lines.push(blank());
                let choice = |selected: bool, enabled: bool| match (enabled, selected) {
                    (false, _) => Tone::Off,
                    (true, true) => Tone::On,
                    (true, false) => Tone::Plain,
                };
                let mut row = vec![kit::dim("FORMAT   ")];
                for container in [
                    Container::Mp4,
                    Container::Mkv,
                    Container::Mov,
                    Container::Webm,
                ] {
                    row.push(kit::toned(
                        "",
                        &container.extension().to_uppercase(),
                        choice(settings.container == container, true),
                        Act::Container(container),
                    ));
                }
                lines.push(Line::row(row));
                let mut row = vec![kit::dim("VIDEO    ")];
                for (codec, label) in [
                    (Codec::H264, "H.264"),
                    (Codec::Hevc, "HEVC"),
                    (Codec::Vp9, "VP9"),
                ] {
                    let enabled = !(media.hdr && codec == Codec::H264)
                        && if settings.container == Container::Webm {
                            codec == Codec::Vp9
                        } else {
                            codec != Codec::Vp9 || settings.container == Container::Mkv
                        };
                    row.push(kit::toned(
                        "",
                        label,
                        choice(settings.codec == codec, enabled),
                        Act::Codec(codec),
                    ));
                }
                row.push(Item::Gap(2));
                row.push(kit::dim(match settings.codec {
                    Codec::H264 => "widely compatible",
                    Codec::Hevc => "smaller files; carries HDR",
                    Codec::Vp9 => "for WebM",
                }));
                lines.push(Line::row(row));
                let mut row = vec![kit::dim("ENCODER  ")];
                for (encoder, label) in [
                    (Encoder::Auto, "AUTO"),
                    (Encoder::Cpu, "CPU"),
                    (Encoder::Nvidia, "NVIDIA"),
                    (Encoder::Intel, "INTEL"),
                    (Encoder::Amd, "AMD"),
                ] {
                    let enabled = settings.codec != Codec::Vp9
                        || matches!(encoder, Encoder::Auto | Encoder::Cpu);
                    row.push(kit::toned(
                        "",
                        label,
                        choice(settings.encoder == encoder, enabled),
                        Act::Encoder(encoder),
                    ));
                }
                row.push(Item::Gap(2));
                row.push(kit::dim(match settings.encoder {
                    Encoder::Auto => "NVIDIA, Intel, AMD, then CPU",
                    Encoder::Cpu => "consistent quality",
                    Encoder::Nvidia => "fails if unavailable",
                    Encoder::Intel => "Quick Sync; fails if unavailable",
                    Encoder::Amd => "fails if unavailable",
                }));
                lines.push(Line::row(row));
                lines.push(Line::row(vec![
                    kit::dim("QUALITY   "),
                    Item::Meter {
                        slot: Slot::Quality,
                        value: 31 - settings.quality.clamp(14, 30) as usize,
                        steps: 17,
                    },
                    kit::text(format!(" CQ {}", settings.quality)),
                    Item::Gap(2),
                    kit::dim("lower CQ: better, larger"),
                ]));
                let mut row = vec![kit::dim("AUDIO    ")];
                if media.audio_tracks == 0 {
                    row.push(kit::dim(" none in the source"));
                } else {
                    row.push(kit::latch(
                        "",
                        &format!(
                            "KEEP {} TRACK{}",
                            media.audio_tracks,
                            if media.audio_tracks == 1 { "" } else { "S" }
                        ),
                        !settings.mute,
                        Act::Audio(false),
                    ));
                    row.push(kit::latch("", "REMOVE", settings.mute, Act::Audio(true)));
                }
                lines.push(Line::row(row));
                lines.push(blank());
                prose(
                    &mut lines,
                    "Subtitles and chapters are not exported.",
                    theme::DIM,
                );
                if media.hdr {
                    prose(
                        &mut lines,
                        "HDR / 10-bit retained. Preview is tone-mapped for this window.",
                        theme::INK,
                    );
                }
                if media.hdr_metadata {
                    prose(
                        &mut lines,
                        "Dynamic HDR metadata is present. This export path cannot preserve it, so export is disabled.",
                        theme::DANGER,
                    );
                }
                lines.push(blank());
                lines.push(Line::row(vec![
                    Item::Spring,
                    kit::key("ESC", "CANCEL", Act::CloseDialog),
                    Item::Gap(1),
                    kit::toned(
                        "ENTER",
                        "CHOOSE DESTINATION",
                        if self.can_export() {
                            Tone::Primary
                        } else {
                            Tone::Off
                        },
                        Act::Destination,
                    ),
                ]));
            }
        }
        ("EXPORT", lines)
    }

    // ── Where they go ────────────────────────────────────────────────────────────────────

    /// `[CUTLINE]`: the name between the trim brackets. Nine cells wide.
    fn wordmark(canvas: &mut Canvas, x: i32, y: i32, band: i32) -> i32 {
        let font = canvas.font;
        let line = font.baseline(y, band);
        let (top, height) = (line + font.ch / 16, font.ch * 3 / 4);
        let stroke = (2 * font.px, font.cw / 2 + font.px);
        kit::bracket(
            canvas,
            (x + font.cw / 4, top, height),
            stroke,
            true,
            theme::TRIM,
        );
        canvas.text(x + font.cw, line, "CUTLINE", theme::INK, true);
        kit::bracket(
            canvas,
            (x + 9 * font.cw - font.cw / 4, top, height),
            stroke,
            false,
            theme::TRIM,
        );
        9 * font.cw
    }
    fn top_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, canvas: &mut Canvas, bar: R) {
        let font = canvas.font;
        canvas.fill(bar, theme::PANEL);
        canvas.fill(
            R::new(bar.x, bar.bottom() - font.px, bar.w, font.px),
            theme::RULE,
        );
        let band = bar.h - font.px;
        let items = self.top_items();
        let keys = kit::width(font, &items);
        let title = R::new(bar.x, bar.y, bar.w - keys, band);
        if self.io.live {
            let response = ui.interact(
                font.points(title),
                ui.id().with("title"),
                Sense::CLICK | Sense::DRAG,
            );
            if response.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if response.double_clicked() && !self.pip {
                self.io.events.push(Event::Act(Act::Maximize));
            }
        }
        let mut x = bar.x + 2 * font.cw;
        x += Self::wordmark(canvas, x, bar.y, band) + 2 * font.cw;
        if let Some(media) = &self.media {
            let line = font.baseline(bar.y, band);
            let room = ((title.right() - x) / font.cw - 2).max(0) as usize;
            let details = format!(
                "{}×{}  {:.3} FPS{}",
                media.width,
                media.height,
                media.rate.fps(),
                if media.hdr { "  HDR" } else { "" }
            );
            let name = media.name();
            let name_cells = name.chars().count();
            let details_cells = details.chars().count() + 3;
            if room >= 12 {
                let with_details = room >= name_cells.min(24) + details_cells;
                let name = cell::fit(&name, room - if with_details { details_cells } else { 0 });
                x += canvas.text(x, line, &name, theme::INK, false);
                if with_details {
                    canvas.text(x + 3 * font.cw, line, &details, theme::DIM, false);
                }
            }
        }
        kit::block(
            ui,
            canvas,
            &mut self.io,
            "top",
            (bar.right() - keys, bar.y, keys),
            &[Line {
                items,
                tight: false,
            }],
        );
    }
    /// Nothing is open: the window is an empty selection. The trim brackets frame the place
    /// a video goes, at the weight they have on the timeline. The top bar already names the
    /// application, so nothing here repeats it. While a file is being read, the indicator
    /// takes the place of the action.
    fn empty(&mut self, ui: &mut egui::Ui, canvas: &mut Canvas, space: R, time: f64) {
        let font = canvas.font;
        let width = 20 * font.cw;
        let lines = if self.loading {
            vec![Line::row(vec![
                kit::dim("READING  "),
                kit::text(Self::stepping(time)),
            ])]
        } else {
            vec![
                Line::row(vec![kit::toned(
                    "O",
                    "OPEN A VIDEO",
                    Tone::Primary,
                    Act::Open,
                )]),
                Line::row(vec![kit::dim("OR DROP A VIDEO HERE")]),
            ]
        };
        let height = kit::measure(font, &lines, width);
        let x = space.x + (space.w - width) / 2 / font.cw * font.cw;
        let y = space.y + (space.h - height) / 2 / font.px * font.px;
        let reach = 13 * font.cw;
        let rise = 6 * font.ch - height / 2;
        let stroke = (3 * font.px, 2 * font.cw);
        kit::bracket(
            canvas,
            (x - reach, y - rise, height + 2 * rise),
            stroke,
            true,
            theme::TRIM,
        );
        kit::bracket(
            canvas,
            (x + width + reach, y - rise, height + 2 * rise),
            stroke,
            false,
            theme::TRIM,
        );
        kit::block(ui, canvas, &mut self.io, "empty", (x, y, width), &lines);
    }
    /// Four cells, one lit, stepping eight times a second: something is being read.
    fn stepping(time: f64) -> String {
        let lit = (time * 8.0) as usize % 6;
        let lit = if lit > 3 { 6 - lit } else { lit };
        (0..4)
            .map(|cell| if cell == lit { '■' } else { '·' })
            .collect()
    }
    /// The window's own edge: a one-pixel rule, a resize zone on every side, and the grip.
    fn window_edge(&self, ctx: &egui::Context, ui: &mut egui::Ui, canvas: &mut Canvas, screen: R) {
        use egui::ResizeDirection as D;
        let font = canvas.font;
        canvas.frame(screen, 1, theme::RULE);
        let full = font.points(screen);
        let (edge, corner) = (5.0, 14.0);
        let zones = [
            (
                D::North,
                Rect::from_min_max(full.min, egui::pos2(full.right(), full.top() + edge)),
                egui::CursorIcon::ResizeVertical,
            ),
            (
                D::South,
                Rect::from_min_max(egui::pos2(full.left(), full.bottom() - edge), full.max),
                egui::CursorIcon::ResizeVertical,
            ),
            (
                D::West,
                Rect::from_min_max(full.min, egui::pos2(full.left() + edge, full.bottom())),
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                D::East,
                Rect::from_min_max(egui::pos2(full.right() - edge, full.top()), full.max),
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                D::NorthWest,
                Rect::from_min_size(full.min, egui::Vec2::splat(corner)),
                egui::CursorIcon::ResizeNwSe,
            ),
            (
                D::NorthEast,
                Rect::from_min_size(
                    egui::pos2(full.right() - corner, full.top()),
                    egui::Vec2::splat(corner),
                ),
                egui::CursorIcon::ResizeNeSw,
            ),
            (
                D::SouthWest,
                Rect::from_min_size(
                    egui::pos2(full.left(), full.bottom() - corner),
                    egui::Vec2::splat(corner),
                ),
                egui::CursorIcon::ResizeNeSw,
            ),
            (
                D::SouthEast,
                Rect::from_min_size(
                    full.max - egui::Vec2::splat(corner),
                    egui::Vec2::splat(corner),
                ),
                egui::CursorIcon::ResizeNwSe,
            ),
        ];
        for (direction, rect, cursor) in zones {
            let response = ui.interact(rect, ui.id().with(("edge", direction as u8)), Sense::DRAG);
            if response.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
            }
            if response.hovered() {
                ctx.set_cursor_icon(cursor);
            }
        }
    }
    fn compose(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, canvas: &mut Canvas, show: bool) {
        let font = canvas.font;
        let screen = font.snap(ctx.content_rect());
        let framed = !self.fullscreen && !ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let inner = if framed {
            screen.inset(font.px, font.px)
        } else {
            screen
        };
        let margin = 2 * font.cw;
        let content = inner.w - 2 * margin;
        self.io.live = self.dialog == Dialog::None;
        let mut after = Vec::new();

        // The bars retract in whole cells: motion stays on the same grid as everything else.
        let goal = if show { 1.0 } else { 0.0 };
        if self.reveal != goal {
            let step = ctx.input(|i| i.stable_dt).min(0.05) / 0.09;
            self.reveal = if goal > self.reveal {
                (self.reveal + step).min(1.0)
            } else {
                (self.reveal - step).max(0.0)
            };
            ctx.request_repaint();
        }
        let reveal = self.reveal;
        let retracted = |height: i32| {
            ((1.0 - reveal) * height as f32 / font.cw as f32).round() as i32 * font.cw
        };

        let status = self.status_lines((content / font.cw).max(0) as usize, ctx.input(|i| i.time));
        let status_height = if status.is_empty() {
            0
        } else {
            kit::measure(font, &status, content) + font.px
        };
        let (upper, keys) = self.deck_lines();
        let timeline_height = if self.media.is_some() && !self.pip {
            Timeline::height(font)
        } else {
            0
        };
        let pad = font.ch / 4;
        let deck_height = if self.media.is_some() {
            font.px
                + pad
                + kit::measure(font, &upper, content)
                + timeline_height
                + kit::measure(font, &keys, content)
                + pad
        } else {
            0
        };
        let top = R::new(
            inner.x,
            inner.y - retracted(font.row + font.px),
            inner.w,
            font.row + font.px,
        );
        let deck = R::new(
            inner.x,
            inner.bottom() - deck_height + retracted(deck_height),
            inner.w,
            deck_height,
        );
        let outer = canvas.narrow(inner);

        // The video. It fills the window under retracting bars; with the bars pinned, or
        // while cropping, it sits between them so the whole frame stays in view.
        let cropping = self.crop_before.is_some();
        if let (Some(media), Some(edit)) = (&self.media, &mut self.edit) {
            let selected = if cropping || self.original {
                Crop::full(media.width, media.height)
            } else {
                edit.crop
            };
            let mut space = inner;
            if cropping || self.pin {
                let above = inner.y + font.row + font.px + status_height;
                space = R::new(
                    inner.x,
                    above,
                    inner.w,
                    inner.bottom() - deck_height - above,
                );
                if cropping {
                    space = space.inset(margin, font.ch);
                }
            }
            let ratio = selected.width as f64 / selected.height as f64;
            let width = (space.w as f64).min(space.h as f64 * ratio).max(2.0);
            let (width, height) = (width.round() as i32, (width / ratio).round() as i32);
            let video = R::new(
                space.x + (space.w - width) / 2,
                space.y + (space.h - height) / 2,
                width,
                height,
            );
            if let Some(player) = &self.player {
                let renderer = player.renderer.clone();
                let dimensions = media.dimensions();
                ui.painter().add(egui::PaintCallback {
                    rect: font.points(video),
                    callback: Arc::new(eframe::egui_glow::CallbackFn::new(
                        move |info, _painter| renderer.lock().paint(info, dimensions, selected),
                    )),
                });
            }
            if cropping {
                let _ = self
                    .crop
                    .show(ui, canvas, font.points(video), media.dimensions(), edit);
            } else if self.io.live {
                let response = ui.interact(
                    font.points(inner),
                    ui.id().with("video"),
                    Sense::CLICK | Sense::DRAG,
                );
                if response.double_clicked() {
                    after.push(Act::Full);
                } else if response.clicked() {
                    after.push(Act::Play);
                }
                if response.drag_started() && self.pip {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
            }
        } else {
            self.empty(
                ui,
                canvas,
                R::new(
                    inner.x,
                    top.bottom() + status_height,
                    inner.w,
                    inner.bottom() - top.bottom() - status_height,
                ),
                ctx.input(|i| i.time),
            );
        }

        if deck_height > 0 && reveal > 0.0 {
            canvas.fill(deck, theme::PANEL);
            canvas.fill(R::new(deck.x, deck.y, deck.w, font.px), theme::RULE);
            if self.io.live {
                // The deck is solid: a click between its keys does not reach the video.
                ui.interact(font.points(deck), ui.id().with("deck"), Sense::CLICK);
            }
            let x = deck.x + margin;
            let mut y = deck.y + font.px + pad;
            y += kit::block(ui, canvas, &mut self.io, "deck", (x, y, content), &upper);
            let mut moved = None;
            if timeline_height > 0
                && let (Some(media), Some(edit)) = (&self.media, &mut self.edit)
            {
                moved = Some(self.timeline.show(
                    ui,
                    canvas,
                    self.io.live,
                    R::new(x, y, content, timeline_height),
                    media,
                    edit,
                    self.playback.position,
                    !self.playback.paused,
                ));
                y += timeline_height;
            }
            // A key's label starts one cell inside its block; pull the line out by that cell
            // so labels, not invisible block edges, sit on the margin.
            kit::block(
                ui,
                canvas,
                &mut self.io,
                "keys",
                (x - font.cw, y, content + font.cw),
                &keys,
            );
            if let Some(result) = moved {
                if result.editing {
                    self.run(|p| p.pause(true));
                }
                if let Some(before) = result.committed {
                    self.remember(before);
                }
                if let Some((time, exact)) = result.seek {
                    self.seek(time, exact);
                }
            }
            if framed {
                // The grip: three stepped rows of dots in the corner that resizes the window.
                let dot = 2 * font.px;
                for row in 1..=3 {
                    for column in 0..row {
                        canvas.fill(
                            R::new(
                                deck.right() - dot * 2 * (column + 1),
                                deck.bottom() - dot * 2 * (row - column),
                                dot,
                                dot,
                            ),
                            theme::DIM,
                        );
                    }
                }
            }
        }
        if reveal > 0.0 {
            self.top_bar(ctx, ui, canvas, top);
        }
        if status_height > 0 {
            let band = R::new(inner.x, top.bottom(), inner.w, status_height);
            canvas.fill(band, theme::PANEL);
            canvas.fill(
                R::new(band.x, band.bottom() - font.px, band.w, font.px),
                theme::RULE,
            );
            if self.io.live {
                ui.interact(font.points(band), ui.id().with("status"), Sense::CLICK);
            }
            kit::block(
                ui,
                canvas,
                &mut self.io,
                "status",
                (band.x + margin, band.y, content),
                &status,
            );
        }
        canvas.restore(outer);

        if self.dialog != Dialog::None {
            let cells = 76.min(screen.w / font.cw - 4);
            let body_cells = (cells - 4).max(8) as usize;
            let (title, lines) = self.sheet_lines(body_cells);
            let height = kit::measure(font, &lines, body_cells as i32 * font.cw);
            let veil = ui.interact(font.points(screen), ui.id().with("veil"), Sense::CLICK);
            let (sheet, body) = kit::sheet(canvas, screen, title, cells, height);
            ui.interact(font.points(sheet), ui.id().with("sheet"), Sense::CLICK);
            if veil.clicked() {
                after.push(Act::CloseDialog);
            }
            self.io.live = true;
            kit::block(
                ui,
                canvas,
                &mut self.io,
                "sheet",
                (body.x, body.y, body.w),
                &lines,
            );
        }
        if framed {
            self.window_edge(ctx, ui, canvas, screen);
        }
        for act in after {
            self.act(ctx, act);
        }
        self.events(ctx);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.messages(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && self.job.is_some() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.dialog = Dialog::Close;
        }
        if self.allow_close && self.job.is_none() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        for path in ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .take(1)
                .collect::<Vec<_>>()
        }) {
            self.open(path, ctx);
        }
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        if pointer != self.last_pointer {
            self.last_pointer = pointer;
            self.last_activity = Instant::now();
        }
        self.shortcuts(ctx);
        let screen = ctx.content_rect();
        let hovering = pointer.is_some_and(|p| p.y < 54.0 || p.y > screen.bottom() - 150.0);
        let show = self.media.is_none()
            || self.pin
            || hovering
            || self.last_activity.elapsed() < Duration::from_millis(1900)
            || self.crop_before.is_some()
            || self.io.entry.is_some()
            || self.dialog != Dialog::None
            || ctx.is_using_pointer();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::VOID))
            .show(ctx, |ui| {
                if self
                    .font
                    .as_ref()
                    .is_none_or(|font| !font.matches(ctx.pixels_per_point()))
                {
                    self.font = Some(Font::load(ctx));
                }
                if let Some(font) = self.font.take() {
                    let mut canvas = Canvas::new(&font);
                    self.compose(ctx, ui, &mut canvas, show);
                    canvas.finish(ui.painter());
                    self.font = Some(font);
                }
            });
        ctx.request_repaint_after(if self.loading {
            Duration::from_millis(60)
        } else if self.playback.paused {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(16)
        });
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save();
        if let Some(job) = &self.job {
            job.cancel();
        }
        if let Some(player) = &self.player {
            player.shutdown_renderer();
        }
    }
}

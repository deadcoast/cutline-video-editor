mod crop;
mod theme;
mod timeline;

use cutline::{
    media::{
        export::{self, Codec, Container, Encoder, Settings},
        player::{Playback, Player},
        probe::{self, Media},
        session,
    },
    model::{Crop, Edit, History, parse_timecode, timecode},
};
use eframe::egui::{
    self, Align2, Key, Order, Rect, RichText, Sense, Stroke, StrokeKind, Vec2, ViewportCommand,
    WindowLevel,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

enum Message {
    Picked(Option<PathBuf>),
    Probed(u64, Result<Media, String>),
    SavePath(Option<PathBuf>, Media, Edit, Settings),
}
#[derive(PartialEq)]
enum Dialog {
    None,
    Export,
    Trim,
    Help,
    Close,
}

pub struct App {
    player: Option<Player>,
    media: Option<Media>,
    edit: Option<Edit>,
    history: History,
    playback: Playback,
    timeline: timeline::Timeline,
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
    trim_start: String,
    trim_end: String,
    trim_error: Option<String>,
    last_poll: Instant,
    last_seek: Instant,
    pending_seek: Option<(f64, bool)>,
    save_at: Option<Instant>,
    loop_applied: Option<(f64, f64, bool)>,
    allow_close: bool,
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
            timeline: timeline::Timeline::default(),
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
            trim_start: String::new(),
            trim_end: String::new(),
            trim_error: None,
            last_poll: Instant::now(),
            last_seek: Instant::now(),
            pending_seek: None,
            save_at: None,
            loop_applied: None,
            allow_close: false,
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
    fn show_trim(&mut self) {
        if let Some(edit) = &self.edit {
            self.trim_start = timecode(edit.trim.start);
            self.trim_end = timecode(edit.trim.end);
            self.trim_error = None;
            self.dialog = Dialog::Trim;
        }
    }
    fn export_dialog(&mut self) {
        if self.job.is_some() || self.media.is_none() {
            return;
        }
        self.finish_crop(true);
        self.dialog = Dialog::Export;
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
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog != Dialog::None {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.crop_before.is_some() {
                self.finish_crop(false);
            } else if self.fullscreen {
                self.toggle_fullscreen(ctx);
            } else if self.pip {
                self.toggle_pip(ctx);
            }
        }
        if ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::text_edit::TextEditState::load(ctx, id).is_some())
        {
            return;
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
            self.last_activity = Instant::now();
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
        if (command_pressed(Key::Z, false) || command_pressed(Key::Z, true))
            && self.crop_before.is_none()
        {
            if let Some(edit) = &mut self.edit {
                if command_pressed(Key::Z, true) {
                    self.history.redo(edit);
                } else {
                    self.history.undo(edit);
                }
            }
            self.save_at = Some(Instant::now());
            self.loop_applied = None;
        }
        if command_pressed(Key::Y, false) && self.crop_before.is_none() {
            if let Some(edit) = &mut self.edit {
                self.history.redo(edit);
            }
            self.save_at = Some(Instant::now());
            self.loop_applied = None;
        }
        if pressed(Key::Space) {
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
        if pressed(Key::T) {
            self.show_trim();
        }
        if pressed(Key::M) {
            if self.volume > 0.0 {
                self.audible_volume = self.volume;
                self.volume = 0.0;
            } else {
                self.volume = self.audible_volume;
            }
            let v = self.volume;
            self.run(|p| p.volume(v));
        }
        if pressed(Key::ArrowLeft) || pressed(Key::ArrowRight) {
            let right = pressed(Key::ArrowRight);
            if shift {
                let time = (self.playback.position + if right { 5.0 } else { -5.0 })
                    .clamp(0.0, self.media.as_ref().map_or(0.0, |m| m.duration));
                self.seek(time, true);
            } else {
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
        if (pressed(Key::I) || pressed(Key::O))
            && let (Some(edit), Some(media)) = (&mut self.edit, &self.media)
        {
            let before = edit.clone();
            let t = media.snap(self.playback.position);
            if pressed(Key::I) {
                edit.trim.start = t.min(edit.trim.end - media.rate.frame_seconds()).max(0.0);
            } else {
                edit.trim.end = (t + media.rate.frame_seconds())
                    .max(edit.trim.start + media.rate.frame_seconds())
                    .min(media.duration);
            }
            self.remember(before);
        }
        if ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
        }) {
            self.last_activity = Instant::now();
        }
    }
    fn chrome(&mut self, ctx: &egui::Context, screen: Rect) {
        egui::Area::new("top-chrome".into())
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(screen.width() - 24.0);
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized(
                                    [76.0, 32.0],
                                    egui::Label::new(RichText::new("CUTLINE").strong().size(14.0))
                                        .sense(Sense::click_and_drag()),
                                )
                                .drag_started()
                            {
                                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                            }
                            if !self.pip
                                && theme::secondary_icon(
                                    ui,
                                    "Open",
                                    theme::Icon::Open,
                                    "Open video · Ctrl+O",
                                )
                                .clicked()
                            {
                                self.pick(ctx);
                            }
                            let control_count = if self.pip { 4.0 } else { 5.0 };
                            let control_width = control_count * 36.0 + (control_count - 1.0) * 8.0;
                            let name = self.media.as_ref().map_or(String::new(), Media::name);
                            let width = (ui.available_width() - control_width - 8.0).max(0.0);
                            let response = ui.add_sized(
                                [width, 32.0],
                                egui::Label::new(RichText::new(name.clone()).color(theme::MUTED))
                                    .truncate()
                                    .sense(Sense::click_and_drag()),
                            );
                            if !name.is_empty() {
                                response.clone().on_hover_text(name);
                            }
                            if response.drag_started() {
                                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                            }
                            if response.double_clicked() {
                                let maximized =
                                    ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                            }
                            ui.allocate_ui_with_layout(
                                egui::vec2(control_width, 32.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    if !self.pip
                                        && theme::icon_button(
                                            ui,
                                            theme::Icon::Help,
                                            "Keyboard shortcuts",
                                            "Keyboard shortcuts · F1",
                                        )
                                        .clicked()
                                    {
                                        self.dialog = Dialog::Help;
                                    }
                                    if theme::icon_button(
                                        ui,
                                        if self.pip {
                                            theme::Icon::Restore
                                        } else {
                                            theme::Icon::Pip
                                        },
                                        if self.pip {
                                            "Return to editor"
                                        } else {
                                            "Picture in picture"
                                        },
                                        if self.pip {
                                            "Return to editor · P"
                                        } else {
                                            "Picture in picture · P"
                                        },
                                    )
                                    .clicked()
                                    {
                                        self.toggle_pip(ctx);
                                    }
                                    if theme::icon_button(
                                        ui,
                                        if self.fullscreen {
                                            theme::Icon::ExitFullscreen
                                        } else {
                                            theme::Icon::Fullscreen
                                        },
                                        if self.fullscreen {
                                            "Exit fullscreen"
                                        } else {
                                            "Fullscreen"
                                        },
                                        if self.fullscreen {
                                            "Exit fullscreen · F / Escape"
                                        } else {
                                            "Fullscreen · F"
                                        },
                                    )
                                    .clicked()
                                    {
                                        self.toggle_fullscreen(ctx);
                                    }
                                    if theme::icon_button(
                                        ui,
                                        theme::Icon::Minimize,
                                        "Minimize",
                                        "Minimize window",
                                    )
                                    .clicked()
                                    {
                                        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                                    }
                                    if theme::icon_button(
                                        ui,
                                        theme::Icon::Close,
                                        "Close",
                                        "Close · Alt+F4",
                                    )
                                    .clicked()
                                    {
                                        ctx.send_viewport_cmd(ViewportCommand::Close);
                                    }
                                },
                            );
                        });
                    });
            });
    }
    fn controls(&mut self, ctx: &egui::Context, screen: Rect) {
        let width = (screen.width() - 40.0).min(1160.0);
        egui::Area::new("transport".into())
            .order(Order::Foreground)
            .anchor(Align2::CENTER_BOTTOM, [0.0, -16.0])
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0_f32, theme::EDGE))
                    .corner_radius(5)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width(width - 20.0);
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.spacing_mut().interact_size.y = 32.0;
                        ui.horizontal(|ui| {
                            if theme::icon_button(
                                ui,
                                if self.playback.paused {
                                    theme::Icon::Play
                                } else {
                                    theme::Icon::Pause
                                },
                                if self.playback.paused {
                                    "Play"
                                } else {
                                    "Pause"
                                },
                                "Play / pause · Space",
                            )
                            .clicked()
                            {
                                self.toggle_play();
                            }
                            if !self.pip && screen.width() > 650.0 {
                                if theme::icon_button(
                                    ui,
                                    theme::Icon::PreviousFrame,
                                    "Previous frame",
                                    "Previous frame · Left arrow",
                                )
                                .clicked()
                                {
                                    self.run(|p| p.command(&["frame-back-step"]));
                                }
                                if theme::icon_button(
                                    ui,
                                    theme::Icon::NextFrame,
                                    "Next frame",
                                    "Next frame · Right arrow",
                                )
                                .clicked()
                                {
                                    self.run(|p| p.command(&["frame-step"]));
                                }
                            }
                            ui.label(RichText::new(timecode(self.playback.position)).monospace());
                            if !self.pip && screen.width() > 950.0 {
                                ui.label(
                                    RichText::new(self.media.as_ref().map_or(String::new(), |m| {
                                        format!(
                                            "{} × {}  /  {:.3} fps",
                                            m.width,
                                            m.height,
                                            m.rate.fps()
                                        )
                                    }))
                                    .small()
                                    .color(theme::MUTED),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if self.pip {
                                        if theme::button(ui, "Return", "Return to editor · P")
                                            .clicked()
                                        {
                                            self.toggle_pip(ctx);
                                        }
                                        return;
                                    }
                                    if ui
                                        .add_enabled(
                                            self.job.is_none() && !self.loading,
                                            egui::Button::new(
                                                RichText::new("Export").color(theme::CANVAS),
                                            )
                                            .fill(theme::TRIM),
                                        )
                                        .on_hover_text("Export a new video · Ctrl+E")
                                        .clicked()
                                    {
                                        self.export_dialog();
                                    }
                                    if theme::button(
                                        ui,
                                        if self.crop_before.is_some() {
                                            "Done crop"
                                        } else {
                                            "Crop"
                                        },
                                        "Crop video · C",
                                    )
                                    .clicked()
                                    {
                                        self.toggle_crop();
                                    }
                                    if theme::button(
                                        ui,
                                        "Trim",
                                        "Enter precise start and end times · T",
                                    )
                                    .clicked()
                                    {
                                        self.show_trim();
                                    }
                                    if screen.width() > 820.0
                                        && theme::button(
                                            ui,
                                            if self.original { "Original" } else { "Edited" },
                                            "Compare source / cropped view · V",
                                        )
                                        .clicked()
                                    {
                                        self.original = !self.original;
                                    }
                                    ui.menu_button("More", |ui| {
                                        ui.set_min_width(220.0);
                                        ui.checkbox(&mut self.loop_selection, "Loop selection");
                                        ui.checkbox(&mut self.original, "Show original frame");
                                        ui.checkbox(&mut self.pin, "Keep controls visible");
                                        ui.separator();
                                        ui.label("Volume");
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut self.volume, 0.0..=100.0)
                                                    .suffix("%"),
                                            )
                                            .changed()
                                        {
                                            let v = self.volume;
                                            self.run(|p| p.volume(v));
                                        }
                                        ui.horizontal(|ui| {
                                            ui.label("Speed");
                                            for speed in [0.5, 1.0, 1.5, 2.0] {
                                                if ui
                                                    .selectable_value(
                                                        &mut self.speed,
                                                        speed,
                                                        format!("{speed}×"),
                                                    )
                                                    .clicked()
                                                {
                                                    self.run(|p| p.rate(speed));
                                                }
                                            }
                                        });
                                        ui.separator();
                                        ui.horizontal(|ui| {
                                            ui.label("Timeline");
                                            for (label, factor) in [("−", 0.5), ("+", 2.0)] {
                                                if theme::button(ui, label, "Zoom the timeline")
                                                    .clicked()
                                                    && let Some(m) = &self.media
                                                {
                                                    self.timeline.zoom_at(
                                                        factor,
                                                        self.playback.position,
                                                        m.duration,
                                                    );
                                                }
                                            }
                                            if theme::button(
                                                ui,
                                                &format!("Fit ({:.0}×)", self.timeline.zoom),
                                                "Show the entire video",
                                            )
                                            .clicked()
                                            {
                                                self.timeline.fit();
                                            }
                                        });
                                        ui.separator();
                                        if ui
                                            .add_enabled(
                                                self.history.can_undo()
                                                    && self.crop_before.is_none(),
                                                egui::Button::new("Undo · Ctrl+Z"),
                                            )
                                            .clicked()
                                        {
                                            if let Some(edit) = &mut self.edit {
                                                self.history.undo(edit);
                                            }
                                            self.save_at = Some(Instant::now());
                                            self.loop_applied = None;
                                            ui.close();
                                        }
                                        if ui
                                            .add_enabled(
                                                self.history.can_redo()
                                                    && self.crop_before.is_none(),
                                                egui::Button::new("Redo · Ctrl+Shift+Z"),
                                            )
                                            .clicked()
                                        {
                                            if let Some(edit) = &mut self.edit {
                                                self.history.redo(edit);
                                            }
                                            self.save_at = Some(Instant::now());
                                            self.loop_applied = None;
                                            ui.close();
                                        }
                                        if ui.button("Reset trim").clicked() {
                                            if let (Some(edit), Some(media)) =
                                                (&mut self.edit, &self.media)
                                            {
                                                let before = edit.clone();
                                                edit.trim = cutline::model::Trim {
                                                    start: 0.0,
                                                    end: media.duration,
                                                };
                                                self.remember(before);
                                            }
                                            ui.close();
                                        }
                                        if let Some(m) = &self.media {
                                            ui.separator();
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} × {} · {:.3} fps{}",
                                                    m.width,
                                                    m.height,
                                                    m.rate.fps(),
                                                    if m.hdr { " · HDR" } else { "" }
                                                ))
                                                .small()
                                                .color(theme::MUTED),
                                            );
                                        }
                                    })
                                    .response
                                    .on_hover_text("Playback, comparison and timeline settings");
                                },
                            );
                        });
                        if !self.pip
                            && let (Some(media), Some(edit)) = (&self.media, &mut self.edit)
                        {
                            let result = self.timeline.show(
                                ui,
                                media,
                                edit,
                                self.playback.position,
                                !self.playback.paused,
                            );
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
                    });
            });
    }
    fn crop_controls(&mut self, ctx: &egui::Context) {
        egui::Area::new("crop-controls".into())
            .order(Order::Foreground)
            .anchor(Align2::CENTER_TOP, [0.0, 62.0])
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0_f32, theme::EDGE))
                    .corner_radius(5)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width((ctx.content_rect().width() - 48.0).min(760.0));
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Crop").strong().color(theme::CROP));
                            let label = match self.crop.ratio {
                                None => "Free",
                                Some(r) if (r - 16.0 / 9.0).abs() < 0.001 => "16:9",
                                Some(r) if (r - 9.0 / 16.0).abs() < 0.001 => "9:16",
                                Some(r) if (r - 1.0).abs() < 0.001 => "1:1",
                                Some(r) if (r - 4.0 / 3.0).abs() < 0.001 => "4:3",
                                _ => "21:9",
                            };
                            egui::ComboBox::from_id_salt("aspect")
                                .selected_text(label)
                                .width(85.0)
                                .show_ui(ui, |ui| {
                                    for (label, ratio) in [
                                        ("Free", None),
                                        ("16:9", Some(16.0 / 9.0)),
                                        ("9:16", Some(9.0 / 16.0)),
                                        ("1:1", Some(1.0)),
                                        ("4:3", Some(4.0 / 3.0)),
                                        ("21:9", Some(21.0 / 9.0)),
                                    ] {
                                        if ui
                                            .selectable_value(&mut self.crop.ratio, ratio, label)
                                            .clicked()
                                            && let (Some(ratio), Some(edit), Some(media)) =
                                                (ratio, &mut self.edit, &self.media)
                                        {
                                            edit.crop =
                                                edit.crop.fit_ratio(ratio, media.dimensions());
                                        }
                                    }
                                });
                            if let (Some(edit), Some(media)) = (&mut self.edit, &self.media) {
                                let mut c = edit.crop;
                                let old = c;
                                for (label, value) in [
                                    ("X", &mut c.x),
                                    ("Y", &mut c.y),
                                    ("W", &mut c.width),
                                    ("H", &mut c.height),
                                ] {
                                    ui.label(label);
                                    ui.add_sized(
                                        [54.0, 28.0],
                                        egui::DragValue::new(value).speed(2.0).range(0..=32768),
                                    );
                                }
                                if let Some(ratio) = self.crop.ratio {
                                    if c.width != old.width {
                                        c.height = (c.width as f64 / ratio).round() as u32;
                                    } else if c.height != old.height {
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
                                if theme::button(ui, "Reset", "Use the entire frame").clicked() {
                                    edit.crop = Crop::full(media.width, media.height);
                                    self.crop.ratio = None;
                                }
                            }
                            if theme::button(ui, "Cancel", "Discard this crop change · Escape")
                                .clicked()
                            {
                                self.finish_crop(false);
                            }
                            if theme::primary(ui, "Done").clicked() {
                                self.finish_crop(true);
                            }
                        });
                    });
            });
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.dialog == Dialog::None {
            return;
        }
        let response=egui::Modal::new("dialog".into()).show(ctx,|ui|{
            ui.set_width(420.0_f32.min(ctx.content_rect().width()-64.0));
            match self.dialog {
                Dialog::Trim=>{
                    ui.heading("Trim precisely");ui.label("The end is exclusive. Time format: hh:mm:ss.mmm");ui.add_space(8.0);
                    egui::Grid::new("trim-fields").num_columns(2).show(ui,|ui|{
                        ui.label("Start");ui.text_edit_singleline(&mut self.trim_start);ui.end_row();
                        ui.label("End");ui.text_edit_singleline(&mut self.trim_end);ui.end_row();
                    });
                    if let Some(error)=&self.trim_error{ui.colored_label(theme::DANGER,error);}
                    ui.add_space(12.0);ui.horizontal(|ui|{
                        if theme::button(ui,"Cancel","Close without changing the trim").clicked(){self.dialog=Dialog::None;}
                        if theme::primary(ui,"Apply trim").clicked(){
                            if let (Some(start),Some(end),Some(media),Some(edit))=(parse_timecode(&self.trim_start),parse_timecode(&self.trim_end),&self.media,&mut self.edit) {
                                let trim=cutline::model::Trim{start:media.snap(start),end:media.snap(end)};
                                if start<=media.duration && end<=media.duration+0.001 && trim.valid(media.duration) {
                                    let before=edit.clone();edit.trim=trim;self.remember(before);self.seek(trim.start,true);self.dialog=Dialog::None;
                                }else{self.trim_error=Some("End must follow start, within the video duration.".into());}
                            }else{self.trim_error=Some("Enter valid start and end times.".into());}
                        }
                    });
                }
                Dialog::Export=>{
                    ui.heading("Export your selection");ui.add_space(8.0);
                    if let (Some(media),Some(edit),Some(settings))=(&self.media,&self.edit,&mut self.settings) {
                        ui.label(RichText::new(format!("{} × {}  ·  {:.3} fps  ·  {}",edit.crop.width,edit.crop.height,media.rate.fps(),timecode(edit.trim.duration()))).monospace().color(theme::TRIM));
                        ui.label("Source frame timing is preserved. No frame-rate cap.");ui.add_space(12.0);
                        egui::Grid::new("export-options").num_columns(2).spacing([18.0,12.0]).show(ui,|ui|{
                            ui.label("Format");egui::ComboBox::from_id_salt("format").selected_text(settings.container.extension().to_uppercase()).show_ui(ui,|ui|{
                                for container in [Container::Mp4,Container::Mkv,Container::Mov,Container::Webm] {
                                    if ui.selectable_value(&mut settings.container,container,container.extension().to_uppercase()).clicked(){
                                        if container==Container::Webm {settings.codec=Codec::Vp9;}
                                        else if settings.codec==Codec::Vp9 && container!=Container::Mkv {settings.codec=if media.hdr{Codec::Hevc}else{Codec::H264};}
                                    }
                                }
                            });ui.end_row();
                            ui.label("Video");egui::ComboBox::from_id_salt("codec").selected_text(settings.codec.label()).show_ui(ui,|ui|{
                                for codec in [Codec::H264,Codec::Hevc,Codec::Vp9] {
                                    let enabled=!(media.hdr&&codec==Codec::H264) && if settings.container==Container::Webm {codec==Codec::Vp9}else{codec!=Codec::Vp9||settings.container==Container::Mkv};
                                    ui.add_enabled_ui(enabled,|ui|{ui.selectable_value(&mut settings.codec,codec,codec.label());});
                                }
                            });ui.end_row();
                            ui.label("Encoder");egui::ComboBox::from_id_salt("encoder").selected_text(settings.encoder.label()).show_ui(ui,|ui|{
                                for encoder in [Encoder::Auto,Encoder::Cpu,Encoder::Nvidia,Encoder::Intel,Encoder::Amd] {
                                    ui.add_enabled_ui(settings.codec!=Codec::Vp9||matches!(encoder,Encoder::Auto|Encoder::Cpu),|ui|{ui.selectable_value(&mut settings.encoder,encoder,encoder.label());});
                                }
                            });ui.end_row();
                            ui.label("Quality");ui.add(egui::Slider::new(&mut settings.quality,14..=30).text("CQ")).on_hover_text("Lower = higher quality and larger files. 20 is a balanced default.");ui.end_row();
                            ui.label("Audio");ui.checkbox(&mut settings.mute,"Remove audio");ui.end_row();
                        });
                        ui.add_space(8.0);ui.label(RichText::new(format!("{} audio track(s) · subtitles and chapters are not exported.",media.audio_tracks)).small().color(theme::MUTED));
                        if media.hdr {ui.colored_label(theme::CROP,"HDR / 10-bit retained. Preview is tone-mapped for this window.");}
                        if media.hdr_metadata {ui.colored_label(theme::DANGER,"Dynamic HDR metadata is present. This export path cannot preserve it, so export is disabled.");}
                        ui.add_space(12.0);
                        let snapshot=(media.clone(),edit.clone(),settings.clone());
                        let can_export=!media.hdr_metadata && !self.picker;
                        ui.horizontal(|ui|{
                            if theme::button(ui,"Cancel","Return to the video").clicked(){self.dialog=Dialog::None;}
                            if ui.add_enabled(can_export,egui::Button::new(RichText::new("Choose destination…").color(theme::CANVAS)).fill(theme::TRIM)).clicked(){
                                self.picker=true;let tx=self.tx.clone();let ctx=ctx.clone();
                                std::thread::spawn(move||{
                                    let (media,edit,settings)=snapshot;
                                    let suggestion=export::suggested_path(&media,settings.container);
                                    let path=rfd::FileDialog::new().set_title("Export video — choose a new filename")
                                        .set_directory(suggestion.parent().unwrap_or(std::path::Path::new(".")))
                                        .set_file_name(suggestion.file_name().unwrap_or_default().to_string_lossy())
                                        .add_filter("Video",&[settings.container.extension()]).save_file();
                                    let _=tx.send(Message::SavePath(path,media,edit,settings));ctx.request_repaint();
                                });
                            }
                        });
                    }
                }
                Dialog::Help=>{
                    ui.heading("Stay with the video");ui.add_space(10.0);
                    egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-180.0).max(100.0)).show(ui,|ui|{
                    egui::Grid::new("shortcuts").spacing([24.0,8.0]).show(ui,|ui|{
                        for (keys,action) in [("Space","Play / pause"),("← / →","Previous / next frame"),("Shift + ← / →","Seek five seconds"),("I / O","Set start / end at playhead"),("Home / End","Jump to trim boundaries"),("T / C","Precise trim / crop"),("V","Compare original / edited"),("L","Loop selection"),("F / P","Fullscreen / picture-in-picture"),("M","Mute / unmute"),("Ctrl + O / E","Open / export"),("Ctrl + Z / Shift + Z","Undo / redo"),("Ctrl + wheel","Zoom timeline"),("Shift + drag","Move trim range"),("Esc","Cancel crop / leave viewing mode")] {
                            ui.label(RichText::new(keys).monospace().color(theme::TRIM));ui.label(action);ui.end_row();
                        }
                    });});ui.add_space(12.0);if theme::primary(ui,"Back to video").clicked(){self.dialog=Dialog::None;}
                }
                Dialog::Close=>{
                    ui.heading("An export is still running");ui.label("Closing now cancels the export and removes its partial file. Your source stays unchanged.");ui.add_space(12.0);
                    ui.horizontal(|ui|{
                        if theme::primary(ui,"Keep exporting").clicked(){self.dialog=Dialog::None;}
                        if theme::button(ui,"Cancel export and close","Stop the export before closing").clicked(){if let Some(job)=&self.job{job.cancel();}self.allow_close=true;self.dialog=Dialog::None;}
                    });
                }
                Dialog::None=>{}
            }
        });
        if response.should_close() {
            self.dialog = Dialog::None;
        }
    }
    fn feedback(&mut self, ctx: &egui::Context) {
        if self.job.is_some() {
            egui::Area::new("export-progress".into())
                .order(Order::Foreground)
                .anchor(Align2::CENTER_TOP, [0.0, 64.0])
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(theme::SURFACE)
                        .stroke(Stroke::new(1.0_f32, theme::EDGE))
                        .corner_radius(5)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.set_width(380.0_f32.min(ctx.content_rect().width() - 56.0));
                            ui.horizontal(|ui| {
                                ui.label(&self.export_status);
                                if theme::button(ui, "Cancel", "Stop export; keep the original")
                                    .clicked()
                                {
                                    if let Some(job) = &self.job {
                                        job.cancel();
                                    }
                                    self.export_status = "Cancelling…".into();
                                }
                            });
                            ui.add(egui::ProgressBar::new(self.progress).text(format!(
                                "{:.0}%  ·  {}",
                                self.progress * 100.0,
                                self.export_speed
                            )));
                        });
                });
        }
        if self.error.is_some() || self.notice.is_some() {
            egui::Area::new("feedback".into())
                .order(Order::Foreground)
                .anchor(
                    Align2::CENTER_TOP,
                    [0.0, if self.job.is_some() { 156.0 } else { 64.0 }],
                )
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(theme::SURFACE)
                        .stroke(Stroke::new(
                            1.0_f32,
                            if self.error.is_some() {
                                theme::DANGER
                            } else {
                                theme::EDGE
                            },
                        ))
                        .corner_radius(5)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.set_max_width(560.0_f32.min(ctx.content_rect().width() - 56.0));
                            let text = self
                                .error
                                .as_ref()
                                .or(self.notice.as_ref())
                                .cloned()
                                .unwrap_or_default();
                            ui.label(RichText::new(text).color(if self.error.is_some() {
                                theme::DANGER
                            } else {
                                theme::INK
                            }));
                            ui.horizontal(|ui| {
                                if let Some(path) = self.completed.clone() {
                                    if theme::button(
                                        ui,
                                        "Open exported video",
                                        "View the completed output",
                                    )
                                    .clicked()
                                    {
                                        self.open(path.clone(), ctx);
                                        self.completed = None;
                                    }
                                    if theme::button(
                                        ui,
                                        "Show in folder",
                                        "Locate the exported file",
                                    )
                                    .clicked()
                                    {
                                        #[cfg(windows)]
                                        {
                                            let _ = std::process::Command::new("explorer.exe")
                                                .arg(format!("/select,{}", path.display()))
                                                .spawn();
                                        }
                                        #[cfg(not(windows))]
                                        {
                                            let _ = std::process::Command::new("xdg-open")
                                                .arg(
                                                    path.parent()
                                                        .unwrap_or(std::path::Path::new(".")),
                                                )
                                                .spawn();
                                        }
                                    }
                                }
                                if theme::button(ui, "Dismiss", "Dismiss this message").clicked() {
                                    self.error = None;
                                    self.notice = None;
                                    self.completed = None;
                                }
                            });
                        });
                });
        }
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
            || egui::Popup::is_any_open(ctx)
            || ctx.is_using_pointer();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::CANVAS))
            .show(ctx, |ui| {
                let area = ui.max_rect();
                if let (Some(media), Some(edit)) = (&self.media, &mut self.edit) {
                    let cropping = self.crop_before.is_some();
                    let selected = if cropping || self.original {
                        Crop::full(media.width, media.height)
                    } else {
                        edit.crop
                    };
                    let available = if cropping {
                        Rect::from_min_max(
                            area.min + egui::vec2(30.0, 128.0),
                            area.max - egui::vec2(30.0, 148.0),
                        )
                    } else {
                        area
                    };
                    let ratio = selected.width as f32 / selected.height as f32;
                    let width = available.width().min(available.height() * ratio).max(1.0);
                    let video = Rect::from_center_size(
                        available.center(),
                        egui::vec2(width, width / ratio),
                    );
                    if let Some(player) = &self.player {
                        let renderer = player.renderer.clone();
                        let dimensions = media.dimensions();
                        ui.painter().add(egui::PaintCallback {
                            rect: video,
                            callback: Arc::new(eframe::egui_glow::CallbackFn::new(
                                move |info, _painter| {
                                    renderer.lock().paint(info, dimensions, selected)
                                },
                            )),
                        });
                    }
                    if cropping {
                        let _ = self.crop.show(ui, video, media.dimensions(), edit);
                    } else {
                        let response =
                            ui.interact(area, ui.id().with("video"), Sense::click_and_drag());
                        if response.double_clicked() {
                            self.toggle_fullscreen(ctx);
                        } else if response.clicked() {
                            self.toggle_play();
                        }
                        if response.drag_started() && self.pip {
                            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                        }
                    }
                } else if !self.loading {
                    // One layout owns the action and its hint, so text metrics and DPI cannot
                    // move independently painted lines into the button's hit area.
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(Rect::from_center_size(
                            area.center(),
                            egui::vec2(280.0, 80.0),
                        )),
                        |ui| {
                            ui.vertical_centered(|ui| {
                                ui.spacing_mut().item_spacing.y = 16.0;
                                if theme::primary_icon(
                                    ui,
                                    "Open video",
                                    theme::Icon::Open,
                                    "Choose a video file · Ctrl+O",
                                )
                                .clicked()
                                {
                                    self.pick(ctx);
                                }
                                ui.label(
                                    RichText::new("or drop a video here")
                                        .size(13.0)
                                        .color(theme::MUTED),
                                );
                            });
                        },
                    );
                }
                if self.loading {
                    egui::Area::new("loading".into())
                        .order(Order::Foreground)
                        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                        .show(ctx, |ui| {
                            egui::Frame::new()
                                .fill(theme::SURFACE)
                                .corner_radius(4)
                                .inner_margin(16)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Reading video…");
                                    });
                                });
                        });
                }
                ui.painter().rect_stroke(
                    area.shrink(0.5),
                    0.0,
                    Stroke::new(1.0_f32, theme::EDGE),
                    StrokeKind::Inside,
                );
            });
        if show {
            self.chrome(ctx, screen);
            if self.media.is_some() {
                self.controls(ctx, screen);
            }
        }
        if self.crop_before.is_some() {
            self.crop_controls(ctx);
        }
        self.feedback(ctx);
        self.dialogs(ctx);
        if !self.fullscreen {
            egui::Area::new("resize-corner".into())
                .order(Order::Foreground)
                .anchor(Align2::RIGHT_BOTTOM, [0.0, 0.0])
                .show(ctx, |ui| {
                    let (_, response) = ui.allocate_exact_size(Vec2::splat(13.0), Sense::drag());
                    if response.drag_started() {
                        ctx.send_viewport_cmd(ViewportCommand::BeginResize(
                            egui::ResizeDirection::SouthEast,
                        ));
                    }
                    if response.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                    }
                });
        }
        ctx.request_repaint_after(if self.playback.paused {
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

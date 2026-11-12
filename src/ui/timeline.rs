use super::theme;
use cutline::{
    media::probe::Media,
    model::{Edit, Trim, timecode},
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

/// Half-width of the pointer zone around a trim bracket, in logical pixels.
const GRAB: f32 = 13.0;
/// Closest spacing of ruler ticks, in logical pixels.
const TICK_GAP: f64 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Head,
    Tail,
    Playhead,
    Selection,
}
struct Drag {
    part: Part,
    before: Edit,
    origin: f64,
}
pub struct Timeline {
    pub zoom: f64,
    pub offset: f64,
    drag: Option<Drag>,
    /// Playhead time at the previous frame; lets a zoomed view page along with playback.
    last_position: f64,
}
impl Default for Timeline {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: 0.0,
            drag: None,
            last_position: 0.0,
        }
    }
}
#[derive(Default)]
pub struct Response {
    pub seek: Option<(f64, bool)>,
    pub committed: Option<Edit>,
    pub editing: bool,
}

/// Which part a press at `x` takes. Brackets that overlap on screen (a short selection in a
/// long video) split at their midpoint, so the end bracket stays reachable from its outer side.
fn grab(x: f32, head: f32, tail: f32, shift: bool) -> Part {
    match ((x - head).abs() < GRAB, (x - tail).abs() < GRAB) {
        (true, true) if x < (head + tail) / 2.0 => Part::Head,
        (true, true) | (false, true) => Part::Tail,
        (true, false) => Part::Head,
        (false, false) if shift && x >= head && x <= tail => Part::Selection,
        (false, false) => Part::Playhead,
    }
}

/// Seconds between ruler ticks for a view showing `span` seconds across `width` pixels.
/// Constant-rate video uses one tick per frame once frames are far enough apart to see.
fn tick_step(span: f64, width: f64, frame: Option<f64>) -> f64 {
    const STEPS: [f64; 22] = [
        0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0,
        120.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 18000.0,
    ];
    let least = span / width.max(1.0) * TICK_GAP;
    if let Some(frame) = frame
        && frame >= least
        && frame < least * 5.0
    {
        return frame;
    }
    STEPS
        .into_iter()
        .find(|step| *step >= least)
        .unwrap_or_else(|| (least / 36000.0).ceil() * 36000.0)
}

impl Timeline {
    pub fn fit(&mut self) {
        self.zoom = 1.0;
        self.offset = 0.0;
    }
    pub fn zoom_at(&mut self, factor: f64, position: f64, duration: f64) {
        let old_span = duration / self.zoom;
        let fraction = ((position - self.offset) / old_span).clamp(0.0, 1.0);
        self.zoom = (self.zoom * factor).clamp(1.0, 1024.0);
        let span = duration / self.zoom;
        self.offset = (position - span * fraction).clamp(0.0, (duration - span).max(0.0));
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        media: &Media,
        edit: &mut Edit,
        position: f64,
        playing: bool,
    ) -> Response {
        let mut result = Response::default();
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 58.0),
            Sense::click_and_drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Slider,
                true,
                "Trim timeline. I sets the start; O sets the end. Open Trim for numeric entry.",
            )
        });
        let rail = Rect::from_min_max(
            rect.min + egui::vec2(12.0, 22.0),
            rect.max - egui::vec2(12.0, 8.0),
        );
        let span = media.duration / self.zoom;
        let visible = |offset: f64, time: f64| time >= offset && time <= offset + span;
        if playing
            && self.drag.is_none()
            && visible(self.offset, self.last_position)
            && !visible(self.offset, position)
        {
            // Playback ran off the edge of a zoomed view: turn the page.
            self.offset = position - span * 0.05;
        }
        self.last_position = position;
        self.offset = self.offset.clamp(0.0, (media.duration - span).max(0.0));
        let time_to_x = |t: f64| rail.left() + ((t - self.offset) / span) as f32 * rail.width();
        let x_to_time = |x: f32| {
            ((x - rail.left()) as f64 / rail.width() as f64 * span + self.offset)
                .clamp(0.0, media.duration)
        };
        let head = time_to_x(edit.trim.start);
        let tail = time_to_x(edit.trim.end);
        if response.drag_started()
            && let Some(pointer) = ui.input(|i| i.pointer.press_origin())
        {
            let part = grab(pointer.x, head, tail, ui.input(|i| i.modifiers.shift));
            self.drag = Some(Drag {
                part,
                before: edit.clone(),
                origin: x_to_time(pointer.x),
            });
        }
        if let Some(pointer) = response.interact_pointer_pos() {
            if let Some(drag) = &self.drag {
                let target = media.snap(x_to_time(pointer.x));
                let minimum = media.rate.frame_seconds().min(media.duration);
                match drag.part {
                    Part::Head => {
                        edit.trim.start = target.min(edit.trim.end - minimum).max(0.0);
                        result.seek = Some((edit.trim.start, false));
                    }
                    Part::Tail => {
                        edit.trim.end = target.max(edit.trim.start + minimum).min(media.duration);
                        result.seek = Some(((edit.trim.end - minimum).max(edit.trim.start), false));
                    }
                    Part::Playhead => result.seek = Some((target, false)),
                    Part::Selection => {
                        let length = drag.before.trim.duration();
                        let start = media
                            .snap(drag.before.trim.start + target - drag.origin)
                            .clamp(0.0, media.duration - length);
                        edit.trim = Trim {
                            start,
                            end: start + length,
                        };
                        result.seek = Some((start, false));
                    }
                }
            } else if response.clicked() {
                result.seek = Some((media.snap(x_to_time(pointer.x)), true));
            }
        }
        if response.drag_stopped()
            && let Some(drag) = self.drag.take()
        {
            result.committed = Some(drag.before);
            let target = match drag.part {
                Part::Head | Part::Selection => edit.trim.start,
                Part::Tail => (edit.trim.end - media.rate.frame_seconds()).max(edit.trim.start),
                Part::Playhead => response
                    .interact_pointer_pos()
                    .map(|p| media.snap(x_to_time(p.x)))
                    .unwrap_or(position),
            };
            result.seek = Some((target, true));
        }
        result.editing = self.drag.is_some();
        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            result.seek = Some((
                (position + media.rate.frame_seconds()).min(media.duration),
                true,
            ));
        }
        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            result.seek = Some(((position - media.rate.frame_seconds()).max(0.0), true));
        }
        let p = ui.painter_at(rect);
        p.rect_filled(rail, 2.0, Color32::from_rgb(46, 51, 57));
        let selected = Rect::from_min_max(
            Pos2::new(time_to_x(edit.trim.start).max(rail.left()), rail.top()),
            Pos2::new(time_to_x(edit.trim.end).min(rail.right()), rail.bottom()),
        );
        if selected.is_positive() {
            p.rect_filled(selected, 0.0, theme::TRIM.gamma_multiply(0.18));
            p.line_segment(
                [selected.left_top(), selected.right_top()],
                Stroke::new(1.0_f32, theme::TRIM),
            );
            p.line_segment(
                [selected.left_bottom(), selected.right_bottom()],
                Stroke::new(1.0_f32, theme::TRIM),
            );
        }
        // The ruler measures source time: ticks stay on their timestamps through zoom and pan.
        let step = tick_step(
            span,
            rail.width() as f64,
            (!media.variable_rate).then(|| media.rate.frame_seconds()),
        );
        let first = (self.offset / step).ceil() as i64;
        let last = ((self.offset + span) / step).floor() as i64;
        for index in first..=last {
            let x = time_to_x(index as f64 * step);
            p.line_segment(
                [
                    Pos2::new(x, rail.center().y - 2.0),
                    Pos2::new(x, rail.center().y + if index % 5 == 0 { 4.0 } else { 2.0 }),
                ],
                Stroke::new(1.0_f32, theme::MUTED.gamma_multiply(0.35)),
            );
        }
        for (time, is_head) in [(edit.trim.start, true), (edit.trim.end, false)] {
            let x = time_to_x(time);
            if x >= rail.left() - 1.0 && x <= rail.right() + 1.0 {
                let handle = Rect::from_center_size(
                    Pos2::new(x, rail.center().y),
                    egui::vec2(18.0, rail.height() + 8.0),
                );
                p.rect_filled(handle, 2.0, theme::TRIM);
                let direction = if is_head { 1.0 } else { -1.0 };
                for y in [-7.0, 7.0] {
                    p.line_segment(
                        [
                            Pos2::new(x - direction * 2.0, rail.center().y + y),
                            Pos2::new(x + direction * 3.0, rail.center().y + y),
                        ],
                        Stroke::new(1.5_f32, theme::CANVAS),
                    );
                }
                p.line_segment(
                    [
                        Pos2::new(x - direction * 2.0, rail.center().y - 7.0),
                        Pos2::new(x - direction * 2.0, rail.center().y + 7.0),
                    ],
                    Stroke::new(1.5_f32, theme::CANVAS),
                );
            }
        }
        let x = time_to_x(position);
        if rail.x_range().contains(x) {
            p.line_segment(
                [
                    Pos2::new(x, rail.top() - 7.0),
                    Pos2::new(x, rail.bottom() + 6.0),
                ],
                Stroke::new(1.5_f32, theme::INK),
            );
            p.circle_filled(Pos2::new(x, rail.top() - 8.0), 3.0, theme::INK);
        }
        p.text(
            Pos2::new(rail.left(), rect.top()),
            Align2::LEFT_TOP,
            format!("IN  {}", timecode(edit.trim.start)),
            FontId::monospace(11.0),
            theme::TRIM,
        );
        p.text(
            Pos2::new(rail.right(), rect.top()),
            Align2::RIGHT_TOP,
            format!("{}  OUT", timecode(edit.trim.end)),
            FontId::monospace(11.0),
            theme::TRIM,
        );
        if rail.width() > 420.0 {
            p.text(
                Pos2::new(rail.center().x, rect.top()),
                Align2::CENTER_TOP,
                if self.zoom > 1.0 {
                    format!(
                        "{} – {}  ·  {:.0}×",
                        timecode(self.offset),
                        timecode(self.offset + span),
                        self.zoom
                    )
                } else {
                    format!("{} selected", timecode(edit.trim.duration()))
                },
                FontId::monospace(11.0),
                theme::MUTED,
            );
        }
        if response.has_focus() {
            p.rect_stroke(
                rect,
                3.0,
                Stroke::new(1.0_f32, theme::CROP),
                StrokeKind::Inside,
            );
        }
        if response.hovered() {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                let handle = (pos.x - head).abs() < GRAB || (pos.x - tail).abs() < GRAB;
                ui.ctx().set_cursor_icon(if handle {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::PointingHand
                });
            }
            // egui turns Ctrl+wheel (and pinch) into a zoom factor and withholds it from the
            // scroll delta, so zoom has to be read from `zoom_delta`.
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if zoom != 1.0 {
                self.zoom_at(zoom as f64, position, media.duration);
            } else {
                // Shift+wheel and trackpads report horizontal travel.
                let travel = if scroll.x.abs() > scroll.y.abs() {
                    scroll.x
                } else {
                    scroll.y
                };
                if travel != 0.0 {
                    self.offset = (self.offset - travel as f64 / 500.0 * span)
                        .clamp(0.0, (media.duration - span).max(0.0));
                }
            }
        }
        response.on_hover_text("Drag the brackets to trim · Click to seek · Shift-drag to move the selection · Ctrl+wheel to zoom");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_brackets_stay_separately_reachable() {
        // Two seconds selected in a two-hour video: both brackets land on the same pixel.
        assert_eq!(grab(396.0, 400.0, 400.2, false), Part::Head);
        assert_eq!(grab(404.0, 400.0, 400.2, false), Part::Tail);
        assert_eq!(grab(105.0, 100.0, 900.0, false), Part::Head);
        assert_eq!(grab(895.0, 100.0, 900.0, false), Part::Tail);
        assert_eq!(grab(500.0, 100.0, 900.0, true), Part::Selection);
        assert_eq!(grab(500.0, 100.0, 900.0, false), Part::Playhead);
        assert_eq!(grab(950.0, 100.0, 900.0, true), Part::Playhead);
    }
    #[test]
    fn ruler_ticks_are_readable_and_frame_true_when_zoomed() {
        for (span, width) in [
            (6.0, 1000.0),
            (7200.0, 700.0),
            (0.25, 1100.0),
            (90000.0, 600.0),
        ] {
            let step = tick_step(span, width, None);
            assert!(
                step / span * width >= TICK_GAP - 1e-9,
                "{span}s over {width}px"
            );
            assert!(span / step <= width / TICK_GAP + 1.0);
        }
        let frame = 1001.0 / 120000.0;
        assert_eq!(tick_step(0.5, 1000.0, Some(frame)), frame);
        assert_ne!(tick_step(600.0, 1000.0, Some(frame)), frame);
    }
    #[test]
    fn zoom_keeps_the_playhead_in_place() {
        let mut timeline = Timeline::default();
        timeline.zoom_at(4.0, 30.0, 60.0);
        let span = 60.0 / timeline.zoom;
        assert!((timeline.offset + span / 2.0 - 30.0).abs() < 1e-9);
        timeline.zoom_at(1.0 / 1024.0, 30.0, 60.0);
        assert_eq!((timeline.zoom, timeline.offset), (1.0, 0.0));
    }
}

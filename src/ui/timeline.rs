//! The timeline: a ruler of source time, the rail, and the two trim brackets.
use super::{
    cell::{Canvas, Font, R},
    kit, theme,
};
use cutline::{
    media::probe::Media,
    model::{Edit, Trim, timecode},
};
use eframe::egui::{self, Sense};

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
fn grab(x: f32, head: f32, tail: f32, reach: f32, shift: bool) -> Part {
    match ((x - head).abs() < reach, (x - tail).abs() < reach) {
        (true, true) if x < (head + tail) / 2.0 => Part::Head,
        (true, true) | (false, true) => Part::Tail,
        (true, false) => Part::Head,
        (false, false) if shift && x >= head && x <= tail => Part::Selection,
        (false, false) => Part::Playhead,
    }
}

/// Seconds between ruler ticks for a view showing `span` seconds across `width` logical
/// pixels. Constant-rate video uses one tick per frame once frames are far enough apart.
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

/// How many ticks apart ruler labels sit: a multiple of five (labels go on long ticks) wide
/// enough that a label of `label_cells` never touches the next one.
fn label_interval(tick_pixels: f64, cell: f64, label_cells: usize) -> i64 {
    let needed = (label_cells as f64 + 3.0) * cell;
    (((needed / (tick_pixels * 5.0)).ceil() as i64).max(1)) * 5
}

impl Timeline {
    /// Ruler line, rail, and the strip that shows where a zoomed view sits in the video.
    pub fn height(font: &Font) -> i32 {
        font.ch + Self::rail(font) + font.ch / 2
    }
    /// The rail is two cells tall: the largest target on the deck.
    fn rail(font: &Font) -> i32 {
        2 * font.ch
    }
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
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        canvas: &mut Canvas,
        live: bool,
        area: R,
        media: &Media,
        edit: &mut Edit,
        position: f64,
        playing: bool,
    ) -> Response {
        let font = canvas.font;
        let mut result = Response::default();
        let ruler = R::new(area.x, area.y, area.w, font.ch);
        let rail = R::new(area.x, ruler.bottom(), area.w, Self::rail(font));
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
        let offset = self.offset;
        // Device pixels. The rail's last pixel column is the end of the visible span.
        let time_to_x = |t: f64| rail.x + ((t - offset) / span * rail.w as f64).round() as i32;
        let x_to_time = |x: f32| {
            ((x * font.ppp - rail.x as f32) as f64 / rail.w as f64 * span + offset)
                .clamp(0.0, media.duration)
        };
        let to_points = |x: i32| x as f32 / font.ppp;
        let head = to_points(time_to_x(edit.trim.start));
        let tail = to_points(time_to_x(edit.trim.end));
        let response = live.then(|| {
            let response = ui.interact(
                font.points(R::new(area.x, area.y, area.w, ruler.h + rail.h)),
                ui.id().with("timeline"),
                Sense::click_and_drag(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Slider,
                    true,
                    "Trim timeline. I sets the start; O sets the end. T types exact times.",
                )
            });
            response
        });
        if let Some(response) = &response {
            if response.drag_started()
                && let Some(pointer) = ui.input(|i| i.pointer.press_origin())
            {
                self.drag = Some(Drag {
                    part: grab(pointer.x, head, tail, GRAB, ui.input(|i| i.modifiers.shift)),
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
                            edit.trim.end =
                                target.max(edit.trim.start + minimum).min(media.duration);
                            result.seek =
                                Some(((edit.trim.end - minimum).max(edit.trim.start), false));
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
            if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                result.seek = Some((
                    (position + media.rate.frame_seconds()).min(media.duration),
                    true,
                ));
            }
            if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                result.seek = Some(((position - media.rate.frame_seconds()).max(0.0), true));
            }
        }
        result.editing = self.drag.is_some();

        // The rail is a recess; the kept part of the video is raised inside it.
        canvas.fill(rail, theme::FIELD);
        let overshoot = font.ch / 8;
        let weight = 3 * font.px;
        let outer = canvas.narrow(R::new(
            area.x - weight,
            area.y,
            area.w + 2 * weight,
            ruler.h + rail.h + overshoot,
        ));
        let (head_x, tail_x) = (time_to_x(edit.trim.start), time_to_x(edit.trim.end));
        let kept = R::new(head_x, rail.y, tail_x - head_x, rail.h).intersect(rail);
        if !kept.is_empty() {
            canvas.fill(kept, theme::RULE);
            canvas.fill(R::new(kept.x, kept.y, kept.w, font.px), theme::TRIM);
            canvas.fill(
                R::new(kept.x, kept.bottom() - font.px, kept.w, font.px),
                theme::TRIM,
            );
        }
        // The ruler measures source time: ticks stay on their timestamps through zoom and pan.
        let step = tick_step(
            span,
            (rail.w as f32 / font.ppp) as f64,
            (!media.variable_rate).then(|| media.rate.frame_seconds()),
        );
        let tick_pixels = step / span * rail.w as f64;
        let whole_seconds = |interval: i64| (step * interval as f64).fract().abs() < 1e-9;
        let mut interval = label_interval(tick_pixels, font.cw as f64, 8);
        if !whole_seconds(interval) {
            interval = label_interval(tick_pixels, font.cw as f64, 12);
        }
        let first = (offset / step).ceil() as i64;
        let last = ((offset + span) / step).floor() as i64;
        for index in first..=last {
            let time = index as f64 * step;
            let x = time_to_x(time).min(rail.right() - font.px);
            let long = index % 5 == 0;
            canvas.fill(
                R::new(
                    x,
                    rail.y,
                    font.px,
                    if long { font.ch / 2 } else { font.ch / 4 },
                ),
                theme::DIM,
            );
            if index % interval == 0 {
                let label = timecode(time);
                let label = if whole_seconds(interval) {
                    &label[..8]
                } else {
                    &label[..]
                };
                if x + font.cw / 2 + font.width(label) <= rail.right() {
                    canvas.fill(
                        R::new(x, ruler.y + font.ch / 4, font.px, ruler.h),
                        theme::DIM,
                    );
                    canvas.text(x + font.cw / 2, ruler.y, label, theme::DIM, false);
                }
            }
        }
        for (x, opening) in [(head_x - weight, true), (tail_x + weight, false)] {
            let edge = if opening { head_x } else { tail_x };
            if edge >= rail.x && edge <= rail.right() {
                kit::bracket(
                    canvas,
                    (x, rail.y - overshoot, rail.h + 2 * overshoot),
                    (weight, font.cw),
                    opening,
                    theme::TRIM,
                );
            }
        }
        canvas.restore(outer);
        let x = time_to_x(position);
        if x >= rail.x && x <= rail.right() {
            let x = x.min(rail.right() - font.px);
            canvas.fill(R::new(x, rail.y, font.px, rail.h), theme::INK);
            // A pixel triangle in the ruler line's descender band, pointing at the frame.
            let rows = font.ch / 4;
            for row in 0..rows {
                let half = (rows - 1 - row) * font.px;
                canvas.fill(
                    R::new(
                        x - half,
                        rail.y - (rows - row) * font.px,
                        2 * half + font.px,
                        font.px,
                    ),
                    theme::INK,
                );
            }
        }
        if response.as_ref().is_some_and(|r| r.has_focus()) {
            canvas.frame(rail, 1, theme::INK);
        }
        if self.zoom > 1.0 {
            // Where the zoomed view sits in the whole video.
            let strip = R::new(area.x, rail.bottom() + font.ch / 8, area.w, font.ch / 4);
            let whole = |t: f64| strip.x + (t / media.duration * strip.w as f64).round() as i32;
            canvas.fill(strip, theme::FIELD);
            let window = R::new(
                whole(offset),
                strip.y,
                (whole(offset + span) - whole(offset)).max(2 * font.px),
                strip.h,
            );
            canvas.fill(window, theme::DIM);
            canvas.fill(
                R::new(
                    whole(edit.trim.start),
                    strip.y,
                    (whole(edit.trim.end) - whole(edit.trim.start)).max(font.px),
                    font.px,
                ),
                theme::TRIM,
            );
            canvas.fill(
                R::new(
                    whole(position).min(strip.right() - font.px),
                    strip.y,
                    font.px,
                    strip.h,
                ),
                theme::INK,
            );
        }
        if let Some(response) = &response
            && response.hovered()
        {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos())
                && ((pos.x - head).abs() < GRAB || (pos.x - tail).abs() < GRAB)
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
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
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_brackets_stay_separately_reachable() {
        // Two seconds selected in a two-hour video: both brackets land on the same pixel.
        assert_eq!(grab(396.0, 400.0, 400.2, GRAB, false), Part::Head);
        assert_eq!(grab(404.0, 400.0, 400.2, GRAB, false), Part::Tail);
        assert_eq!(grab(105.0, 100.0, 900.0, GRAB, false), Part::Head);
        assert_eq!(grab(895.0, 100.0, 900.0, GRAB, false), Part::Tail);
        assert_eq!(grab(500.0, 100.0, 900.0, GRAB, true), Part::Selection);
        assert_eq!(grab(500.0, 100.0, 900.0, GRAB, false), Part::Playhead);
        assert_eq!(grab(950.0, 100.0, 900.0, GRAB, true), Part::Playhead);
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
    fn ruler_labels_never_touch() {
        for (tick, cells) in [(10.0, 8), (17.3, 12), (64.0, 8), (400.0, 12)] {
            let interval = label_interval(tick, 8.0, cells);
            assert_eq!(interval % 5, 0);
            assert!(interval as f64 * tick >= (cells as f64 + 3.0) * 8.0);
        }
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

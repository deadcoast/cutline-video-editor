//! The crop frame: drag a corner or an edge, or move the whole rectangle.
use super::{
    cell::{Canvas, R},
    theme,
};
use cutline::model::{Crop, Edit};
use eframe::egui::{self, Pos2, Rect, Sense};

#[derive(Clone, Copy)]
enum Handle {
    Corner(usize),
    Edge(usize),
    Move,
}
struct Drag {
    handle: Handle,
    origin: Pos2,
    before: Edit,
}
#[derive(Default)]
pub struct CropEditor {
    drag: Option<Drag>,
    pub ratio: Option<f64>,
}

impl CropEditor {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        canvas: &mut Canvas,
        video: Rect,
        source: [u32; 2],
        edit: &mut Edit,
    ) -> Option<Edit> {
        let c = edit.crop;
        let sx = video.width() / source[0] as f32;
        let sy = video.height() / source[1] as f32;
        let crop_rect = |c: Crop| {
            Rect::from_min_size(
                video.min + egui::vec2(c.x as f32 * sx, c.y as f32 * sy),
                egui::vec2(c.width as f32 * sx, c.height as f32 * sy),
            )
        };
        let r = crop_rect(c);
        let corners = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ];
        let edges = [
            r.center_top(),
            r.right_center(),
            r.center_bottom(),
            r.left_center(),
        ];
        let response = ui.interact(video, ui.id().with("crop-canvas"), Sense::DRAG);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Other,
                true,
                "Crop area. Use the crop controls for exact pixel coordinates.",
            )
        });
        let hit = |pos: Pos2| -> Option<Handle> {
            for (i, p) in corners.iter().enumerate() {
                if p.distance(pos) < 15.0 {
                    return Some(Handle::Corner(i));
                }
            }
            for (i, p) in edges.iter().enumerate() {
                if p.distance(pos) < 15.0 {
                    return Some(Handle::Edge(i));
                }
            }
            r.contains(pos).then_some(Handle::Move)
        };
        if response.drag_started()
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            && let Some(handle) = hit(origin)
        {
            self.drag = Some(Drag {
                handle,
                origin,
                before: edit.clone(),
            });
        }
        if let Some(drag) = &self.drag
            && let Some(pos) = response.interact_pointer_pos()
        {
            let dx = ((pos.x - drag.origin.x) / sx) as f64;
            let dy = ((pos.y - drag.origin.y) / sy) as f64;
            let b = drag.before.crop;
            let (x, y, w, h) = (b.x as f64, b.y as f64, b.width as f64, b.height as f64);
            if matches!(drag.handle, Handle::Move) {
                edit.crop = Crop::constrained(x + dx, y + dy, w, h, source);
            } else {
                let (left, top, right, bottom) = match drag.handle {
                    Handle::Corner(0) => (true, true, false, false),
                    Handle::Corner(1) => (false, true, true, false),
                    Handle::Corner(2) => (false, false, true, true),
                    Handle::Corner(3) => (true, false, false, true),
                    Handle::Edge(0) => (false, true, false, false),
                    Handle::Edge(1) => (false, false, true, false),
                    Handle::Edge(2) => (false, false, false, true),
                    _ => (true, false, false, false),
                };
                let mut l = if left {
                    (x + dx).clamp(0.0, x + w - 2.0)
                } else {
                    x
                };
                let mut t = if top {
                    (y + dy).clamp(0.0, y + h - 2.0)
                } else {
                    y
                };
                let mut rr = if right {
                    (x + w + dx).clamp(x + 2.0, source[0] as f64)
                } else {
                    x + w
                };
                let mut bb = if bottom {
                    (y + h + dy).clamp(y + 2.0, source[1] as f64)
                } else {
                    y + h
                };
                if let Some(ratio) = self.ratio {
                    let mut nw = rr - l;
                    let mut nh = bb - t;
                    if matches!(drag.handle, Handle::Edge(0) | Handle::Edge(2)) {
                        nw = nh * ratio;
                    } else {
                        nh = nw / ratio;
                    }
                    let anchor_x = if left {
                        x + w
                    } else if right {
                        x
                    } else {
                        x + w / 2.0
                    };
                    let anchor_y = if top {
                        y + h
                    } else if bottom {
                        y
                    } else {
                        y + h / 2.0
                    };
                    let max_w = if left {
                        anchor_x
                    } else if right {
                        source[0] as f64 - anchor_x
                    } else {
                        2.0 * anchor_x.min(source[0] as f64 - anchor_x)
                    };
                    let max_h = if top {
                        anchor_y
                    } else if bottom {
                        source[1] as f64 - anchor_y
                    } else {
                        2.0 * anchor_y.min(source[1] as f64 - anchor_y)
                    };
                    let factor = (max_w / nw).min(max_h / nh).min(1.0);
                    nw *= factor;
                    nh *= factor;
                    l = if left {
                        anchor_x - nw
                    } else if right {
                        anchor_x
                    } else {
                        anchor_x - nw / 2.0
                    };
                    t = if top {
                        anchor_y - nh
                    } else if bottom {
                        anchor_y
                    } else {
                        anchor_y - nh / 2.0
                    };
                    rr = l + nw;
                    bb = t + nh;
                }
                edit.crop = Crop::constrained(l, t, rr - l, bb - t, source);
            }
        }
        let font = canvas.font;
        let frame = font.snap(crop_rect(edit.crop));
        let whole = font.snap(video);
        // Everything outside the crop is dimmed by the veil; nothing else covers the frame.
        for outside in [
            R::new(whole.x, whole.y, whole.w, frame.y - whole.y),
            R::new(
                whole.x,
                frame.bottom(),
                whole.w,
                whole.bottom() - frame.bottom(),
            ),
            R::new(whole.x, frame.y, frame.x - whole.x, frame.h),
            R::new(
                frame.right(),
                frame.y,
                whole.right() - frame.right(),
                frame.h,
            ),
        ] {
            if !outside.is_empty() {
                canvas.fill(outside, theme::VEIL);
            }
        }
        for third in 1..3 {
            canvas.fill(
                R::new(frame.x + frame.w * third / 3, frame.y, font.px, frame.h),
                theme::DIM,
            );
            canvas.fill(
                R::new(frame.x, frame.y + frame.h * third / 3, frame.w, font.px),
                theme::DIM,
            );
        }
        // The frame and its drag marks, heavier at the corners and mid-edges. Each mark carries
        // a one-pixel black edge so it reads on any picture, including one in the crop colour.
        let weight = 3 * font.px;
        let arm = font.ch.min(frame.w / 2).min(frame.h / 2);
        let mut marks = vec![
            R::new(frame.x, frame.y, frame.w, font.px),
            R::new(frame.x, frame.bottom() - font.px, frame.w, font.px),
            R::new(frame.x, frame.y, font.px, frame.h),
            R::new(frame.right() - font.px, frame.y, font.px, frame.h),
        ];
        for (x, y) in [
            (frame.x, frame.y),
            (frame.right() - arm, frame.y),
            (frame.x, frame.bottom() - weight),
            (frame.right() - arm, frame.bottom() - weight),
        ] {
            marks.push(R::new(x, y, arm, weight));
        }
        for (x, y) in [
            (frame.x, frame.y),
            (frame.right() - weight, frame.y),
            (frame.x, frame.bottom() - arm),
            (frame.right() - weight, frame.bottom() - arm),
        ] {
            marks.push(R::new(x, y, weight, arm));
        }
        if frame.w > 4 * arm {
            let x = frame.x + (frame.w - arm) / 2;
            marks.push(R::new(x, frame.y, arm, weight));
            marks.push(R::new(x, frame.bottom() - weight, arm, weight));
        }
        if frame.h > 4 * arm {
            let y = frame.y + (frame.h - arm) / 2;
            marks.push(R::new(frame.x, y, weight, arm));
            marks.push(R::new(frame.right() - weight, y, weight, arm));
        }
        for mark in &marks {
            canvas.fill(mark.inset(-font.px, -font.px), theme::VOID);
        }
        for mark in marks {
            canvas.fill(mark, theme::CROP);
        }
        // The size travels with the frame while it changes.
        let size = format!("{}×{}", edit.crop.width, edit.crop.height);
        let label = R::new(
            frame.x,
            if frame.y - font.ch >= whole.y {
                frame.y - font.ch
            } else {
                frame.y
            },
            font.width(&size) + 2 * font.cw,
            font.ch,
        );
        canvas.fill(label, theme::CROP);
        canvas.text(label.x + font.cw, label.y, &size, theme::PANEL, true);
        if let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            && let Some(h) = hit(pos)
        {
            ui.ctx().set_cursor_icon(match h {
                Handle::Move => egui::CursorIcon::Move,
                Handle::Corner(0) | Handle::Corner(2) => egui::CursorIcon::ResizeNwSe,
                Handle::Corner(_) => egui::CursorIcon::ResizeNeSw,
                Handle::Edge(0) | Handle::Edge(2) => egui::CursorIcon::ResizeVertical,
                Handle::Edge(_) => egui::CursorIcon::ResizeHorizontal,
            });
        }
        if response.drag_stopped() {
            return self.drag.take().map(|d| d.before);
        }
        None
    }
}

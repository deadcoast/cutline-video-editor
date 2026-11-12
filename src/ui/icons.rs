use eframe::egui::{self, Color32, Painter, Rect, Shape, Stroke};

/// One 16-point drawing grid keeps window, transport and editing controls related.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Open,
    Help,
    Pip,
    Restore,
    Fullscreen,
    ExitFullscreen,
    Minimize,
    Close,
    Play,
    Pause,
    PreviousFrame,
    NextFrame,
}

pub fn paint(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let scale = rect.width() / 16.0;
    let point = |x: f32, y: f32| rect.min + egui::vec2(x, y) * scale;
    let stroke = Stroke::new(1.5 * scale, color);
    let line = |coordinates: &[(f32, f32)]| {
        painter.add(Shape::line(
            coordinates.iter().map(|&(x, y)| point(x, y)).collect(),
            stroke,
        ));
    };
    let outline = |min: (f32, f32), max: (f32, f32)| {
        painter.rect_stroke(
            Rect::from_min_max(point(min.0, min.1), point(max.0, max.1)),
            1,
            stroke,
            egui::StrokeKind::Middle,
        );
    };
    let triangle = |coordinates: &[(f32, f32)]| {
        painter.add(Shape::convex_polygon(
            coordinates.iter().map(|&(x, y)| point(x, y)).collect(),
            color,
            Stroke::NONE,
        ));
    };

    match icon {
        Icon::Open => {
            line(&[(1.5, 6.0), (1.5, 3.0), (6.0, 3.0), (8.0, 5.0), (14.0, 5.0)]);
            line(&[
                (1.5, 6.0),
                (14.5, 6.0),
                (12.5, 13.0),
                (3.0, 13.0),
                (1.5, 6.0),
            ]);
        }
        Icon::Help => {
            painter.circle_stroke(point(8.0, 8.0), 6.5 * scale, stroke);
            line(&[
                (5.8, 5.8),
                (6.2, 4.5),
                (7.6, 4.0),
                (9.2, 4.5),
                (9.8, 5.8),
                (9.2, 7.0),
                (8.0, 8.0),
                (8.0, 9.2),
            ]);
            painter.circle_filled(point(8.0, 11.7), 0.8 * scale, color);
        }
        Icon::Pip => {
            outline((1.5, 2.5), (14.5, 13.5));
            painter.rect_filled(
                Rect::from_min_max(point(8.0, 7.5), point(12.5, 11.5)),
                0,
                color,
            );
        }
        Icon::Restore => {
            outline((2.0, 5.0), (11.0, 14.0));
            line(&[
                (5.0, 5.0),
                (5.0, 2.0),
                (14.0, 2.0),
                (14.0, 11.0),
                (11.0, 11.0),
            ]);
        }
        Icon::Fullscreen => {
            line(&[(1.5, 6.0), (1.5, 1.5), (6.0, 1.5)]);
            line(&[(10.0, 1.5), (14.5, 1.5), (14.5, 6.0)]);
            line(&[(14.5, 10.0), (14.5, 14.5), (10.0, 14.5)]);
            line(&[(6.0, 14.5), (1.5, 14.5), (1.5, 10.0)]);
        }
        Icon::ExitFullscreen => {
            line(&[(1.5, 6.0), (6.0, 6.0), (6.0, 1.5)]);
            line(&[(10.0, 1.5), (10.0, 6.0), (14.5, 6.0)]);
            line(&[(14.5, 10.0), (10.0, 10.0), (10.0, 14.5)]);
            line(&[(6.0, 14.5), (6.0, 10.0), (1.5, 10.0)]);
        }
        Icon::Minimize => line(&[(3.0, 8.0), (13.0, 8.0)]),
        Icon::Close => {
            line(&[(3.5, 3.5), (12.5, 12.5)]);
            line(&[(12.5, 3.5), (3.5, 12.5)]);
        }
        Icon::Play => triangle(&[(4.0, 2.5), (13.0, 8.0), (4.0, 13.5)]),
        Icon::Pause => {
            for x in [4.0, 10.0] {
                painter.rect_filled(
                    Rect::from_min_max(point(x, 3.0), point(x + 2.0, 13.0)),
                    0,
                    color,
                );
            }
        }
        Icon::PreviousFrame => {
            line(&[(2.5, 3.0), (2.5, 13.0)]);
            triangle(&[(12.5, 3.0), (5.5, 8.0), (12.5, 13.0)]);
        }
        Icon::NextFrame => {
            triangle(&[(3.5, 3.0), (10.5, 8.0), (3.5, 13.0)]);
            line(&[(13.5, 3.0), (13.5, 13.0)]);
        }
    }
}

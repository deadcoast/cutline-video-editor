use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl FrameRate {
    pub fn parse(value: &str) -> Option<Self> {
        let (n, d) = value.split_once('/')?;
        let rate = Self {
            numerator: n.parse().ok()?,
            denominator: d.parse().ok()?,
        };
        (rate.numerator > 0 && rate.denominator > 0).then_some(rate)
    }
    pub fn fps(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
    pub fn frame_seconds(self) -> f64 {
        1.0 / self.fps()
    }
    pub fn snap(self, seconds: f64) -> f64 {
        (seconds * self.fps()).round() / self.fps()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trim {
    pub start: f64,
    /// Exclusive end, measured in source presentation seconds.
    pub end: f64,
}
impl Trim {
    pub fn duration(self) -> f64 {
        self.end - self.start
    }
    pub fn valid(self, duration: f64) -> bool {
        self.start.is_finite()
            && self.end.is_finite()
            && self.start >= 0.0
            && self.end <= duration + 0.000_001
            && self.end > self.start
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Crop {
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width: width / 2 * 2,
            height: height / 2 * 2,
        }
    }
    pub fn valid(self, width: u32, height: u32) -> bool {
        self.width >= 2
            && self.height >= 2
            && self.x.checked_add(self.width).is_some_and(|r| r <= width)
            && self.y.checked_add(self.height).is_some_and(|b| b <= height)
            && [self.x, self.y, self.width, self.height]
                .iter()
                .all(|n| n % 2 == 0)
    }
    pub fn constrained(x: f64, y: f64, w: f64, h: f64, source: [u32; 2]) -> Self {
        let even = |v: f64| (v.max(0.0) / 2.0).round() as u32 * 2;
        let width = even(w).clamp(2, source[0] / 2 * 2);
        let height = even(h).clamp(2, source[1] / 2 * 2);
        Self {
            x: even(x).min((source[0] - width) / 2 * 2),
            y: even(y).min((source[1] - height) / 2 * 2),
            width,
            height,
        }
    }
    pub fn fit_ratio(self, ratio: f64, source: [u32; 2]) -> Self {
        let cx = self.x as f64 + self.width as f64 / 2.0;
        let cy = self.y as f64 + self.height as f64 / 2.0;
        let w = (self.width as f64).min(self.height as f64 * ratio);
        let h = w / ratio;
        Self::constrained(cx - w / 2.0, cy - h / 2.0, w, h, source)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edit {
    pub trim: Trim,
    pub crop: Crop,
}
impl Edit {
    pub fn new(duration: f64, width: u32, height: u32) -> Self {
        Self {
            trim: Trim {
                start: 0.0,
                end: duration,
            },
            crop: Crop::full(width, height),
        }
    }
    pub fn valid(&self, duration: f64, width: u32, height: u32) -> bool {
        self.trim.valid(duration) && self.crop.valid(width, height)
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}
impl History {
    pub fn record(&mut self, before: Edit, after: &Edit) {
        if before == *after {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn undo(&mut self, current: &mut Edit) -> bool {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(current, previous));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, current: &mut Edit) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(current, next));
            true
        } else {
            false
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

pub fn timecode(seconds: f64) -> String {
    let ms = (seconds.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

pub fn parse_timecode(value: &str) -> Option<f64> {
    let parts = value.trim().split(':').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut result = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let n: f64 = part.parse().ok()?;
        if !n.is_finite() || n < 0.0 || (i > 0 && n >= 60.0) {
            return None;
        }
        result = result * 60.0 + n;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    #[test]
    fn fractional_high_frame_rate_does_not_round_to_sixty() {
        let fps = FrameRate::parse("120000/1001").unwrap();
        assert!((fps.snap(100.0) * fps.fps()).fract().abs() < 1e-8);
        assert!(fps.fps() > 119.0);
        assert_eq!(FrameRate::parse("0/0"), None);
    }
    #[test]
    fn rejects_invalid_ranges_and_timecodes() {
        assert!(
            !Trim {
                start: f64::NAN,
                end: 2.0
            }
            .valid(3.0)
        );
        assert!(
            !Trim {
                start: 2.0,
                end: 2.0
            }
            .valid(3.0)
        );
        assert_eq!(parse_timecode("1:60"), None);
        assert_eq!(parse_timecode("NaN"), None);
        assert_eq!(parse_timecode("01:02:03.125"), Some(3723.125));
        assert_eq!(timecode(3723.125), "01:02:03.125");
    }
    #[test]
    fn history_branch_discards_redo() {
        let original = Edit::new(10.0, 1920, 1080);
        let mut edit = original.clone();
        let mut history = History::default();
        edit.trim.start = 2.0;
        history.record(original.clone(), &edit);
        assert!(history.undo(&mut edit));
        assert_eq!(edit, original);
        let before = edit.clone();
        edit.trim.end = 8.0;
        history.record(before, &edit);
        assert!(!history.redo(&mut edit));
    }
    proptest! {
        #[test]
        fn every_drag_stays_inside_source(x in -10000.0..10000.0, y in -10000.0..10000.0,
            w in -10000.0..10000.0, h in -10000.0..10000.0) {
            prop_assert!(Crop::constrained(x,y,w,h,[3840,2160]).valid(3840,2160));
        }
    }
}

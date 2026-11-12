//! The cell renderer. Every mark Cutline draws over the video goes through here: one bitmap
//! face on a fixed character cell, flat fills, and rules a whole number of device pixels wide.
//!
//! Coordinates are device pixels (`i32`), so nothing lands between pixels at any display
//! scale. The face ships as five bitmap strikes (8×16 … 16×32, regular and bold) that match
//! the 100/125/150/175/200 % display steps exactly; larger scales repeat a strike a whole
//! number of times. No outline font and no antialiasing is involved.
use eframe::egui::{self, Color32, Mesh, Pos2, Rect, TextureHandle, TextureOptions, epaint};

const STRIKES: [(&[u8], &[u8]); 5] = [
    (
        include_bytes!("../../assets/cell/cell-16n.bin"),
        include_bytes!("../../assets/cell/cell-16b.bin"),
    ),
    (
        include_bytes!("../../assets/cell/cell-20n.bin"),
        include_bytes!("../../assets/cell/cell-20b.bin"),
    ),
    (
        include_bytes!("../../assets/cell/cell-24n.bin"),
        include_bytes!("../../assets/cell/cell-24b.bin"),
    ),
    (
        include_bytes!("../../assets/cell/cell-28n.bin"),
        include_bytes!("../../assets/cell/cell-28b.bin"),
    ),
    (
        include_bytes!("../../assets/cell/cell-32n.bin"),
        include_bytes!("../../assets/cell/cell-32b.bin"),
    ),
];
const ATLAS_COLUMNS: usize = 64;

/// A rectangle in device pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct R {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl R {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(self) -> i32 {
        self.y + self.h
    }
    pub fn inset(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w - 2 * dx, self.h - 2 * dy)
    }
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self::new(
            x,
            y,
            self.right().min(other.right()) - x,
            self.bottom().min(other.bottom()) - y,
        )
    }
    pub fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }
}

/// Picks the strike and whole-number repeat for a display scale: the largest cell not wider
/// than eight logical pixels, preferring a true strike over a repeated smaller one.
fn pick(pixels_per_point: f32) -> (usize, i32) {
    let target = (8.0 * pixels_per_point + 0.01).floor() as i32;
    let mut best = (0, 1, 8);
    for repeat in 1..=4 {
        for (index, width) in [8, 10, 12, 14, 16].into_iter().enumerate() {
            let cell = width * repeat;
            if cell <= target && cell > best.2 {
                best = (index, repeat, cell);
            }
        }
    }
    (best.0, best.1)
}

/// The loaded face plus the metrics every layout derives from.
pub struct Font {
    /// Device pixels per logical point this face was built for.
    pub ppp: f32,
    /// Cell width and height in device pixels. Height is always twice the width.
    pub cw: i32,
    pub ch: i32,
    /// One design pixel in device pixels: the weight of every rule.
    pub px: i32,
    /// Height of a control row: one cell plus a quarter cell above and below.
    pub row: i32,
    /// Thickness of the regular weight's upright strokes; drawn shapes use the same.
    pub stem: i32,
    texture: TextureHandle,
    codes: Vec<u32>,
    uv_cell: [f32; 2],
    bold_rows: usize,
    replacement: usize,
}
impl Font {
    pub fn matches(&self, pixels_per_point: f32) -> bool {
        (self.ppp - pixels_per_point).abs() < 1e-4
    }
    pub fn load(ctx: &egui::Context) -> Self {
        let ppp = ctx.pixels_per_point();
        let (index, repeat) = pick(ppp);
        let (regular, bold) = STRIKES[index];
        let (width, height) = (regular[5] as usize, regular[6] as usize);
        let count = u16::from_le_bytes([regular[7], regular[8]]) as usize;
        let codes = (0..count)
            .map(|i| {
                let at = 9 + i * 4;
                u32::from_le_bytes([
                    regular[at],
                    regular[at + 1],
                    regular[at + 2],
                    regular[at + 3],
                ])
            })
            .collect::<Vec<_>>();
        let stride = width.div_ceil(8);
        let rows = (count + 1).div_ceil(ATLAS_COLUMNS);
        let size = [ATLAS_COLUMNS * width, rows * 2 * height];
        let mut pixels = vec![Color32::TRANSPARENT; size[0] * size[1]];
        // Slot 0 is solid: fills and rules sample it, so one mesh carries a whole frame.
        for y in 0..height {
            for x in 0..width {
                pixels[y * size[0] + x] = Color32::WHITE;
            }
        }
        for (weight, data) in [regular, bold].into_iter().enumerate() {
            let bitmaps = &data[9 + count * 4..];
            for glyph in 0..count {
                let slot = glyph + 1;
                let left = slot % ATLAS_COLUMNS * width;
                let top = (slot / ATLAS_COLUMNS + weight * rows) * height;
                for y in 0..height {
                    let row = &bitmaps[(glyph * height + y) * stride..][..stride];
                    for x in 0..width {
                        if row[x / 8] & (0x80 >> (x % 8)) != 0 {
                            pixels[(top + y) * size[0] + left + x] = Color32::WHITE;
                        }
                    }
                }
            }
        }
        let texture = ctx.load_texture(
            "cell-face",
            egui::ColorImage::new(size, pixels),
            TextureOptions::NEAREST,
        );
        let replacement = codes
            .binary_search(&0xFFFD)
            .or_else(|_| codes.binary_search(&('?' as u32)))
            .unwrap_or(0);
        // Measure the upright of "I" on its middle row.
        let stem = codes.binary_search(&('I' as u32)).map_or(1, |glyph| {
            let row = &regular[9 + count * 4 + (glyph * height + height / 2) * stride..][..stride];
            row.iter()
                .map(|byte| byte.count_ones() as i32)
                .sum::<i32>()
                .max(1)
        });
        let (cw, ch) = (width as i32 * repeat, height as i32 * repeat);
        Self {
            ppp,
            cw,
            ch,
            px: repeat,
            row: ch + ch / 2,
            stem: stem * repeat,
            texture,
            codes,
            uv_cell: [1.0 / ATLAS_COLUMNS as f32, 1.0 / (rows * 2) as f32],
            bold_rows: rows,
            replacement,
        }
    }
    /// Device pixels for a length in logical points.
    pub fn px_of(&self, points: f32) -> i32 {
        (points * self.ppp).round() as i32
    }
    pub fn points(&self, r: R) -> Rect {
        Rect::from_min_max(
            Pos2::new(r.x as f32 / self.ppp, r.y as f32 / self.ppp),
            Pos2::new(r.right() as f32 / self.ppp, r.bottom() as f32 / self.ppp),
        )
    }
    pub fn snap(&self, rect: Rect) -> R {
        let x = self.px_of(rect.left());
        let y = self.px_of(rect.top());
        R::new(
            x,
            y,
            self.px_of(rect.right()) - x,
            self.px_of(rect.bottom()) - y,
        )
    }
    /// Width of a string in device pixels: one cell per character.
    pub fn width(&self, text: &str) -> i32 {
        text.chars().count() as i32 * self.cw
    }
    /// Top of a text line centred on its capitals inside a band starting at `y`.
    pub fn baseline(&self, y: i32, band: i32) -> i32 {
        // Capitals sit one sixteenth of a cell above the cell's centre.
        y + (band - self.ch) / 2 + self.ch / 16
    }
}

/// One frame's drawing, collected into a single textured mesh.
pub struct Canvas<'a> {
    pub font: &'a Font,
    mesh: Mesh,
    clip: Option<R>,
}
impl<'a> Canvas<'a> {
    pub fn new(font: &'a Font) -> Self {
        Self {
            font,
            mesh: Mesh::with_texture(font.texture.id()),
            clip: None,
        }
    }
    /// Narrows drawing to `clip` (inside any clip already set) and returns the previous clip,
    /// which the caller hands back to `restore` when it is done.
    pub fn narrow(&mut self, clip: R) -> Option<R> {
        let previous = self.clip;
        self.clip = Some(previous.map_or(clip, |outer| outer.intersect(clip)));
        previous
    }
    pub fn restore(&mut self, clip: Option<R>) {
        self.clip = clip;
    }
    fn quad(&mut self, r: R, uv: Rect, color: Color32) {
        let mut uv = uv;
        let mut r = r;
        if let Some(clip) = self.clip {
            let c = r.intersect(clip);
            if c.is_empty() {
                return;
            }
            let (uw, uh) = (uv.width() / r.w as f32, uv.height() / r.h as f32);
            uv = Rect::from_min_size(
                Pos2::new(
                    uv.left() + (c.x - r.x) as f32 * uw,
                    uv.top() + (c.y - r.y) as f32 * uh,
                ),
                egui::vec2(c.w as f32 * uw, c.h as f32 * uh),
            );
            r = c;
        }
        if r.is_empty() {
            return;
        }
        let rect = self.font.points(r);
        let base = self.mesh.vertices.len() as u32;
        for (pos, uv) in [
            (rect.left_top(), uv.left_top()),
            (rect.right_top(), uv.right_top()),
            (rect.right_bottom(), uv.right_bottom()),
            (rect.left_bottom(), uv.left_bottom()),
        ] {
            self.mesh.vertices.push(epaint::Vertex { pos, uv, color });
        }
        self.mesh
            .indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    pub fn fill(&mut self, r: R, color: Color32) {
        let [u, v] = self.font.uv_cell;
        let solid =
            Rect::from_min_max(Pos2::new(u * 0.25, v * 0.25), Pos2::new(u * 0.75, v * 0.75));
        self.quad(r, solid, color);
    }
    /// A rule `weight` design pixels thick drawn inside the edge of `r`.
    pub fn frame(&mut self, r: R, weight: i32, color: Color32) {
        let t = weight * self.font.px;
        self.fill(R::new(r.x, r.y, r.w, t), color);
        self.fill(R::new(r.x, r.bottom() - t, r.w, t), color);
        self.fill(R::new(r.x, r.y + t, t, r.h - 2 * t), color);
        self.fill(R::new(r.right() - t, r.y + t, t, r.h - 2 * t), color);
    }
    /// Draws `text` with its cell's top-left corner at (`x`, `y`); returns the width drawn.
    pub fn text(&mut self, x: i32, y: i32, text: &str, color: Color32, bold: bool) -> i32 {
        let font = self.font;
        let mut at = x;
        for character in text.chars() {
            if character != ' ' {
                let glyph = font
                    .codes
                    .binary_search(&(character as u32))
                    .unwrap_or(font.replacement);
                let slot = glyph + 1;
                let column = slot % ATLAS_COLUMNS;
                let row = slot / ATLAS_COLUMNS + if bold { font.bold_rows } else { 0 };
                let [u, v] = font.uv_cell;
                let uv = Rect::from_min_size(
                    Pos2::new(column as f32 * u, row as f32 * v),
                    egui::vec2(u, v),
                );
                self.quad(R::new(at, y, font.cw, font.ch), uv, color);
            }
            at += font.cw;
        }
        at - x
    }
    pub fn finish(self, painter: &egui::Painter) {
        if !self.mesh.is_empty() {
            painter.add(egui::Shape::mesh(self.mesh));
        }
    }
}

/// Shortens `text` to `cells` characters, ending in an ellipsis when it was cut.
pub fn fit(text: &str, cells: usize) -> String {
    if text.chars().count() <= cells {
        return text.to_owned();
    }
    let mut cut = text
        .chars()
        .take(cells.saturating_sub(1))
        .collect::<String>();
    cut.push('…');
    cut
}

/// Breaks `text` at spaces into lines of at most `cells` characters.
pub fn wrap(text: &str, cells: usize) -> Vec<String> {
    let cells = cells.max(8);
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word.to_owned();
            loop {
                let used = line.chars().count();
                let length = word.chars().count();
                if used == 0 && length <= cells {
                    line = word;
                    break;
                } else if used > 0 && used + 1 + length <= cells {
                    line.push(' ');
                    line.push_str(&word);
                    break;
                } else if used > 0 {
                    lines.push(std::mem::take(&mut line));
                } else {
                    // A single unbroken run longer than the line (a path): hard split.
                    let head = word.chars().take(cells).collect::<String>();
                    word = word.chars().skip(cells).collect();
                    lines.push(head);
                }
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_windows_display_step_gets_a_true_strike() {
        // (scale, strike index, repeat): 100–200 % map onto real bitmaps, never a resample.
        for (scale, strike, repeat) in [
            (1.0, 0, 1),
            (1.25, 1, 1),
            (1.5, 2, 1),
            (1.75, 3, 1),
            (2.0, 4, 1),
            (2.5, 1, 2),
            (3.0, 2, 2),
            (1.1, 0, 1),
            (0.8, 0, 1),
        ] {
            assert_eq!(pick(scale), (strike, repeat), "at {scale}");
        }
    }
    #[test]
    fn strikes_are_whole_cells_with_the_same_glyph_set() {
        let count = |data: &[u8]| u16::from_le_bytes([data[7], data[8]]);
        for (index, (regular, bold)) in STRIKES.into_iter().enumerate() {
            let width = 8 + 2 * index;
            for data in [regular, bold] {
                assert_eq!(&data[..5], b"CELL1");
                assert_eq!((data[5] as usize, data[6] as usize), (width, width * 2));
                assert_eq!(count(data), count(STRIKES[0].0));
                let glyphs = count(data) as usize;
                assert_eq!(
                    data.len(),
                    9 + glyphs * 4 + glyphs * width * 2 * width.div_ceil(8)
                );
            }
        }
    }
    #[test]
    fn text_is_cut_and_wrapped_on_cell_counts() {
        assert_eq!(fit("clip.mp4", 8), "clip.mp4");
        assert_eq!(fit("a-long-name.mp4", 8), "a-long-…");
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(
            wrap("C:\\a\\very\\long\\path", 8),
            ["C:\\a\\ver", "y\\long\\p", "ath"]
        );
    }
}

//! Controls built from cells. A bar is a list of lines; a line is a list of items measured in
//! whole cells. `arrange` places them (wrapping at gaps when the window is narrow), `block`
//! draws them and turns pointer and keyboard input into `Event`s for the application.
use super::{
    Act, Slot,
    cell::{Canvas, Font, R},
    theme,
};
use eframe::egui::{self, Color32, Key, Sense};

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Plain,
    /// Latched: the state this key controls is on.
    On,
    /// The one committing action of its bar or sheet.
    Primary,
    /// Unavailable right now.
    Off,
}
#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Minimize,
    Maximize,
    Close,
}
pub enum Item {
    /// An action. `cap` is the keyboard key that triggers it, printed ahead of the label.
    Key {
        cap: &'static str,
        label: String,
        tone: Tone,
        act: Act,
    },
    Icon {
        icon: Icon,
        act: Act,
    },
    Text {
        text: String,
        color: Color32,
        bold: bool,
    },
    /// A caption on a solid block.
    Tag {
        text: String,
        color: Color32,
    },
    /// A value that can be typed over.
    Field {
        slot: Slot,
        text: String,
        cells: usize,
        color: Color32,
    },
    /// A stepped level set by pointer, wheel or arrow keys.
    Meter {
        slot: Slot,
        value: usize,
        steps: usize,
    },
    /// Progress, not adjustable.
    Bar {
        fraction: f32,
        cells: usize,
    },
    Gap(i32),
    Rule,
    /// Everything after it sits against the right edge.
    Spring,
}
pub fn key(cap: &'static str, label: &str, act: Act) -> Item {
    Item::Key {
        cap,
        label: label.to_owned(),
        tone: Tone::Plain,
        act,
    }
}
pub fn toned(cap: &'static str, label: &str, tone: Tone, act: Act) -> Item {
    Item::Key {
        cap,
        label: label.to_owned(),
        tone,
        act,
    }
}
pub fn latch(cap: &'static str, label: &str, on: bool, act: Act) -> Item {
    toned(cap, label, if on { Tone::On } else { Tone::Plain }, act)
}
pub fn text(text: impl Into<String>) -> Item {
    Item::Text {
        text: text.into(),
        color: theme::INK,
        bold: false,
    }
}
pub fn dim(text: impl Into<String>) -> Item {
    Item::Text {
        text: text.into(),
        color: theme::DIM,
        bold: false,
    }
}
pub fn colored(text: impl Into<String>, color: Color32, bold: bool) -> Item {
    Item::Text {
        text: text.into(),
        color,
        bold,
    }
}
pub fn tag(text: &str, color: Color32) -> Item {
    Item::Tag {
        text: text.to_owned(),
        color,
    }
}

pub struct Line {
    pub items: Vec<Item>,
    /// One cell tall instead of a control row; for tables of text.
    pub tight: bool,
}
impl Line {
    pub fn row(items: Vec<Item>) -> Self {
        Self {
            items,
            tight: false,
        }
    }
    pub fn tight(items: Vec<Item>) -> Self {
        Self { items, tight: true }
    }
    fn height(&self, font: &Font) -> i32 {
        if self.tight { font.ch } else { font.row }
    }
}

/// The value being typed. Only one field is open at a time.
pub struct Entry {
    pub slot: Slot,
    pub text: String,
    /// Just opened: the first character typed replaces the whole value.
    pub fresh: bool,
}
pub enum Event {
    Act(Act),
    Meter(Slot, usize),
    /// A closed field was clicked.
    Edit(Slot),
    /// Enter, Tab or a click elsewhere. The last value is +1/-1 when Tab asks for a neighbour.
    Commit(Slot, String, i8),
    Cancel,
    /// Wheel or arrow keys over a field: step the value.
    Nudge(Slot, i32),
}
/// Input state shared by every block drawn in a frame.
pub struct Io {
    pub entry: Option<Entry>,
    pub events: Vec<Event>,
    /// False while a sheet covers the block: it is drawn but does not react.
    pub live: bool,
}

fn cells(item: &Item) -> i32 {
    let count = |s: &str| s.chars().count() as i32;
    match item {
        Item::Key { cap, label, .. } => {
            count(cap) + count(label) + 2 + i32::from(!cap.is_empty() && !label.is_empty())
        }
        Item::Icon { .. } | Item::Rule => 3,
        Item::Text { text, .. } => count(text),
        Item::Tag { text, .. } => count(text) + 2,
        Item::Field { cells, .. } => *cells as i32 + 2,
        Item::Meter { steps, .. } => *steps as i32,
        Item::Bar { cells, .. } => *cells as i32,
        Item::Gap(n) => *n,
        Item::Spring => 0,
    }
}

/// Places a line's items inside `width` device pixels. Returns `(item index, x, row)` for
/// every item that is drawn, and the number of rows used. Items wrap only at gaps and rules;
/// a gap that would start a row is dropped.
pub fn arrange(font: &Font, items: &[Item], width: i32) -> (Vec<(usize, i32, i32)>, i32) {
    arrange_with(font.cw, items, width)
}
fn arrange_with(cw: i32, items: &[Item], width: i32) -> (Vec<(usize, i32, i32)>, i32) {
    let split = items
        .iter()
        .position(|item| matches!(item, Item::Spring))
        .unwrap_or(items.len());
    let w = |index: usize| cells(&items[index]) * cw;
    let breaker = |index: usize| matches!(items[index], Item::Gap(_) | Item::Rule);
    let mut placed = Vec::new();
    let (mut x, mut row) = (0, 0);
    let mut index = 0;
    let mut pending: Vec<usize> = Vec::new();
    while index < split {
        if breaker(index) {
            pending.push(index);
            index += 1;
            continue;
        }
        let end = (index..split).find(|i| breaker(*i)).unwrap_or(split);
        let group: i32 = (index..end).map(w).sum();
        let lead: i32 = pending.iter().map(|i| w(*i)).sum();
        if x > 0 && x + lead + group > width {
            row += 1;
            x = 0;
        } else if x > 0 {
            for separator in pending.drain(..) {
                placed.push((separator, x, row));
                x += w(separator);
            }
        }
        pending.clear();
        for item in index..end {
            placed.push((item, x, row));
            x += w(item);
        }
        index = end;
    }
    if split < items.len() {
        let tail: i32 = (split + 1..items.len()).map(w).sum();
        if x > 0 && x + 2 * cw + tail > width {
            row += 1;
        }
        let mut at = (width - tail).max(0);
        for item in split + 1..items.len() {
            placed.push((item, at, row));
            at += w(item);
        }
    }
    (placed, row + 1)
}

/// Width of a run of items laid end to end, in device pixels.
pub fn width(font: &Font, items: &[Item]) -> i32 {
    items.iter().map(cells).sum::<i32>() * font.cw
}

/// The bracket: Cutline's one recurring shape. It marks the ends of the trim selection on
/// the timeline and encloses the wordmark. An opening bracket has its upright on
/// `[x, x + weight)` with serifs running right; a closing bracket has its upright on
/// `[x - weight, x)` with serifs running left. `weight` and `serif` are device pixels.
pub fn bracket(
    canvas: &mut Canvas,
    (x, top, height): (i32, i32, i32),
    (weight, serif): (i32, i32),
    opening: bool,
    color: Color32,
) {
    let (upright, reach) = if opening {
        (x, x)
    } else {
        (x - weight, x - serif)
    };
    canvas.fill(R::new(upright, top, weight, height), color);
    canvas.fill(R::new(reach, top, serif, weight), color);
    canvas.fill(R::new(reach, top + height - weight, serif, weight), color);
}

pub fn measure(font: &Font, lines: &[Line], width: i32) -> i32 {
    lines
        .iter()
        .map(|line| arrange(font, &line.items, width).1 * line.height(font))
        .sum()
}

/// Draws `lines` from (`x`, `y`) across `width`; returns the height used.
pub fn block(
    ui: &mut egui::Ui,
    canvas: &mut Canvas,
    io: &mut Io,
    tag: &str,
    (x, y, width): (i32, i32, i32),
    lines: &[Line],
) -> i32 {
    let font = canvas.font;
    let mut top = y;
    for (number, line) in lines.iter().enumerate() {
        let height = line.height(font);
        let (placed, rows) = arrange(font, &line.items, width);
        for (index, at, row) in placed {
            let item = &line.items[index];
            let r = R::new(x + at, top + row * height, cells(item) * font.cw, height);
            let id = egui::Id::new((tag, number, index));
            draw(ui, canvas, io, id, r, item);
        }
        top += rows * height;
    }
    top - y
}

/// The solid block behind a key, tag or field: the row with an eighth of a cell trimmed off
/// the top and bottom, so neighbouring rows of blocks never touch.
fn slab(font: &Font, r: R) -> R {
    r.inset(0, (r.h - font.ch) / 2 - font.ch / 8)
}

fn draw(ui: &mut egui::Ui, canvas: &mut Canvas, io: &mut Io, id: egui::Id, r: R, item: &Item) {
    let font = canvas.font;
    let line = font.baseline(r.y, r.h);
    match item {
        Item::Key {
            cap,
            label,
            tone,
            act,
        } => {
            let response = (*tone != Tone::Off && io.live).then(|| {
                let response = ui.interact(font.points(r), id, Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.as_str())
                });
                response
            });
            let lit = response
                .as_ref()
                .is_some_and(|r| r.hovered() || r.has_focus());
            let down = response
                .as_ref()
                .is_some_and(|r| r.is_pointer_button_down_on());
            let (ground, cap_ink, label_ink) = match tone {
                Tone::Off => (None, theme::RULE, theme::RULE),
                Tone::Primary if lit || down => (Some(theme::INK), theme::PANEL, theme::PANEL),
                Tone::Primary => (Some(theme::TRIM), theme::PANEL, theme::PANEL),
                _ if down => (Some(theme::TRIM), theme::PANEL, theme::PANEL),
                _ if lit => (Some(theme::INK), theme::PANEL, theme::PANEL),
                Tone::On => (None, theme::TRIM, theme::TRIM),
                Tone::Plain => (None, theme::DIM, theme::INK),
            };
            if let Some(ground) = ground {
                canvas.fill(slab(font, r), ground);
            } else if *tone == Tone::On {
                // On, or the chosen one of a set: enclosed by the bracket, like the selection
                // on the timeline.
                let (top, height) = (line + font.ch / 16, font.ch * 3 / 4);
                let stroke = (2 * font.px, font.cw / 2);
                bracket(
                    canvas,
                    (r.x + font.cw / 4, top, height),
                    stroke,
                    true,
                    theme::TRIM,
                );
                bracket(
                    canvas,
                    (r.right() - font.cw / 4, top, height),
                    stroke,
                    false,
                    theme::TRIM,
                );
            }
            let mut at = r.x + font.cw;
            if !cap.is_empty() {
                at += canvas.text(at, line, cap, cap_ink, false);
                if !label.is_empty() {
                    at += font.cw;
                }
            }
            canvas.text(at, line, label, label_ink, *tone == Tone::Primary);
            if response.is_some_and(|r| r.clicked()) {
                io.events.push(Event::Act(act.clone()));
            }
        }
        Item::Icon { icon, act } => {
            let response = io.live.then(|| {
                let response = ui.interact(font.points(r), id, Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        match icon {
                            Icon::Minimize => "Minimize",
                            Icon::Maximize => "Maximize",
                            Icon::Close => "Close",
                        },
                    )
                });
                response
            });
            let lit = response
                .as_ref()
                .is_some_and(|r| r.hovered() || r.has_focus());
            let ink = if lit {
                canvas.fill(
                    slab(font, r),
                    if *icon == Icon::Close {
                        theme::DANGER
                    } else {
                        theme::INK
                    },
                );
                theme::PANEL
            } else {
                theme::INK
            };
            // Drawn at the weight of the face's own stems, on the capital height.
            let side = font.ch * 5 / 8;
            let stem = font.stem;
            let left = r.x + (r.w - side) / 2;
            let top = r.y + (r.h - side) / 2;
            match icon {
                Icon::Minimize => {
                    canvas.fill(R::new(left, top + side - stem, side, stem), ink);
                }
                Icon::Maximize => {
                    let t = stem;
                    canvas.fill(R::new(left, top, side, t), ink);
                    canvas.fill(R::new(left, top + side - t, side, t), ink);
                    canvas.fill(R::new(left, top, t, side), ink);
                    canvas.fill(R::new(left + side - t, top, t, side), ink);
                }
                Icon::Close => {
                    let step = font.px;
                    let mut i = 0;
                    while i + step <= side {
                        let rise = (i + stem).min(side) - i;
                        canvas.fill(R::new(left + i, top + i, rise.max(step), step), ink);
                        canvas.fill(
                            R::new(
                                left + side - i - rise.max(step),
                                top + i,
                                rise.max(step),
                                step,
                            ),
                            ink,
                        );
                        i += step;
                    }
                }
            }
            if response.is_some_and(|r| r.clicked()) {
                io.events.push(Event::Act(act.clone()));
            }
        }
        Item::Text { text, color, bold } => {
            canvas.text(r.x, line, text, *color, *bold);
        }
        Item::Tag { text, color } => {
            canvas.fill(slab(font, r), *color);
            canvas.text(r.x + font.cw, line, text, theme::PANEL, true);
        }
        Item::Field {
            slot,
            text,
            cells,
            color,
        } => field(ui, canvas, io, id, r, *slot, text, *cells, *color),
        Item::Meter { slot, value, steps } => {
            let response = io.live.then(|| {
                let response = ui.interact(font.points(r), id, Sense::click_and_drag());
                response.widget_info(|| {
                    egui::WidgetInfo::slider(true, *value as f64, format!("{slot:?}"))
                });
                response
            });
            let mut next = *value as i32;
            let mut active = false;
            if let Some(response) = &response {
                active = response.dragged() || response.is_pointer_button_down_on();
                if active && let Some(pointer) = response.interact_pointer_pos() {
                    let travel = font.px_of(pointer.x) - r.x;
                    next = (travel + font.cw - 1).div_euclid(font.cw);
                }
                if response.hovered() {
                    let wheel = ui.input(|i| i.raw_scroll_delta.y);
                    next += i32::from(wheel > 0.0) - i32::from(wheel < 0.0);
                }
                if response.has_focus() {
                    next += i32::from(ui.input(|i| i.key_pressed(Key::ArrowRight)))
                        - i32::from(ui.input(|i| i.key_pressed(Key::ArrowLeft)));
                }
                active |= response.hovered() || response.has_focus();
            }
            let next = next.clamp(0, *steps as i32) as usize;
            if next != *value {
                io.events.push(Event::Meter(*slot, next));
            }
            segments(
                canvas,
                r,
                *steps,
                next,
                if active { theme::TRIM } else { theme::INK },
            );
        }
        Item::Bar { fraction, cells } => {
            let filled = (fraction.clamp(0.0, 1.0) * *cells as f32).floor() as usize;
            segments(canvas, r, *cells, filled, theme::TRIM);
        }
        Item::Rule => {
            let inset = font.ch / 4;
            canvas.fill(
                R::new(r.x + r.w / 2, r.y + inset, font.px, r.h - 2 * inset),
                theme::RULE,
            );
        }
        Item::Gap(_) | Item::Spring => {}
    }
}

fn segments(canvas: &mut Canvas, r: R, steps: usize, filled: usize, ink: Color32) {
    let font = canvas.font;
    let height = font.ch * 5 / 8;
    let top = r.y + (r.h - height) / 2;
    for step in 0..steps {
        canvas.fill(
            R::new(r.x + step as i32 * font.cw, top, font.cw - font.px, height),
            if step < filled { ink } else { theme::RULE },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn field(
    ui: &mut egui::Ui,
    canvas: &mut Canvas,
    io: &mut Io,
    id: egui::Id,
    r: R,
    slot: Slot,
    value: &str,
    cells: usize,
    color: Color32,
) {
    let font = canvas.font;
    let line = font.baseline(r.y, r.h);
    let body = slab(font, r);
    let right = |text: &str| r.right() - font.cw - font.width(text);
    if !io.live {
        canvas.fill(body, theme::FIELD);
        canvas.text(right(value), line, value, color, false);
        return;
    }
    let response = ui.interact(font.points(r), id, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::text_edit(true, value, value, format!("{slot:?}").as_str())
    });
    let wheel = if response.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        i32::from(wheel > 0.0) - i32::from(wheel < 0.0)
    } else {
        0
    };
    let open = io.entry.as_ref().is_some_and(|entry| entry.slot == slot);
    if !open {
        if response.has_focus() {
            // Give Tab and the arrows back to focus traversal once the field is closed.
            ui.memory_mut(|memory| memory.set_focus_lock_filter(id, egui::EventFilter::default()));
        }
        let lit = response.hovered() || response.has_focus();
        canvas.fill(body, if lit { theme::INK } else { theme::FIELD });
        canvas.text(
            right(value),
            line,
            value,
            if lit { theme::PANEL } else { color },
            false,
        );
        if response.clicked() {
            io.events.push(Event::Edit(slot));
        } else if wheel != 0 {
            io.events.push(Event::Nudge(slot, wheel));
        }
        return;
    }
    // Keep egui's focus on the open field so Tab, arrows and Escape reach it and Enter does
    // not activate some other control.
    ui.memory_mut(|memory| {
        memory.request_focus(id);
        memory.set_focus_lock_filter(
            id,
            egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        );
    });
    let events = ui.input(|i| i.events.clone());
    let clicked_away = ui.input(|i| i.pointer.any_pressed()) && !response.hovered();
    let mut outcome = None;
    let mut nudge = wheel;
    if let Some(entry) = &mut io.entry {
        let insert = |entry: &mut Entry, typed: &str| {
            for character in typed
                .chars()
                .filter(|c| c.is_ascii_digit() || ":.".contains(*c))
            {
                if entry.fresh {
                    entry.text.clear();
                    entry.fresh = false;
                }
                if entry.text.chars().count() < cells {
                    entry.text.push(character);
                }
            }
        };
        for event in &events {
            match event {
                egui::Event::Text(typed) | egui::Event::Paste(typed) => insert(entry, typed),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => match key {
                    Key::Backspace | Key::Delete => {
                        if entry.fresh {
                            entry.text.clear();
                            entry.fresh = false;
                        } else {
                            entry.text.pop();
                        }
                    }
                    Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End => entry.fresh = false,
                    Key::ArrowUp => nudge += 1,
                    Key::ArrowDown => nudge -= 1,
                    Key::Enter => outcome = Some(Event::Commit(slot, entry.text.clone(), 0)),
                    Key::Tab => {
                        outcome = Some(Event::Commit(
                            slot,
                            entry.text.clone(),
                            if modifiers.shift { -1 } else { 1 },
                        ));
                    }
                    Key::Escape => outcome = Some(Event::Cancel),
                    _ => {}
                },
                _ => {}
            }
        }
        if outcome.is_none() && clicked_away {
            outcome = Some(Event::Commit(slot, entry.text.clone(), 0));
        }
        canvas.fill(body, theme::FIELD);
        canvas.fill(
            R::new(body.x, body.bottom() - font.px, body.w, font.px),
            theme::TRIM,
        );
        if entry.fresh {
            // The whole value is selected: typing replaces it.
            let x = right(&entry.text);
            canvas.fill(
                R::new(x, line, font.width(&entry.text), font.ch).intersect(body),
                theme::INK,
            );
            canvas.text(x, line, &entry.text, theme::PANEL, false);
        } else {
            // The cursor is a solid cell after the last character.
            let x = right(&entry.text) - font.cw;
            canvas.text(x, line, &entry.text, theme::INK, false);
            canvas.fill(
                R::new(r.right() - 2 * font.cw, line, font.cw, font.ch).intersect(body),
                theme::INK,
            );
        }
    }
    if let Some(outcome) = outcome {
        io.events.push(outcome);
    } else if nudge != 0 {
        io.events.push(Event::Nudge(slot, nudge));
    }
}

/// A sheet: the only thing that ever covers the bars. Draws the veil, the box and its title
/// line; returns the box and the body area the caller fills with `block`.
pub fn sheet(canvas: &mut Canvas, screen: R, title: &str, cells: i32, body_height: i32) -> (R, R) {
    let font = canvas.font;
    canvas.fill(screen, theme::VEIL);
    let width = cells * font.cw;
    let height = font.row + font.ch / 2 + body_height + font.ch / 2;
    let outer = R::new(
        screen.x + (screen.w - width) / 2 / font.cw * font.cw,
        screen.y + ((screen.h - height) / 2).max(font.cw) / font.px * font.px,
        width,
        height,
    );
    canvas.fill(outer, theme::PANEL);
    canvas.fill(R::new(outer.x, outer.y, outer.w, font.row), theme::INK);
    canvas.text(
        outer.x + 2 * font.cw,
        font.baseline(outer.y, font.row),
        title,
        theme::PANEL,
        true,
    );
    canvas.frame(outer, 1, theme::INK);
    let body = R::new(
        outer.x + 2 * font.cw,
        outer.y + font.row + font.ch / 2,
        outer.w - 4 * font.cw,
        body_height,
    );
    (outer, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_wrap_at_gaps_and_the_tail_keeps_the_right_edge() {
        // O OPEN = 8 cells, P PIP = 7, F FULL = 8, MORE = 6.
        let items = [
            key("O", "OPEN", Act::Open),
            Item::Gap(2),
            key("P", "PIP", Act::Pip),
            Item::Gap(2),
            key("F", "FULL", Act::Full),
            Item::Spring,
            key("", "MORE", Act::More),
        ];
        let (placed, rows) = arrange_with(8, &items, 40 * 8);
        assert_eq!(rows, 1);
        assert_eq!(placed.last().unwrap(), &(6, (40 - 6) * 8, 0));
        let (placed, rows) = arrange_with(8, &items, 20 * 8);
        assert_eq!(rows, 2);
        // The gap before a wrapped group is dropped: the group starts at the margin.
        assert!(placed.contains(&(4, 0, 1)));
        assert!(
            !placed
                .iter()
                .any(|(index, _, row)| *index == 3 && *row == 1)
        );
        assert_eq!(placed.last().unwrap(), &(6, (20 - 6) * 8, 1));
        // A tail that cannot share the last row takes its own.
        let (placed, rows) = arrange_with(8, &items, 12 * 8);
        assert_eq!(rows, 4);
        assert_eq!(placed.last().unwrap(), &(6, (12 - 6) * 8, 3));
    }
}

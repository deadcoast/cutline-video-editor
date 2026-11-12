---
version: beta
name: Cutline
description: One video, drawn over by a character-cell instrument. One bitmap face, eight inks, one recurring shape.
colors:
  void: "#000000"
  panel: "#131312"
  field: "#21201E"
  rule: "#42403B"
  dim: "#8C887E"
  ink: "#E4E0D4"
  trim: "#F5A623"
  crop: "#5CC8D6"
  danger: "#F0563F"
  veil: "#000000 at 66%"
typography:
  face: "Terminus Font bitmap strikes, regular and bold, one size"
  cell: "8 × 16 device pixels at 100 %; 10 × 20, 12 × 24, 14 × 28, 16 × 32 at 125–200 %"
spacing:
  cell-width: "1 cell"
  control-row: "1.5 cells high"
  bar-margin: "2 cells"
  rule: "1 design pixel"
components:
  top-bar: "1 control row + rule"
  timeline-rail: "2 cells high"
  trim-bracket: "3 design pixels, serif 1 cell"
  sheet: "76 cells wide"
---

# Cutline

Status of this document: the interface system below was designed by the development agent on
2026-11-12 after the 0.1.0 interface (stock egui widgets with a recoloured palette, rounded
floating panels, two outline fonts, hover tooltips) was rejected. It replaces the earlier
DESIGN.md in full. The product scope is unchanged and comes from the original brief: Windows,
Rust, minimal-retro, one video, precise trim and crop, high frame rates, full viewing and PiP.

## Overview

The video owns the window. Everything else is a character-cell instrument laid over it: two
docked bars, a line of status under the top bar when there is something to say, and one sheet
when a decision is needed. There is no library, sidebar, card, floating panel or popup menu.

The retro part is the construction, and it is complete: a fixed cell grid, one bitmap face,
eight flat inks, rules one pixel wide, state shown by inverting a block. The modern part is
what that construction is held to: pixel-exact at every display scale, a full-rate video
surface underneath, every action reachable by pointer and by a key printed on the control,
values that can be typed or stepped, and layouts that reflow by whole cells.

## The five rules

1. **Everything sits on the cell.** The cell is 8 × 16 device pixels at 100 % scale. Every
   position and size is a whole number of device pixels derived from the cell: control rows are
   1½ cells high, bar margins 2 cells, the timeline rail 2 cells. Nothing is positioned in
   fractional or logical pixels.
2. **One face, one size, two weights.** All text is Terminus Font at the cell size, regular or
   bold. Each Windows scale step from 100 % to 200 % has its own hand-drawn strike, so text is
   never resampled or antialiased; beyond 200 % a strike is repeated a whole number of times.
   Hierarchy comes from weight, ink and position, never from a second size or face. Labels and
   keys are upper case; sentences addressed to the person are sentence case.
3. **Eight inks, never blended.** Void, panel, field, rule, dim, ink, and three signals. Amber
   is time and commitment: the trim brackets, the selection, a latched key, the one committing
   action. Cyan is space: the crop frame and its values. Red is a problem that needs the
   person. No gradients, shadows, glows, rounded corners or tints. The veil is the only
   translucent surface; it dims the picture outside a crop and behind a sheet.
4. **State is an inverted block.** A control under the pointer or holding keyboard focus is
   drawn as an ink block with panel-coloured text. Pressed is the same block in amber.
   Unavailable is rule-coloured text that does not react. A meter lights its segments amber
   instead, and the focused timeline rail takes a one-pixel ink frame. There are no hover tints, no
   outlines that fade in, and no tooltips: what a control does and which key triggers it is
   printed on the control (`SPACE PLAY`, `CTRL+E EXPORT`), and F1 lists every key.
5. **The bracket means selected.** The one recurring shape is the square bracket drawn as
   geometry, not as a glyph. A pair of brackets encloses the kept span on the timeline, a
   latched key (`[L LOOP]`), the chosen option in a set (`[MP4] MKV MOV WEBM`), the name
   `[CUTLINE]`, and the empty window where a video goes. Nothing else is decorated.

## Motion

Stepped, never eased. The bars retract and return in whole cells over 90 ms. The reading
indicator is one lit cell stepping across four, eight times a second. Nothing else moves
except the video and what the person drags.

## Layout

- **Top bar**, docked, one row: `[CUTLINE]`, file name, source size and rate; then
  `O OPEN`, `F1 KEYS`, `P PIP`, `F FULL` and the window's minimize, maximize and close marks,
  drawn at the stroke weight of the face. The empty part of the bar moves the window;
  double-click maximizes.
- **Status lines**, docked under the top bar and only present when needed: `READING`,
  `EXPORT` with a 24-cell progress meter and `CANCEL`, `ERROR` and `NOTE` with `DISMISS`.
  Messages wrap on cell boundaries and stay until dismissed. They remain when the bars
  retract.
- **Deck**, docked at the bottom:
  - registers: `POS` (bold), `IN` and `OUT` (amber, typed in place), `DUR`;
  - timeline: a ruler line of timestamps, the rail, and a thin strip that appears when zoomed
    to show where the view sits in the whole video;
  - keys: play, frame step, `I IN`, `O OUT`, `T TRIM`, `C CROP`, `V SOURCE`, `L LOOP`,
    `M MUTE`, then `MORE` and `CTRL+E EXPORT` against the right margin.
  - `MORE` adds a line: level meter, speed, timeline zoom, undo, redo, reset trim, pin.
  - Cropping adds a line: `CROP`, X Y W H fields, the aspect set, `RESET`, `ESC CANCEL`,
    `ENTER DONE`.
  Lines wrap at their gaps when the window is narrow; the deck grows by whole rows.
- **Video.** Under retracting bars it fills the window and the bars slide over it. With the
  bars pinned, or while cropping, it sits between them so the whole frame stays visible.
- **Sheet.** Export settings, the key list and the close-during-export question. A veil, a
  one-pixel ink frame, an inverted title line, the body, and the actions against the lower
  right with the committing action last.
- **Empty window.** A pair of brackets at timeline weight around `O OPEN A VIDEO` and the drop
  hint. The application is named once, in the top bar; there is no centred wordmark, tagline
  or slogan. While a file is being read, the indicator takes the place of the action.
- **Window.** A one-pixel rule is the window's edge and every bar ends exactly on it. All four
  edges and corners resize; the lower-right corner carries a dotted grip.

Editing minimum: 720 × 480 logical pixels (90 × 30 cells). PiP minimum: 360 × 220; PiP keeps
only `SPACE PLAY`, the position and `P RETURN`.

## Components

`src/ui/cell.rs` owns the face, the pixel grid and the single mesh every frame is drawn into.
`src/ui/kit.rs` owns the controls: key, latch, option, field, meter, progress bar, tag, rule,
window marks, sheet, the bracket, and the line layout. `src/ui/theme.rs` owns the eight inks.
`src/ui/timeline.rs` and `src/ui/crop.rs` own their gestures and drawing. `src/ui/mod.rs`
says what each bar contains and carries out the actions. No stock egui widget is drawn
anywhere; egui supplies input, focus and the window.

- **Key**: the trigger key in dim, the label in ink, one cell of padding each side. A key with
  no keyboard trigger shows only its label.
- **Field**: a field-coloured block holding a right-aligned value. Click (or `T` for the trim
  times) opens it with the value selected; typing replaces it; Enter or Tab commits, Escape
  restores. Wheel over a field, or Up/Down inside it, steps a time by one frame and a crop
  value by two pixels. The open field is underlined in amber with a solid cell cursor.
- **Meter**: one segment per cell, set by click, drag, wheel or arrow keys. Level has twenty
  segments; export quality has seventeen, fuller meaning higher quality.
- **Timeline**: the kept span is raised (rule on field) between two amber brackets with a
  one-pixel amber line above and below. Ticks hang from the top of the rail on source
  timestamps, long every fifth; labelled ticks carry a flag line up to their timecode.
  Constant-rate video gets one tick per frame once frames are ten pixels apart. The playhead
  is a one-pixel ink line with a pixel triangle above the rail.
- **Crop frame**: a one-pixel cyan frame with three-pixel marks at corners and mid-edges, each
  edged in black so it reads on any picture; thirds in dim; the crop size on a cyan tag that
  travels with the frame.

States: empty, reading, playable, paused, scrubbing, typing a value, crop draft, export
settings, exporting, verifying, completed, cancelled and error. Errors persist until
dismissed. Existing outputs are never overwritten. A crop draft has Done and Cancel;
committed edits have Undo and Redo.

## Do's and Don'ts

- Draw through `Canvas` in device pixels. Do not call an egui widget, painter text, rounded
  rectangle or stroke for anything the person sees.
- Do not add a second text size, a second face, an icon set, a colour outside the eight, or
  any blend other than the veil.
- Do not add tooltips, popups, dropdowns or floating panels. A new control goes on a bar line
  or a sheet line, with its key printed on it.
- Do not use the bracket for anything that is not a selection.
- Preserve source timestamps; never introduce a 30/60 fps output preset.
- Keep crop coordinates and the rendered preview in the same display-oriented, square-pixel
  coordinate space.
- Keep every operation local. Do not alter source videos.
- Do not call cross-compilation a Windows runtime test or claim a GPU performance result from
  software rendering.
- Do not add general-purpose editor features to fill empty space.

---
version: alpha
name: Cutline
description: A native video surface with tape-splice trim brackets and quiet, retracting controls.
colors:
  background: "#111316"
  surface: "#1D2025"
  border: "#454B53"
  text: "#E7EAEC"
  muted: "#A6AFB9"
  primary: "#EDC58B"
  crop: "#9FC5CE"
  danger: "#F69A94"
typography:
  sans:
    fontFamily: "Ubuntu, sans-serif"
    fontSize: "14px"
  mono:
    fontFamily: "Hack, monospace"
    fontSize: "13px"
rounded:
  DEFAULT: "3px"
  panel: "5px"
  dialog: "6px"
spacing:
  control-gap: "8px"
  panel-padding: "10px"
  viewport-inset: "20px"
components:
  timeline:
    height: "58px"
  trim-handle:
    width: "18px"
  controls:
    height: "32px"
---

# Cutline

## Overview

This is an operating surface for a single video. The user's brief is the product authority: Windows, Rust, minimal-retro, precise crop and trim, high frame rates, full viewing and PiP. No library dashboard, account, effects browser, tracks, cloud service or web frontend.

The visual reference is the measured labeling of a compact video deck, expressed through modern native controls. The signature is the warm pair of inward-facing trim brackets. Video owns the window. Metadata earns space only when it helps an edit. Retro character comes from the precise ruler, restrained amber selection and consistent rectangular controls, not decorative slogans or competing asset styles.

English interface; filenames retain Unicode. No locale-specific market assumptions. Native Windows dialogs own file selection.

## Colors

The normative palette above maps directly to constants in `src/ui/theme.rs`. That module is the only runtime theme owner. Amber means temporal selection or commit, pale cyan means crop/focus, and coral means an actionable problem. Labels and geometry accompany color.

## Typography

egui's embedded default fonts are used offline. Proportional type carries the wordmark, actions and headings; monospace is reserved for timecodes, technical values and shortcut references. No online font fetches. Titles 20px, actions/body 14px, time 13px, secondary metadata 12px. OS DPI scaling is native.

## Layout

A video viewport with top chrome and one bottom transport/trim strip. Secondary viewing controls live in More. Menus appear with pointer activity and disappear after 1.9 seconds away from their activation zones. Tab or Keep controls enables persistent keyboard access. Playback does not move the viewport when controls appear.

The top bar has 12px horizontal and 10px vertical padding. Its 32px controls use 8px gaps, with filename space yielding before window controls. Icon buttons have 36px-wide hit areas, one shared vector stroke language, accessible action names and shortcut tooltips. They retain visible boundaries at rest and distinct hover, pressed and keyboard-focus states.

The empty viewport has one 44px-high Open video action and one drop instruction, separated by 16px within the same layout. There is no centered wordmark, feature tagline or footer slogan. The title bar identifies the application once. Loading replaces the empty action rather than covering it.

Editing minimum: 720×480 logical pixels. PiP minimum: 360×220. PiP uses the same decoder and window, changes to always-on-top, and remembers the former window geometry. Crop mode reserves space for its controls so all handles remain reachable.

## Elevation & Depth

Solid graphite panels, one-pixel rules and restrained corners. No blur, neon glow, textured backgrounds, oversized cards, decorative animation, gradients or sidebars. The media itself supplies visual richness.

## Shapes

Rectangular action surfaces, 3px controls, 5px overlay panels. Trim boundaries use actual bracket geometry. Crop corners and edge handles express their drag directions.

## Components

`theme.rs` owns shared buttons; `timeline.rs` owns the time mapping and trim gesture; `crop.rs` owns crop gestures; `ui/mod.rs` composes these and owns dialogs, feedback and transport state. Export/probe/session work lives outside the UI.

States: empty, probing, playable, paused, scrubbing, crop draft, export configuration, exporting, verifying, completed, cancelled and actionable error. Errors persist until dismissed. Existing outputs are never overwritten. A crop draft has Done and Cancel; committed edits have Undo/Redo.

There is no decorative UI animation. Source video playback is intentional motion; Space pauses it. The interface uses the same labels in shortcuts, tooltips and dialogs.

## Do's and Don'ts

- Preserve source timestamps; never introduce a 30/60 fps output preset.
- Keep crop coordinates and rendered preview in the same display-oriented, square-pixel coordinate space.
- Keep every operation local. Do not alter source videos.
- Test generated media through the real encoder and probe it afterwards.
- Do not call cross-compilation a Windows runtime test or claim a GPU performance result from software rendering.
- Do not add general-purpose editor features to fill empty space.

# Cutline

A native Rust video viewer, trimmer and crop editor for Windows 10/11 x64.

This build has passed host-side media tests and Windows cross-compilation. Windows device and GPU validation remains outstanding; see `docs/VERIFICATION.md` (or `VERIFICATION.md` in the portable folder).

## Run the portable application

Extract **the entire ZIP**, then open **Cutline.exe**. No installation, account, Node, browser runtime or Rust toolchain is required to run the packaged application. Keep `libmpv-2.dll`, `vulkan-1.dll` and `tools/` beside the executable. Do not launch from inside the ZIP.

1. Drop a video into the window, or choose **Open**.
2. Drag the amber brackets to trim. Click the **IN** or **OUT** time (or press **T**) to type an exact timecode; Enter or Tab commits it, and the wheel steps it one frame.
3. Press **C** for crop, then drag a corner or edge, move the rectangle, choose an aspect, or type X/Y/W/H. **ENTER DONE** commits the crop; **ESC CANCEL** restores it.
4. Press **Ctrl+E**, set the export, then choose a destination filename. Source frame timing and the selected pixel dimensions are preserved.

Every control prints the key that triggers it; **F1** lists them all. There are no tooltips. The bars retract after pointer inactivity away from the top and bottom; **MORE → PIN** (or Tab) keeps them out and fits the video between them. **P** makes a compact always-on-top viewer and returns from it. Drag the top bar or the PiP image to move the window. Resize from any edge or corner.

## Keyboard

| Keys | Action |
|---|---|
| Space | Play / pause |
| Left / Right | Previous / next frame |
| Shift + Left / Right | Seek five seconds |
| I / O | Set start / end at the displayed frame |
| Home / End | Go to the first / last selected frame |
| T / C | Type exact trim times / crop |
| Enter / Tab in a value | Commit / commit and go to the next value |
| Up / Down in a value, wheel over it | Step one frame, or two pixels for crop |
| Enter while cropping | Commit the crop |
| V | Original / cropped preview |
| L | Loop selection |
| F or F11 / P | Fullscreen / PiP |
| M | Mute / unmute |
| Ctrl+O / Ctrl+E | Open / export |
| Ctrl+Z / Ctrl+Shift+Z or Ctrl+Y | Undo / redo |
| Ctrl+wheel over timeline | Zoom around the playhead |
| Wheel or Shift+wheel over timeline | Pan a zoomed timeline |
| Shift+drag on selection | Move both trim boundaries together |
| Escape | Close an open value, cancel crop, dismiss a sheet, or leave fullscreen/PiP |
| F1 | Shortcut reference |

## Media behavior

- No application frame-rate ceiling. Output uses source timestamps, including fractional rates such as 59.94/119.88 and variable-rate recordings. Playback refresh is bounded by the monitor and decoding capacity.
- No automatic 1080p downscale. Full 3840×2160 output is supported. Cropping produces the chosen dimensions without scaling the crop back up.
- mpv provides broad local format/codec playback, including common MP4, MKV, MOV, WebM, AVI and transport-stream recordings, subject to the codecs in the bundled engine.
- Output: H.264/HEVC in MP4, MOV or MKV; VP9 in WebM or MKV. All audio tracks are transcoded and retained unless removed. Subtitles, attachments and chapters are not exported.
- Accurate trim/crop exports re-encode. There is no misleading keyframe-only lossless trim mode. Trim end is exclusive. Constant-rate values snap to source frames; VFR uses presentation time.
- Crop positions and sizes use an even-pixel grid for chroma-compatible encoding. Rotation and non-square source pixels are normalized before applying the crop.
- Automatic encoding tries NVIDIA, Intel, AMD and then CPU. Explicit encoder selection reports failure if unavailable. Encoder availability depends on the GPU and driver.
- On hardware graphics, HDR playback is tone-mapped into this SDR application surface. The software-driver compatibility preview supports SDR and should not be used to assess HDR color. HDR output uses 10-bit HEVC/VP9 with source color tags. This is not an HDR mastering suite; dynamic HDR metadata is guarded rather than silently discarded. See the verification report for the tested scope.
- The original file is never edited or replaced. Exports use a temporary sibling file, are checked, and are published without replacing existing files. Cancel removes partial work.
- Committed edit recipes recover automatically for the same unmodified source. On Windows they live under the user's local application-data `Cutline/edits` directory. No media or filenames are uploaded.

## Build from source

Install the Rust stable toolchain with the MSVC target and Visual Studio Build Tools' Desktop development with C++ workload. This project requires Rust 1.88 or newer; the delivered build was compiled with Rust 1.99.1. The delivered portable executable is cross-compiled with the Windows GNU target; the included script builds natively with MSVC. The repo does not change global toolchains or Node installations.

Run from this directory in PowerShell:

```powershell
./scripts/build-windows.ps1
```

The script builds the native application, downloads checksum-pinned media archives from their upstream distributors, and creates `dist/Cutline-Windows-x64.zip`. It needs 7-Zip for the mpv archive. It checks the standard installation path and PATH; install 7-Zip if neither is available. Dependencies and download hashes are in `Cargo.lock` and `tools.lock.json`.

In this repository the current portable package is checked in at the root beside the source (`Cutline.exe`, the two DLLs, `tools/`, `licenses/`, `SHA256SUMS.txt`, `VERIFICATION.md`), so a clone runs by opening `Cutline.exe`. `history/` keeps each release's source and portable archives; the files without a version in their name are 0.1.0.

For Linux development, provide libmpv 0.37+ and FFmpeg/ffprobe on PATH, then `cargo run -- path/to/video.mp4`. Linux is a development/test host, not the shipped platform. Use `CUTLINE_MPV_LOG` with an absolute path when diagnosing playback.

## Verification

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=2
```

The integration tests generate and encode real media with FFmpeg. On Windows, place the packaged tools beside the test executable or set up the build's test tooling as described in `scripts/verify-windows.ps1`. `docs/VERIFICATION.md` records the executed checks and remaining device validation.

## Architecture

`model.rs`: source-time ranges, crop geometry and bounded edit history. `media/player.rs`: contained libmpv FFI and GPU rendering with checked GL-thread ownership. `media/probe.rs`: bounded metadata inspection and orientation normalization. `media/export.rs`: immutable export plans, progress, cancellation and no-clobber publication. `media/session.rs`: atomic local edit recovery. `ui/cell.rs`: the bitmap face, device-pixel grid and single draw mesh. `ui/kit.rs`: the controls built from cells. `ui/timeline.rs`, `ui/crop.rs`: gestures and drawing. `ui/mod.rs`: bar contents and the single-video workflow. egui/eframe supply the window, input and focus; no stock widget is drawn.

No shell interprets filenames. Media tools receive separate argument values. There is no remote API, updater, telemetry or arbitrary script runner.

The application's source is GPL-3.0-or-later. See `LICENSE` and `THIRD_PARTY.md` for bundled component provenance.

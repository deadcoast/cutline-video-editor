# Cutline

A native Rust video viewer, trimmer and crop editor for Windows 10/11 x64.

This build has passed host-side media tests and Windows cross-compilation. Windows device and GPU validation remains outstanding; see `docs/VERIFICATION.md` (or `VERIFICATION.md` in the portable folder).

## Run the portable application

Extract **the entire ZIP**, then open **Cutline.exe**. No installation, account, Node, browser runtime or Rust toolchain is required to run the packaged application. Keep `libmpv-2.dll`, `vulkan-1.dll` and `tools/` beside the executable. Do not launch from inside the ZIP.

1. Drop a video into the window, or choose **Open**.
2. Drag the amber start/end brackets to trim. **Trim** accepts exact timecodes.
3. Choose **Crop**, then drag a corner/edge, move the rectangle, choose an aspect preset or enter X/Y/W/H. **Done** commits the crop; **Cancel** restores it.
4. Choose **Export**, then a destination filename. Source frame timing and the selected pixel dimensions are preserved.

Controls disappear after pointer inactivity away from the top/bottom. **More → Keep controls visible** pins them. **PiP** makes a compact always-on-top viewer; **Return** restores the editing window. Drag the title or the PiP image to move the window. Resize from the lower-right corner.

## Keyboard

| Keys | Action |
|---|---|
| Space | Play / pause |
| Left / Right | Previous / next frame |
| Shift + Left / Right | Seek five seconds |
| I / O | Set start / end at the displayed frame |
| Home / End | Go to the first / last selected frame |
| T / C | Precise trim / crop |
| V | Original / cropped preview |
| L | Loop selection |
| F or F11 / P | Fullscreen / PiP |
| M | Mute / unmute |
| Ctrl+O / Ctrl+E | Open / export |
| Ctrl+Z / Ctrl+Shift+Z or Ctrl+Y | Undo / redo |
| Ctrl+wheel over timeline | Zoom around the playhead |
| Wheel or Shift+wheel over timeline | Pan a zoomed timeline |
| Shift+drag on selection | Move both trim boundaries together |
| Escape | Cancel crop, dismiss dialog, or leave fullscreen/PiP |
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

In this working folder the 0.1.0 portable runtime (`Cutline.exe`, the two DLLs, `tools/`) sits beside the source and is git-ignored, and `history/` keeps the 0.1.0 source and portable archives. `Cutline-next.exe` is the current source cross-compiled for Windows; it runs from this folder against the same DLLs and tools.

For Linux development, provide libmpv 0.37+ and FFmpeg/ffprobe on PATH, then `cargo run -- path/to/video.mp4`. Linux is a development/test host, not the shipped platform. Use `CUTLINE_MPV_LOG` with an absolute path when diagnosing playback.

## Verification

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=2
```

The integration tests generate and encode real media with FFmpeg. On Windows, place the packaged tools beside the test executable or set up the build's test tooling as described in `scripts/verify-windows.ps1`. `docs/VERIFICATION.md` records the executed checks and remaining device validation.

## Architecture

`model.rs`: source-time ranges, crop geometry and bounded edit history. `media/player.rs`: contained libmpv FFI and GPU rendering with checked GL-thread ownership. `media/probe.rs`: bounded metadata inspection and orientation normalization. `media/export.rs`: immutable export plans, progress, cancellation and no-clobber publication. `media/session.rs`: atomic local edit recovery. `ui/`: native egui controls and single-video workflow.

No shell interprets filenames. Media tools receive separate argument values. There is no remote API, updater, telemetry or arbitrary script runner.

The application's source is GPL-3.0-or-later. See `LICENSE` and `THIRD_PARTY.md` for bundled component provenance.

# Cutline — verification record

## 0.1.0 — 2026-10-20

Date: 2026-10-20. Status: Windows build candidate; automated media checks and native Linux UI inspection passed. A physical Windows device was not available in this environment. This record does not claim Windows runtime certification or sustained 4K playback performance.

### Executed checks

| Check | Result / evidence |
|---|---|
| `cargo fmt --check` | Passed |
| `cargo clippy --all-targets -- -D warnings` | Passed without warnings |
| `cargo test --locked -- --test-threads=2` | 14 tests passed: 6 unit/property tests and 8 real-media integration tests |
| Windows x64 release cross-build | Passed using Rust 1.99.1, `x86_64-pc-windows-gnu`, static compiler runtime |
| Windows PE dependency inspection | GUI subsystem; application imports Windows DLLs only. libmpv's additional Vulkan loader is included. No MinGW runtime DLL is required beside the app. |
| Native UI inspection | Real egui/libmpv executable under Xvfb/Mesa on Linux; no web mockup |
| Interface contract audit | Strict mode: 0 errors, 0 warnings |
| Media archive integrity | SHA-256 verified against the pinned values in tools.lock.json |

The automated media tests used FFmpeg/ffprobe 6.1.1 on Linux. The portable package contains upstream Windows FFmpeg/ffprobe 9.0.2 and mpv revision 6a262d11f6. The native UI host used libmpv 0.37. These are different execution environments; passing the host checks does not establish that every Windows driver behaves correctly.

### Media results

- 60 fps and 120000/1001 fps: trim source frames 17 through 76, crop to 480×270, preserve one audio track, and export exactly 60 frames at the source rate. A decoded-image comparison confirms the first exported image matches source frame 17 more closely than either neighboring frame; this checks content as well as metadata.
- 3840×2160 at 60 fps: trim 0.1–0.4 seconds; output remains 3840×2160 and contains exactly 18 frames.
- Variable-frame-rate input alternating a 120 fps segment with a 60 fps segment: crop retains frame count and each presentation timestamp within the test tolerance.
- Portrait orientation: a 90-degree display matrix is normalized before a 300×500 crop; the export has the expected portrait dimensions and 60 frames.
- HEVC/MP4 and VP9/WebM: 60 fps samples retain all 30 frames and their audio track.
- A 240 fps source produces an export plan with timestamp passthrough and no frame-rate replacement argument. This is an argument check, not a 240 fps performance measurement.
- Existing destination content remains unchanged. Cancellation removes partial output. Geometry property tests keep crop rectangles inside the source and on the encoder-compatible pixel grid.

### Native interaction results

Observed the actual application with a 1920×1080, 120 fps test recording: video display and play/pause; independent head/tail dragging; precise numeric trim; free crop dragging and committed crop preview; undo; crop cancellation; compact PiP sizing and return; control auto-hide; export configuration; and edit recovery across restarts. The editor was inspected at 1120×720 and its 720×480 minimum. The export dialog remains fully reachable at the minimum size. Crop fields have fixed widths so their changing values do not move the confirmation buttons.

A quick Ctrl+Z sequence exposed reliance on the current modifier state after key release. Shortcuts now use the modifiers attached to each key event, and undo was rechecked in the running application.

The host's Mesa software renderer produced black/incorrect-color intermediate video passes. Cutline now selects mpv's documented compatibility renderer only when a software rasterizer is identified. Repeated launches and playback/trim/crop operations showed correct SDR test colors through that path. Hardware graphics retain the normal renderer. The software compatibility path is not an HDR color-reference preview; HDR output uses the independent FFmpeg pipeline.

### Required Windows device acceptance

The included Windows workflow and `scripts/verify-windows.ps1` make the automated checks repeatable. They have not been run on a remote Windows runner during this session. Before considering a public production release, verify on the intended machines:

1. Extract the full portable folder, launch without development tools, and open local files through both drag/drop and native dialogs, including Unicode paths. Repeat with a standard non-administrator account.
2. Run `scripts/verify-windows.ps1`. Check H.264, HEVC, VP9, portrait, multiple audio tracks, and long captures with the bundled Windows engines.
3. Play representative 1080p120 and 4K60 recordings on NVIDIA, AMD and Intel devices. Measure dropped frames, audio synchronization, memory use and seeking latency; confirm the monitor refresh rate. No sustained-rate measurement is claimed here.
4. Verify fullscreen, PiP always-on-top, restored placement, multi-monitor DPI changes, window resizing, hardware decoding and each hardware export encoder. Automatic export must fall back to CPU after an unavailable encoder.
5. Verify audible output, device changes, HDR tone mapping and 10-bit output on appropriate media/hardware. Dynamic HDR export remains intentionally guarded. Static HDR mastering metadata is not guaranteed to survive re-encoding; this is not an HDR mastering tool.
6. Check screen-reader navigation, keyboard focus, long filenames, native save dialog cancellation, low disk space, and closing during an active export. Cancel must remove partial work and never replace the source.

The executable is unsigned. The package includes complete application source separately, dependency licenses, pinned engine download hashes, and SHA256SUMS.txt for the portable contents.

## 0.1.1 — 2026-11-12

Scope: timeline input and ruler, preview render cost, two interface state fixes. Export, probe and session code are unchanged. Evidence below is from the Linux host (Xvfb, Mesa llvmpipe, libmpv 0.37, FFmpeg 6.1.1) and a Windows cross-build. Nothing here was run on a Windows device or a hardware GPU.

| Check | Result / evidence |
|---|---|
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo test --locked -- --test-threads=2` | 17 passed: 6 library, 3 timeline, 8 real-media |
| Windows x64 release cross-build | Passed with Rust 1.97.0, `x86_64-pc-windows-gnu`; GUI subsystem, Windows system DLL imports only |
| Native UI | 0.1.0 and 0.1.1 binaries driven with the same synthetic pointer/keyboard input against a 1920×1080 120 fps clip |

- Ctrl+wheel timeline zoom did nothing in 0.1.0. egui converts Ctrl+wheel into `zoom_delta` and leaves the scroll delta at zero, and the timeline read the scroll delta. Six Ctrl+wheel notches left 0.1.0 at 1×; 0.1.1 reached 11× around the playhead. Plain wheel pans, and horizontal travel (Shift+wheel, trackpads) now pans too.
- The ruler was 40 fixed marks unrelated to time. Ticks now sit on source timestamps and move with zoom and pan; constant-rate video gets one tick per frame once frames are at least 10 px apart (observed at 11× on the 120 fps clip). A zoomed view shows its visible range and factor in place of the selected-duration readout.
- Trim brackets closer than 13 px resolved every press to the start bracket, so the end bracket of a short selection in a long video could not be grabbed. Overlapping brackets now split at their midpoint (unit test).
- A zoomed view pages forward when playback leaves its edge.
- Preview rendering: 0.1.0 ran mpv's full render into a source-resolution target on every interface repaint. 0.1.1 renders only when mpv reports a changed frame and sizes the target to the viewport's pixel density, capped at source resolution. With a 3840×2160 60 fps clip paused in the 1120×720 window, process CPU over 10 s fell from 19.36 s to 1.33 s; with the pointer moving over the video for 5 s, from 11.61 s to 5.49 s. These are software-rasterizer CPU figures and say nothing about a hardware GPU. Frame-step, click-seek, playback to the trim end, crop mode and the cropped preview (compared with an FFmpeg crop of the same rectangle) displayed the correct frame through the new path; three consecutive launches restored the saved crop.
- M now returns to the previous volume instead of 75%. The "Open exported video / Show in folder" actions are cleared on dismiss, on a later export failure and when another file is opened.

Not re-verified in this pass: PiP, fullscreen, HDR, hardware encoders, and the export dialog (the host has no native file dialog). The timeline hover tooltip covers the transport row's left side while the pointer rests on the timeline; this is unchanged from 0.1.0.

## 0.1.1 interface replacement — 2026-11-12

Scope: every drawn part of the interface (`src/ui/`). Player, probe, export and session code are unchanged from the section above. Evidence is from the Linux host (Xvfb, Mesa llvmpipe, libmpv 0.37) and a Windows cross-build. Nothing here was run on a Windows device, a hardware GPU, or a physical high-density display.

| Check | Result / evidence |
|---|---|
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo test --locked -- --test-threads=2` | 22 passed: 6 library, 8 interface (cell, kit, timeline), 8 real-media |
| Source search for stock widgets and tooltips | No `on_hover_text`, ComboBox, Slider, DragValue, Modal, menu, TextEdit, ProgressBar, spinner or checkbox remains in `src/` |
| Windows x64 release cross-build | Passed with Rust 1.97.0, `x86_64-pc-windows-gnu`; GUI subsystem, Windows system DLL imports only |

Observed in the running application at 1120×720, 100 % scale, against a 1920×1080 120 fps clip: empty window; top bar and deck with a loaded video; retracted bars; latched key; pointer highlight on a key and on the close mark; the MORE line; timeline at 3× zoom with labelled ruler and overview strip; I and O marks; typing an IN time (commits) and an invalid OUT time (rejected with an ERROR line, edit unchanged); crop mode with the video between the bars; the export sheet; the key sheet; a failed open; PiP at 480×292; the minimum 720×480 window, where the crop and key lines wrap by whole rows. The same build at forced 125 %, 150 % and 200 % scales selected the 10×20, 12×24 and 16×32 strikes and drew pixel-exact. Tab stepped keyboard focus through the IN and OUT fields, the timeline, the deck keys and the top bar, with Enter opening a field and triggering a key.

The export progress line and the completion line with OPEN EXPORT / SHOW IN FOLDER / DISMISS were observed through a temporary build that skipped the native save dialog (the host has none) and exported a 3840×2160 clip to completion; that bypass is not in the source.

Not observed: fullscreen and maximize (the host has no window manager), edge resizing, the close-during-export sheet, HDR notes in the export sheet, the 175 % scale, and screen-reader output. At 150 % the regular weight's strokes are one device pixel wide; whether that is too light on a real high-density panel has not been judged on one.

## 0.1.1 release package — 2026-11-12

The source is commit b0314a4 with only the README, this record and the package files changed; no Rust source differs from that commit.

| Check | Result / evidence |
|---|---|
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked -- --test-threads=2` on the Linux host with Rust 1.99.1 | Passed; 22 tests: 6 library, 8 interface, 8 real-media |
| `Cutline.exe` in this package | Cross-compiled from that source with Rust 1.99.1, `x86_64-pc-windows-gnu`, static compiler runtime; GUI subsystem; imports Windows system DLLs only |
| GitHub Actions run 43243717471 for b0314a4 on a `windows-2025` runner, Rust 1.99.1 MSVC | Passed: native release build, packaging by `scripts/build-windows.ps1` with the three engine archives checked against `tools.lock.json`, then `scripts/verify-windows.ps1` (fmt, clippy, the same 22 tests, the real-media ones run through the bundled Windows FFmpeg and ffprobe) |
| Engines | `libmpv-2.dll`, `tools/ffmpeg.exe`, `tools/ffprobe.exe` and `vulkan-1.dll` are byte-identical to the 0.1.0 package; their lines in `SHA256SUMS.txt` are unchanged |

The Actions runner has no display or GPU and does not launch the application, and its MSVC build is not the executable shipped here. An earlier development build of this version was launched once on the owner's Windows machine at 100 % scale (`screenshots/Screenshot 2026-11-12 034827.png`); that was before the interface replacement and shows only that the executable starts and loads the playback engine there. The device acceptance list in the 0.1.0 section is still outstanding for this release, and so are the items listed as not observed above.

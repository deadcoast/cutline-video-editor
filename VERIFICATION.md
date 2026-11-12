# Cutline 0.1.0 — verification record

Date: 2026-10-20. Status: Windows build candidate; automated media checks and native Linux UI inspection passed. A physical Windows device was not available in this environment. This record does not claim Windows runtime certification or sustained 4K playback performance.

## Executed checks

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

## Media results

- 60 fps and 120000/1001 fps: trim source frames 17 through 76, crop to 480×270, preserve one audio track, and export exactly 60 frames at the source rate. A decoded-image comparison confirms the first exported image matches source frame 17 more closely than either neighboring frame; this checks content as well as metadata.
- 3840×2160 at 60 fps: trim 0.1–0.4 seconds; output remains 3840×2160 and contains exactly 18 frames.
- Variable-frame-rate input alternating a 120 fps segment with a 60 fps segment: crop retains frame count and each presentation timestamp within the test tolerance.
- Portrait orientation: a 90-degree display matrix is normalized before a 300×500 crop; the export has the expected portrait dimensions and 60 frames.
- HEVC/MP4 and VP9/WebM: 60 fps samples retain all 30 frames and their audio track.
- A 240 fps source produces an export plan with timestamp passthrough and no frame-rate replacement argument. This is an argument check, not a 240 fps performance measurement.
- Existing destination content remains unchanged. Cancellation removes partial output. Geometry property tests keep crop rectangles inside the source and on the encoder-compatible pixel grid.

## Native interaction results

Observed the actual application with a 1920×1080, 120 fps test recording: video display and play/pause; independent head/tail dragging; precise numeric trim; free crop dragging and committed crop preview; undo; crop cancellation; compact PiP sizing and return; control auto-hide; export configuration; and edit recovery across restarts. The editor was inspected at 1120×720 and its 720×480 minimum. The export dialog remains fully reachable at the minimum size. Crop fields have fixed widths so their changing values do not move the confirmation buttons.

A quick Ctrl+Z sequence exposed reliance on the current modifier state after key release. Shortcuts now use the modifiers attached to each key event, and undo was rechecked in the running application.

The host's Mesa software renderer produced black/incorrect-color intermediate video passes. Cutline now selects mpv's documented compatibility renderer only when a software rasterizer is identified. Repeated launches and playback/trim/crop operations showed correct SDR test colors through that path. Hardware graphics retain the normal renderer. The software compatibility path is not an HDR color-reference preview; HDR output uses the independent FFmpeg pipeline.

## Required Windows device acceptance

The included Windows workflow and `scripts/verify-windows.ps1` make the automated checks repeatable. They have not been run on a remote Windows runner during this session. Before considering a public production release, verify on the intended machines:

1. Extract the full portable folder, launch without development tools, and open local files through both drag/drop and native dialogs, including Unicode paths. Repeat with a standard non-administrator account.
2. Run `scripts/verify-windows.ps1`. Check H.264, HEVC, VP9, portrait, multiple audio tracks, and long captures with the bundled Windows engines.
3. Play representative 1080p120 and 4K60 recordings on NVIDIA, AMD and Intel devices. Measure dropped frames, audio synchronization, memory use and seeking latency; confirm the monitor refresh rate. No sustained-rate measurement is claimed here.
4. Verify fullscreen, PiP always-on-top, restored placement, multi-monitor DPI changes, window resizing, hardware decoding and each hardware export encoder. Automatic export must fall back to CPU after an unavailable encoder.
5. Verify audible output, device changes, HDR tone mapping and 10-bit output on appropriate media/hardware. Dynamic HDR export remains intentionally guarded. Static HDR mastering metadata is not guaranteed to survive re-encoding; this is not an HDR mastering tool.
6. Check screen-reader navigation, keyboard focus, long filenames, native save dialog cancellation, low disk space, and closing during an active export. Cancel must remove partial work and never replace the source.

The executable is unsigned. The package includes complete application source separately, dependency licenses, pinned engine download hashes, and SHA256SUMS.txt for the portable contents.

# Native media architecture

## Boundaries

The UI is Rust/egui/eframe with the Glow backend. egui provides the window, input events, focus and accessibility nodes; it draws nothing the person sees. Every frame, `ui/cell.rs` collects the whole interface into one textured mesh in device-pixel coordinates: glyphs come from embedded bitmap strikes (one per display-scale step, chosen in `cell::pick`), and fills and rules sample a solid texel of the same atlas. Bars and sheets are built as data (`kit::Line` of `kit::Item`) carrying `Act` values; `kit::arrange` places them in whole cells and `App::act` is the only place an action is carried out. libmpv owns video/audio synchronization, decoding, seeking and frame stepping. Its OpenGL render API produces a GPU framebuffer that Cutline composites into the video surface. The surface is retained and sized to the pixel density the viewport shows, capped at source resolution; mpv renders into it only when it reports a changed frame, and interface repaints reuse it. Crop preview copies the selected rectangle from that surface; it does not send every decoded frame through Rust or encode a temporary preview movie.

The thin C ABI adapter retains the loaded library until all client and render objects are destroyed. Rendering checks GL-thread ownership. Update callbacks request an egui repaint and do no media work. Shutdown removes the callback and frees the render context before releasing the client. The UI renderer's OpenGL state is reset at the media boundary. Playback commands are asynchronous. Detected software rasterizers use mpv's compatibility path for SDR preview; hardware drivers keep its normal color/scaling pipeline. Exports are independent of preview rendering.

Probe/export work uses separate processes and worker threads. File paths are arguments, never command strings or shell fragments. On Windows, engine files resolve only beside the executable or in its tools directory. The app does not scan PATH for an arbitrary DLL. Local development uses platform-installed libraries on Linux.

## Media invariants

- Source presentation seconds are the timeline authority. Fractional frame rates remain rational. Constant-rate gesture positions snap to frames; VFR values are not rounded to an average-rate frame grid.
- A trim is an inclusive start and exclusive end within the source duration.
- Crop coordinates are relative to the oriented, square-pixel display frame. Even dimensions/offsets ensure predictable 4:2:0 encoder geometry.
- Export plans clone the media, edit and settings. Subsequent UI changes cannot mutate an active job.
- FFmpeg uses accurate input seek plus output duration. No output `-r`, `fps` filter, size preset or implicit resampling is introduced.
- The encoder writes a randomly named temporary sibling, drains progress/diagnostics concurrently, and is killed and reaped on cancellation.
- Verification checks dimensions, duration, constant frame rate, audio-track count and HDR tags before a no-clobber final publication.
- Invalid input, a missing dependency, timeout, encoder failure, verification failure or an existing destination produces a recoverable error. Source bytes remain untouched.

## Scope and ownership

There is one media item, one trim and one rectangular crop. History is a bounded local stack. Crop mode owns an explicit draft. Session recovery is keyed to the source path, size and modification time. There is no generalized project graph, asset database, multitrack mixer, compositing engine, network server, authentication or telemetry.

Product visuals and behavior are specified in DESIGN.md and UX-CONTRACT.md. tests/media_roundtrip.rs validates real encoded outputs; model tests validate bounds, time parsing and history; timeline tests validate bracket hit resolution, ruler spacing, label spacing and zoom anchoring; cell tests validate strike selection per display scale, strike integrity and text fitting; kit tests validate line wrapping. Windows packaging hashes downloads and includes the exact lockfile and notices. Cross-build evidence is not a substitute for target-device playback and encoder validation.

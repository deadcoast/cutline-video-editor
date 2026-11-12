# Third-party components

Cutline is a Rust application. It embeds egui/eframe for native UI, dynamically loads libmpv for playback, and launches FFmpeg/ffprobe as separate processes for probing and encoding. No Chromium, Electron or webview is included.

| Component | Packaged version | Upstream / source |
|---|---|---|
| egui / eframe | 0.33.3 | https://github.com/emilk/egui/tree/0.33.3 |
| libmpv | mpv git 6a262d11f6; shinchiro build 20261020, x86_64 | https://github.com/mpv-player/mpv/commit/6a262d11f6 |
| mpv Windows build recipe | shinchiro mpv-winbuild-cmake 20261020 | https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20261020 |
| FFmpeg / ffprobe | Gyan FFmpeg 9.0.2 essentials | https://www.gyan.dev/ffmpeg/builds/ and https://ffmpeg.org/releases/ |
| Vulkan loader | LunarG 1.4.363.0, x64 | https://github.com/KhronosGroup/Vulkan-Loader/tree/v1.4.363 and https://vulkan.lunarg.com/sdk/home |
| Rust dependencies | Exact versions in Cargo.lock | Crates.io source archives; their license texts are retained in licenses/rust-dependencies |

The media archive URLs and SHA-256 hashes are pinned in `tools.lock.json`. The original mpv archive is `mpv-dev-x86_64-20261020-git-6a262d11f6.7z`. The FFmpeg archive is `ffmpeg-9.0.2-essentials_build.zip`. Executable checksums are in the portable package's SHA256SUMS.txt.

The UI libraries are MIT/Apache-2.0. The supplied media builds contain GPL-enabled components. The application's source is supplied under GPL-3.0-or-later, with the GPL text included. The portable package retains the FFmpeg distribution license; the original mpv build recipe and exact upstream revision are linked above. Upstream dependencies retain their own copyright and license notices.

Technical references used in implementation:

- https://mpv.io/manual/stable/ — playback commands, hardware decode, loop and timing behavior.
- https://github.com/mpv-player/mpv/blob/master/include/mpv/render_gl.h — OpenGL state and render API contract.
- https://ffmpeg.org/ffmpeg.html — accurate seek, stream mapping, frame timestamp passthrough and encoder progress.
- https://ffmpeg.org/ffmpeg-filters.html — crop and square-pixel normalization.
- https://docs.rs/eframe/0.33.3/eframe/ — native app lifecycle and custom OpenGL paint callbacks.

The official Vulkan loader DLL is included locally because the mpv DLL imports it. It does not install drivers or require a system-wide runtime installer. Its Apache-2.0/MIT notices are included in licenses/VulkanRT-License.txt.

# Cutline maintenance contract

Read README.md, DESIGN.md and UX-CONTRACT.md before changing behavior.

- This is a Windows-first Rust video viewer/crop/trim editor. Preserve the single-video workflow and retracting controls.
- No mocked media, placeholder execution paths, TODO implementations or silent frame-rate conversion.
- Preserve the original file. Validate geometry. Keep export arguments structured and shell-free. Publish outputs only after verification and without overwriting existing files.
- Keep libmpv unsafe code in media/player.rs with lifetime/thread explanations. Respect its OpenGL state contract. Test repeated launches as well as a fresh process.
- Run cargo fmt --check, cargo clippy --all-targets -- -D warnings, and the relevant real-media tests. Keep the lockfile.
- Verify native UI states after layout/gesture changes. Do not replace native inspection with an HTML mockup.
- Treat Windows cross-compilation, native Linux runtime checks, Windows device tests and GPU performance measurement as distinct evidence.
- Do not introduce server accounts, telemetry, webviews, effect suites or dashboard layouts.

# Interaction and media contract

Authority: the Windows Video Editor brief supplied on 2026-10-20. The application has one local workflow: open → view → select trim/crop → export a new file. No accounts or remote data are involved.

## Canonical UI Map

| Capability | Canonical owner | Source of truth | Allowed variants | Verification |
|---|---|---|---|---|
| Select/Listbox | kit option set (keys on one line, chosen one bracketed) | DESIGN.md and this contract | All options visible at once; unavailable options dimmed; Tab and Enter reach each | Native UI inspection |
| Form | kit field and meter on a deck line or a sheet | DESIGN.md and this contract | Typed in place; Enter/Tab commit, Escape restores; a rejected value leaves the edit unchanged and reports why | Trim/crop/export interaction checks |
| Scrollbar | none | DESIGN.md and this contract | No scrolling surface exists; lines wrap and sheets fit the minimum window | Source review |
| Toast | Status lines under the top bar | DESIGN.md and this contract | Persistent NOTE/ERROR with DISMISS; errors do not erase edits | Failure/cancel checks |
| Timeline | Timeline + model::Trim | DESIGN.md and this contract | Start inclusive, end exclusive; never crossing handles; numeric alternative | Unit/property/media tests |
| Crop | CropEditor + model::Crop | DESIGN.md and this contract | Move, resize, aspect presets and numeric controls; valid even-pixel region | Property tests and native drag |

Open uses a worker and a generation identifier; superseded metadata cannot replace a newer selection. A failed open retains the current video. The parser has a deadline and bounded output. Playback loading is independent from probing.

Changes affect an edit recipe, never the source. Completed edits are saved under the Windows local application-data directory, keyed to path, source length and modification time; stale or invalid recipes are ignored. Undo/redo is bounded to 100 gestures. Crop edits are one draft until Done; Cancel restores the previous crop.

Pointer gestures reveal controls. Tab keeps them visible for keyboard use. Every drag has a numerical or shortcut alternative: the IN and OUT registers and the crop X/Y/W/H fields are typed in place, and wheel or Up/Down steps them. Escape closes an open field first, then cancels the active crop, then leaves fullscreen/PiP. A sheet blocks the bars and the video beneath it. There are no tooltips: each control prints its own key, and a filename cut to fit the bar ends in an ellipsis and remains whole in the native window title.

Export captures an immutable media/edit/settings snapshot. The user chooses a destination using the native Save dialog. Existing files, including the source, cannot be replaced. Progress is measured from encoder presentation time; completion is reported only after metadata verification and atomic no-clobber publication. Cancellation kills and reaps the encoder, removes the temporary file, and leaves the edit available. Closing during export offers Keep exporting or Cancel export and close; the latter waits for cleanup.

Output preserves frame timestamps. Exact trimming uses FFmpeg accurate input seeking and re-encoding. A crop is applied after orientation and sample-aspect normalization. Multiple audio tracks are retained unless Remove audio is selected. Subtitles and chapters are omitted and labeled as such in the export dialog.

Supported output combinations: H.264 or HEVC in MP4/MOV/MKV; VP9 in WebM/MKV. HDR selects 10-bit HEVC by default. H.264 is disabled for HDR. Dynamic HDR (Dolby Vision/HDR10+ metadata) is guarded because this pipeline cannot promise to retain it. The interface does not claim HDR mastering capability.

Automatic encoding tries NVIDIA, Intel and AMD, then CPU. Explicit device selection reports an error instead of switching silently. Software encoding is quality-oriented; high resolution playback/export speed depends on hardware, decoder support and drivers.

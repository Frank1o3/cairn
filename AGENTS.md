# Cairn contributor guidance

Cairn is a Linux file manager written in Rust with GTK4 and GIO. Keep system integration native to the desktop where practical: use GIO for file, mount, trash, clipboard, and application-launch behavior instead of reimplementing those protocols.

## Structure

- Keep `src/ui.rs` as shared UI state and navigation glue. Put window construction in `src/ui/window.rs`, listing and row construction in `src/ui/listing.rs`, and user-triggered file actions in `src/ui/actions.rs`.
- Keep built-in GTK styles in `src/ui/style/` and load user styles from the Cairn config directory. Put color tokens in `definitions.css`, component rules in `style.css`, and keep widget hierarchy and behavior in Rust.
- Keep modules focused. Treat 1,500 lines as a firm signal to split a file into cohesive modules; prefer substantially smaller files when that improves separation.
- Keep filesystem policy in `src/operations.rs` and `src/filesystem.rs`, rather than embedding it in GTK callbacks.
- Keep user preferences backward-compatible when adding settings, and persist them through `src/settings.rs`.
- Preserve old user configuration when changing formats; migrate in place without deleting the prior file.

## Linux safety and behavior

- Never run Cairn itself with elevated privileges and never add implicit `sudo` or root execution. Launch files as the logged-in user. Any future privileged operation must be narrow, explicit, and separately reviewed.
- Diagnostics should be read-only by default. Cleanup must be scoped, explain what will be removed, respect ownership and mounts, and require a clear user action. Do not guess that a cache is safe to delete because a process appears idle.
- Do not follow symbolic links while recursively traversing directories unless the operation explicitly requires it; handle optional GIO attributes defensively.
- Prefer GIO APIs for local and GVFS locations. Make unsupported remote operations clear to the user instead of silently treating a URI as a local path.
- Keep potentially slow scans and media processing off GTK's main thread, and expose progress or cancellation for long-running work as those flows are developed.

## Product direction

Use `ROADMAP.md` as the durable project plan. Keep the file manager focused on managing files: use the desktop's default app for full media playback, and prioritize lightweight previews. Treat semantic or model-assisted search as a later, optional, offline feature with explicit resource and privacy limits.

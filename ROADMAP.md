# Cairn roadmap

Cairn aims to combine a clear, adaptable everyday file workflow with the depth of Thunar and Dolphin and the familiar polish of Windows 11, while remaining a Linux-native Rust and GTK application. This is an evolving plan, not a promise that every item belongs in one release.

## Working principles

- Prefer GIO/GVFS, desktop MIME handling, user directories, and the active GTK theme over distribution-specific assumptions.
- Keep controls stable while the content pane absorbs spare window space. Let users resize the sidebar, but do not make it grow just because the window got larger.
- Run files only after an explicit user action and as the current user. Cairn itself must never require root privileges.
- Keep diagnostics read-only. Make cleanup scope and consequences clear before removing anything.
- Prefer fast, lightweight previews and desktop handoff over building a full media player into the file manager.
- Keep modules cohesive and generally below 1,500 lines.

## Milestone 1: dependable daily file management

- [x] Separate window construction, file actions, and directory listing from the main UI module.
- [x] Guard optional GIO symlink attributes and repair copy/duplicate redo behavior.
- [x] Move GTK styles into an embedded default stylesheet that is materialized for user editing.
- [x] Support user CSS imports and JSON preferences with migration from the existing INI file.
- [x] Keep the places sidebar and toolbar terminal control from stretching with the window.
- [x] Add an explicit context action to run executable files in a terminal as the logged-in user; cover binaries, AppImages, and executable shell/Python scripts.
- [ ] Review file-operation edge cases: partial transfers, conflict handling, trash restore, symlink traversal, and undo failure recovery.
- [ ] Add cancellable, non-blocking operation progress for large copies, moves, archive jobs, and scans.
- [ ] Add drag-and-drop with clear copy/move feedback and URI-list interoperability.

## Milestone 2: previews and navigation polish

- [ ] Keep image previews bounded and asynchronous; cover representative image formats and large files.
- [ ] Generate first-frame video thumbnails for common formats when a suitable optional system tool or library is available. Fall back cleanly when it is not installed.
- [ ] Keep audio files represented by audio icons and open them with the desktop's default application.
- [ ] Continue to use the desktop's default video player rather than embedding a full player unless a concrete workflow justifies the added scope.
- [ ] Improve recursive search, keyboard navigation, selection behavior, and large-directory responsiveness.

## Milestone 3: storage diagnostics

- [ ] Add a read-only storage analysis view for a selected filesystem, removable drive, or GIO location.
- [ ] Show largest directories and files, with drill-down charts and scan progress/cancellation.
- [ ] Avoid following symlinks by default, identify unreadable locations, and show when results are incomplete.
- [ ] Support GVFS-backed devices when their backend provides the required metadata.

## Milestone 4: safe cleanup

- [ ] Start with user-owned cache locations and preview the exact paths and total size before cleanup.
- [ ] Do not infer safety from process activity alone. Use known cache contracts or ask the user to close the relevant application.
- [ ] Keep system-wide cleanup out of the default app. Any future privileged action must use a narrowly scoped desktop authorization mechanism and explain its effect.

## Milestone 5: search

- [ ] Improve indexed and fuzzy filename search before adding semantic search.
- [ ] Explore optional, offline description-based search only after the conventional search experience is reliable.
- [ ] Keep any model small, opt-in, privacy-preserving, resource-bounded, and removable without affecting normal file search.

## Milestone 6: distribution and release quality

- [ ] Define and document the minimum supported GTK/GIO versions and optional runtime tools.
- [ ] Build and package for major Linux families, beginning with Arch packages and Debian/Ubuntu packages, then add other distributions based on maintainers and CI capacity.
- [ ] Add reproducible build checks, packaging validation, and manual desktop integration checks to the release process.
- [ ] Document install, uninstall, desktop entry, MIME integration, GVFS backends, and optional preview dependencies.

# Cairn

Cairn is a Rust and GTK4 file manager started with Linux desktop workflows in mind. It uses GTK's theme colors so its interface follows the active desktop theme.

## Current features

- Browse folders from a places sidebar, navigate back and forward, or enter a path with **Ctrl+L**.
- Use clickable Home, Downloads, Documents, Pictures, Music, Videos, and File system shortcuts.
- Bookmark frequently used local or GVFS locations; bookmarks persist in the user settings.
- Discover GIO volumes and mounted locations as they appear; mount unmounted volumes and safely eject or unmount devices from the sidebar.
- Browse local and GVFS-backed locations through GIO, including remote URIs when the matching GVFS backend is installed (for example, SFTP or SMB).
- Filter the current folder by name with **Ctrl+F**.
- Multi-select items and use **Ctrl+C/X/V**, **F2**, **Delete**, and **Shift+Delete** for copy, cut, paste, rename, Trash, and permanent deletion.
- Right-click a file or folder for contextual open, copy, cut, rename, Trash, delete, and Properties actions; folders can also open in a new tab or detached terminal.
- Copy local paths or remote URIs with **Copy Location** in the context menu.
- Select files by wildcard pattern with **Ctrl+S** (for example, `*.rs`) or invert the current selection with **Ctrl+Shift+A**.
- Share copy and cut data through GTK's URI-list and GNOME file clipboard formats for interoperability with other desktop apps.
- Open files with the desktop's default application.
- Create empty files and folders, inspect basic file properties, and sort by name, size, or modified date.
- Save preferences for hidden files, folder ordering, sort mode, single-click activation, and Trash confirmations.
- Work across multiple tabs, switch with **Ctrl+Tab** / **Ctrl+Shift+Tab**, and search each tab independently.
- See file sizes, Unix permission bits, symbolic links, hidden files, and Git repositories in the listing.
- Create folders from the toolbar.
- Open the current folder in an installed terminal emulator. Cairn starts the terminal in a new session with standard input and output detached, so closing Cairn does not close that terminal.
- Supports Kitty, Alacritty, Foot, WezTerm, Konsole, Xfce Terminal, GNOME Terminal, and xterm, trying installed terminals in that order.

## Build and run

Install Rust and the GTK4 development files for your distribution, then run:

```sh
cargo run
```

The window opens at 1280×820 and keeps a practical 820×560 minimum so it remains usable in a floating layout. Its file pane expands with the window, and the address and search controls release space as the window narrows. The application currently targets Linux. GVFS backends are supplied by the desktop distribution and are not bundled by Cairn.

## Direction

Cairn is intended to make developer workflows easy to discover while staying approachable for people learning Linux. Advanced permissions editing, undo history, drag and drop, archive handling, and thumbnail previews remain future work.

# Cairn

Cairn is a Rust and GTK4 file manager started with Linux desktop workflows in mind. It uses GTK's theme colors so its interface follows the active desktop theme.

## Current features

- Browse folders from a places sidebar, navigate back and forward, or enter a path with **Ctrl+L**.
- Use clickable Home, Downloads, Documents, Pictures, Music, Videos, File system, and Trash shortcuts.
- Middle-click or right-click a place or bookmark to open it in a new tab.
- Right-click the Trash shortcut to empty it with confirmation.
- Full Trash integration: inspect Trash, restore files to their original location, or empty the Trash with confirmation.
- Bookmark frequently used local or GVFS locations; bookmarks persist in the user settings.
- Discover GIO volumes and mounted locations as they appear; mount unmounted volumes and safely eject or unmount devices from the sidebar.
- Browse local and GVFS-backed locations through GIO, including remote URIs when the matching GVFS backend is installed (for example, SFTP or SMB).
- Open the Network location or use Connect to Server for SMB, FTP, SFTP, and WebDAV URIs; matching GVFS backends must be installed for each protocol.
- Filter the current folder by name with **Ctrl+F**, with an inline clear button and **Escape** shortcut.
- Multi-select items and use **Ctrl+C/X/V**, **Ctrl+D** (duplicate), **F2**, **Delete**, and **Shift+Delete** for copy, cut, paste, duplicate, rename, Trash, and permanent deletion.
- Right-click a file or folder for contextual open, open with text editor, copy, cut, duplicate, rename, Trash, delete, compression, extraction, and Properties actions; folders can also open in a new tab or detached terminal.
- Right-click an executable file to run it in a terminal as the current user; folders offer **Open in Terminal Here**.
- Right-click empty space in the file view for quick access to New Folder, New File, Paste, Select All, Open in Terminal, and folder Properties.
- Archive management: extract `.zip`, `.tar.gz`, `.tar.xz`, `.tar.bz2`, `.tar.zst`, and `.tar` archives with **Extract Here** or **Extract to Folder**, and compress selected items into `.zip` or `.tar.gz` archives.
- Copy local paths or remote URIs with **Copy Location** in the context menu.
- Keyboard navigation: use **Alt+Up** for parent folder, **Alt+Left/Right** for back/forward, **Ctrl+N** for new file, **Ctrl+R** / **F5** to reload, and **Enter** to open items.
- Status bar displays real-time disk free space when idle, and selected item counts and sizes when items are selected.
- Select files by wildcard pattern with **Ctrl+S** (for example, `*.rs`) or invert the current selection with **Ctrl+Shift+A**.
- Share copy and cut data through GTK's URI-list and GNOME file clipboard formats for interoperability with other desktop apps.
- Open files with the desktop's default application or installed text editors.
- Create empty files and folders, inspect detailed file properties (including async folder size calculations, full modification timestamps, and symlink targets), and sort by name, size, modified date, or file type (ascending or descending).
- Customize system or dark appearance, comfortable or compact list density, hidden files, folder ordering, sort mode and direction, thumbnails, single-click activation, and Trash confirmations; preferences, window size, sidebar width, and maximized state persist between sessions.
- Work across multiple tabs, switch with **Ctrl+Tab** / **Ctrl+Shift+Tab**, and search each tab independently.
- See file sizes, Unix permission bits, symbolic links, hidden files, and Git repositories in the listing.
- Preview image files inline in the file list, with a setting to turn thumbnails off.
- Create folders from the toolbar.
- Open the current folder in an installed terminal emulator. Cairn starts the terminal in a new session with standard input and output detached, so closing Cairn does not close that terminal.
- Supports Kitty, Alacritty, Foot, WezTerm, Konsole, Xfce Terminal, GNOME Terminal, and xterm, trying installed terminals in that order.

## Build and run

Install Rust and the GTK4 development files for your distribution, then run:

```sh
cargo run
```

The window opens at 1280×820 and keeps a practical 820×560 minimum so it remains usable in a floating layout. Its file pane expands with the window while the places sidebar and toolbar actions keep their selected widths. The application currently targets Linux. GVFS backends are supplied by the desktop distribution and are not bundled by Cairn.

## Developer customization

On first launch Cairn creates its user configuration directory under `$XDG_CONFIG_HOME/cairn` (normally `~/.config/cairn`), including `settings.json`, `style.css`, and `definitions.css`. The previous `settings.ini` is read and migrated when no JSON settings file exists; it is left in place. Cairn also adds the `definitions.css` import to older user stylesheets without replacing their existing rules.

The stylesheet uses GTK CSS and continues to follow the active desktop theme by default. `style.css` imports `definitions.css`, which defines Cairn color tokens from GTK's theme colors. Place palette files next to `style.css` or use an absolute file URL, then restart Cairn to apply changes. A palette template can emit GTK-compatible token definitions; the stylesheet can then reference its colors and override component sizes, for example:

```css
@import url("wallust-palette.css");

.terminal-button { min-width: 180px; }
.sidebar-panel { min-width: 200px; }
```

No Wallust dependency is required.

## Direction

Cairn is in an ongoing modernization step: improving previews, network browsing, everyday workflows, and a cleaner, softly layered interface while keeping its developer-friendly tools discoverable. See [ROADMAP.md](ROADMAP.md) for planned diagnostics, previews, search, safety boundaries, and distribution support.

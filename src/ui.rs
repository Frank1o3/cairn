use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, CheckButton, DropDown, Entry,
    GestureClick, Grid, HeaderBar, Image, Label, ListBox, ListBoxRow, MenuButton, Notebook,
    Orientation, Paned, Popover, ScrolledWindow, Separator,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::{filesystem, operations, settings, terminal};

const APP_ID: &str = "dev.cairn.FileManager";

struct AppState {
    current: gio::File,
    back: Vec<gio::File>,
    forward: Vec<gio::File>,
    query: String,
    rows: Option<ListBox>,
    notebook: Option<Notebook>,
    tabs: Vec<TabState>,
    active_tab: usize,
    path_entry: Option<Entry>,
    search_entry: Option<Entry>,
    status_label: Option<Label>,
    back_button: Option<Button>,
    forward_button: Option<Button>,
    devices: Option<GtkBox>,
    bookmarks: Option<GtkBox>,
    volume_monitor: Option<gio::VolumeMonitor>,
    load_generation: u64,
    mount_parent: Option<ApplicationWindow>,
    show_hidden_check: Option<CheckButton>,
    bookmark_button: Option<Button>,
    preferences: settings::Preferences,
    clipboard: Vec<gio::File>,
    clipboard_is_cut: bool,
    undo_stack: Vec<operations::UndoAction>,
    redo_stack: Vec<operations::UndoAction>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            current: gio::File::for_path("/"),
            back: Vec::new(),
            forward: Vec::new(),
            query: String::new(),
            rows: None,
            notebook: None,
            tabs: Vec::new(),
            active_tab: 0,
            path_entry: None,
            search_entry: None,
            status_label: None,
            back_button: None,
            forward_button: None,
            devices: None,
            bookmarks: None,
            volume_monitor: None,
            load_generation: 0,
            mount_parent: None,
            show_hidden_check: None,
            bookmark_button: None,
            preferences: settings::Preferences::default(),
            clipboard: Vec::new(),
            clipboard_is_cut: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }
}

#[derive(Clone)]
struct FileRowData {
    file: gio::File,
    is_dir: bool,
    info: gio::FileInfo,
}

struct TabState {
    current: gio::File,
    back: Vec<gio::File>,
    forward: Vec<gio::File>,
    query: String,
    rows: ListBox,
    title: Label,
}

pub fn run() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    install_css();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let preferences = settings::Preferences::load();
    let window_width = preferences.window_width;
    let window_height = preferences.window_height;
    let sidebar_width = preferences.sidebar_width;
    let window_maximized = preferences.window_maximized;
    if let Some(gtk_settings) = gtk::Settings::default() {
        gtk_settings.set_gtk_application_prefer_dark_theme(preferences.prefer_dark);
    }
    let state = Rc::new(RefCell::new(AppState {
        current: gio::File::for_path(home.clone()),
        preferences,
        ..Default::default()
    }));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Cairn")
        .default_width(window_width)
        .default_height(window_height)
        .build();
    // GTK's natural minimum for the individual controls is too small to make
    // a useful floating file-manager window. Keep a practical floor while
    // allowing the main pane and its entries to expand with larger windows.
    window.set_size_request(820, 560);

    let header = HeaderBar::new();
    let back = tool_button("go-previous-symbolic", "Back");
    let forward = tool_button("go-next-symbolic", "Forward");
    let up = tool_button("go-up-symbolic", "Parent folder");
    let refresh = tool_button("view-refresh-symbolic", "Refresh");
    let terminal = menu_action_button("utilities-terminal-symbolic", "Open in Terminal");
    terminal.add_css_class("suggested-action");

    let location = Entry::new();
    location.set_placeholder_text(Some("Enter a folder path"));
    location.set_hexpand(true);
    location.set_icon_from_icon_name(
        gtk::EntryIconPosition::Primary,
        Some("folder-open-symbolic"),
    );
    location.add_css_class("location-entry");
    header.pack_start(&back);
    header.pack_start(&forward);
    header.pack_start(&up);
    let brand = GtkBox::new(Orientation::Horizontal, 7);
    let brand_icon = Image::from_icon_name("system-file-manager-symbolic");
    brand_icon.set_pixel_size(18);
    let brand_name = Label::new(Some("Cairn"));
    brand_name.add_css_class("brand-title");
    brand.append(&brand_icon);
    brand.append(&brand_name);
    header.set_title_widget(Some(&brand));
    header.pack_end(&terminal);
    header.pack_end(&refresh);

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("app-shell");
    root.set_margin_top(8);
    root.set_margin_bottom(8);
    root.set_margin_start(8);
    root.set_margin_end(8);

    let pane = Paned::new(Orientation::Horizontal);
    pane.add_css_class("workspace-pane");
    pane.set_hexpand(true);
    pane.set_vexpand(true);
    pane.set_shrink_start_child(true);
    pane.set_shrink_end_child(true);
    pane.set_position(sidebar_width);

    let sidebar = GtkBox::new(Orientation::Vertical, 8);
    sidebar.set_margin_top(14);
    sidebar.set_margin_bottom(14);
    sidebar.set_margin_start(10);
    sidebar.set_margin_end(10);
    sidebar.set_size_request(168, -1);
    let places_title = Label::new(Some("PLACES"));
    places_title.set_xalign(0.0);
    places_title.add_css_class("section-label");
    sidebar.append(&places_title);

    let places = GtkBox::new(Orientation::Vertical, 2);
    let home_path = state.borrow().current.clone();
    let mut shortcuts = vec![("user-home-symbolic", "Home", home_path)];
    if let Some(user_dirs) = glib::user_special_dir(glib::UserDirectory::Downloads) {
        shortcuts.push((
            "folder-download-symbolic",
            "Downloads",
            gio::File::for_path(user_dirs),
        ));
    }
    for (kind, title, folder) in [
        (
            glib::UserDirectory::Documents,
            "Documents",
            "folder-documents-symbolic",
        ),
        (
            glib::UserDirectory::Pictures,
            "Pictures",
            "folder-pictures-symbolic",
        ),
        (glib::UserDirectory::Music, "Music", "folder-music-symbolic"),
        (
            glib::UserDirectory::Videos,
            "Videos",
            "folder-videos-symbolic",
        ),
    ] {
        if let Some(path) = glib::user_special_dir(kind) {
            shortcuts.push((folder, title, gio::File::for_path(path)));
        }
    }
    shortcuts.push((
        "drive-harddisk-symbolic",
        "File system",
        gio::File::for_path("/"),
    ));
    shortcuts.push((
        "user-trash-symbolic",
        "Trash",
        gio::File::for_uri("trash:///"),
    ));
    shortcuts.push((
        "network-workgroup-symbolic",
        "Network",
        gio::File::for_uri("network:///"),
    ));
    for (icon, title, file) in shortcuts {
        let row = place_row(icon, title);
        places.append(&row);
        if title == "Trash" {
            connect_trash_place_menu(&row, &window, &state);
        } else {
            connect_place_context_menu(&row, file.clone(), &state);
        }
        let state = state.clone();
        row.connect_clicked(move |_| navigate_to(&state, file.clone(), true));
    }
    sidebar.append(&places);

    let bookmarks_title = Label::new(Some("BOOKMARKS"));
    bookmarks_title.set_xalign(0.0);
    bookmarks_title.add_css_class("section-label");
    bookmarks_title.set_margin_top(14);
    sidebar.append(&bookmarks_title);
    let bookmarks = GtkBox::new(Orientation::Vertical, 2);
    sidebar.append(&bookmarks);

    let devices_title = Label::new(Some("DEVICES"));
    devices_title.set_xalign(0.0);
    devices_title.add_css_class("section-label");
    devices_title.set_margin_top(14);
    sidebar.append(&devices_title);
    let devices = GtkBox::new(Orientation::Vertical, 2);
    sidebar.append(&devices);
    let side_scroll = ScrolledWindow::new();
    side_scroll.add_css_class("sidebar-panel");
    side_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    side_scroll.set_child(Some(&sidebar));
    side_scroll.set_size_request(168, -1);
    pane.set_start_child(Some(&side_scroll));

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.set_hexpand(true);
    content.set_vexpand(true);
    let toolbar = GtkBox::new(Orientation::Horizontal, 8);
    toolbar.set_margin_top(10);
    toolbar.set_margin_bottom(10);
    toolbar.set_margin_start(16);
    toolbar.set_margin_end(16);
    toolbar.add_css_class("address-toolbar");
    location.set_width_chars(16);
    location.set_hexpand(true);
    let search = Entry::new();
    search.set_placeholder_text(Some("Search this folder…"));
    search.set_icon_from_icon_name(
        gtk::EntryIconPosition::Primary,
        Some("system-search-symbolic"),
    );
    search.set_icon_from_icon_name(
        gtk::EntryIconPosition::Secondary,
        Some("edit-clear-symbolic"),
    );
    search.connect_icon_press(|entry, position| {
        if position == gtk::EntryIconPosition::Secondary {
            entry.set_text("");
        }
    });
    search.set_width_chars(14);
    let new_folder = menu_action_button("folder-new-symbolic", "New Folder");
    new_folder.add_css_class("flat");
    let bookmark_button = tool_button("bookmark-new-symbolic", "Bookmark this location");
    let actions_button = MenuButton::new();
    actions_button.set_label("Files");
    let actions_popover = Popover::new();
    let action_list = GtkBox::new(Orientation::Vertical, 2);
    action_list.set_margin_top(6);
    action_list.set_margin_bottom(6);
    action_list.set_margin_start(6);
    action_list.set_margin_end(6);
    for (label, icon, action) in [
        ("Undo", "edit-undo-symbolic", FileAction::Undo),
        ("Redo", "edit-redo-symbolic", FileAction::Redo),
        ("Open", "document-open-symbolic", FileAction::Open),
        ("New Tab", "tab-new-symbolic", FileAction::NewTab),
        ("Close Tab", "window-close-symbolic", FileAction::CloseTab),
        ("Connect to Server…", "network-server-symbolic", FileAction::ConnectToServer),
        ("New Folder…", "folder-new-symbolic", FileAction::NewFolder),
        (
            "New Empty File…",
            "document-new-symbolic",
            FileAction::NewFile,
        ),
        ("Copy", "edit-copy-symbolic", FileAction::Copy),
        ("Cut", "edit-cut-symbolic", FileAction::Cut),
        ("Paste", "edit-paste-symbolic", FileAction::Paste),
        ("Duplicate", "edit-copy-symbolic", FileAction::Duplicate),
        (
            "Select by Pattern…",
            "edit-find-symbolic",
            FileAction::SelectByPattern,
        ),
        (
            "Invert Selection",
            "object-select-symbolic",
            FileAction::InvertSelection,
        ),
        ("Rename…", "document-edit-symbolic", FileAction::Rename),
        ("Move to Trash", "user-trash-symbolic", FileAction::Trash),
        (
            "Delete Permanently…",
            "edit-delete-symbolic",
            FileAction::Delete,
        ),
        ("Empty Trash…", "user-trash-symbolic", FileAction::EmptyTrash),
        (
            "Properties",
            "dialog-information-symbolic",
            FileAction::Properties,
        ),
    ] {
        let button = menu_action_button(icon, label);
        button.add_css_class("flat");
        let state = state.clone();
        let parent = window.clone();
        let popover = actions_popover.clone();
        button.connect_clicked(move |_| {
            popover.popdown();
            perform_file_action(action, &parent, &state);
        });
        action_list.append(&button);
    }
    actions_popover.set_child(Some(&action_list));
    actions_button.set_popover(Some(&actions_popover));

    let settings_button = MenuButton::new();
    settings_button.set_icon_name("emblem-system-symbolic");
    settings_button.set_tooltip_text(Some("File manager settings"));
    let settings_popover = Popover::new();
    let settings_box = GtkBox::new(Orientation::Vertical, 10);
    settings_box.set_margin_top(14);
    settings_box.set_margin_bottom(14);
    settings_box.set_margin_start(14);
    settings_box.set_margin_end(14);
    let settings_heading = Label::new(Some("FILE VIEW"));
    settings_heading.set_xalign(0.0);
    settings_heading.add_css_class("section-label");
    let appearance_label = Label::new(Some("Appearance"));
    appearance_label.set_xalign(0.0);
    let appearance = DropDown::from_strings(&["System", "Dark"]);
    appearance.set_selected(if state.borrow().preferences.prefer_dark { 1 } else { 0 });
    let density_label = Label::new(Some("List density"));
    density_label.set_xalign(0.0);
    let density = DropDown::from_strings(&["Comfortable", "Compact"]);
    density.set_selected(if state.borrow().preferences.compact_view { 1 } else { 0 });
    let show_hidden = CheckButton::with_label("Show hidden files");
    show_hidden.set_active(state.borrow().preferences.show_hidden);
    let directories_first = CheckButton::with_label("Folders first");
    directories_first.set_active(state.borrow().preferences.directories_first);
    let confirm_trash = CheckButton::with_label("Confirm before moving to Trash");
    confirm_trash.set_active(state.borrow().preferences.confirm_trash);
    let single_click = CheckButton::with_label("Open items with one click");
    single_click.set_active(state.borrow().preferences.open_on_single_click);
    let show_thumbnails = CheckButton::with_label("Show image thumbnails");
    show_thumbnails.set_active(state.borrow().preferences.show_thumbnails);
    let sort_label = Label::new(Some("Sort by"));
    sort_label.set_xalign(0.0);
    let sort_by = DropDown::from_strings(&["Name", "File size", "Date modified", "File type"]);
    sort_by.set_selected(match state.borrow().preferences.sort_by {
        settings::SortBy::Name => 0,
        settings::SortBy::Size => 1,
        settings::SortBy::Modified => 2,
        settings::SortBy::Type => 3,
    });
    let sort_reversed = CheckButton::with_label("Reverse sort order (descending)");
    sort_reversed.set_active(state.borrow().preferences.sort_reversed);
    settings_box.append(&settings_heading);
    settings_box.append(&appearance_label);
    settings_box.append(&appearance);
    settings_box.append(&density_label);
    settings_box.append(&density);
    settings_box.append(&show_hidden);
    settings_box.append(&directories_first);
    settings_box.append(&confirm_trash);
    settings_box.append(&single_click);
    settings_box.append(&show_thumbnails);
    settings_box.append(&sort_label);
    settings_box.append(&sort_by);
    settings_box.append(&sort_reversed);
    settings_popover.set_child(Some(&settings_box));
    settings_button.set_popover(Some(&settings_popover));
    toolbar.append(&location);
    toolbar.append(&search);
    toolbar.append(&new_folder);
    toolbar.append(&bookmark_button);
    toolbar.append(&actions_button);
    toolbar.append(&settings_button);
    content.append(&toolbar);
    content.append(&Separator::new(Orientation::Horizontal));

    let rows = ListBox::new();
    rows.set_selection_mode(gtk::SelectionMode::Multiple);
    rows.add_css_class("file-list");
    rows.set_activate_on_single_click(false);
    rows.set_vexpand(true);
    let scroll = ScrolledWindow::new();
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_child(Some(&rows));
    let notebook = Notebook::new();
    notebook.set_scrollable(true);
    let (tab_header, tab_title, tab_close) = tab_header("Home");
    notebook.append_page(&scroll, Some(&tab_header));
    content.append(&notebook);
    let footer = GtkBox::new(Orientation::Horizontal, 8);
    footer.add_css_class("statusbar");
    let status = Label::new(None);
    status.set_xalign(0.0);
    footer.append(&status);
    content.append(&Separator::new(Orientation::Horizontal));
    content.append(&footer);
    pane.set_end_child(Some(&content));
    root.append(&pane);
    window.set_titlebar(Some(&header));
    window.set_child(Some(&root));

    {
        let mut s = state.borrow_mut();
        s.rows = Some(rows.clone());
        s.notebook = Some(notebook.clone());
        s.tabs.push(TabState {
            current: gio::File::for_path(home.clone()),
            back: Vec::new(),
            forward: Vec::new(),
            query: String::new(),
            rows: rows.clone(),
            title: tab_title.clone(),
        });
        s.path_entry = Some(location.clone());
        s.search_entry = Some(search.clone());
        s.status_label = Some(status.clone());
        s.back_button = Some(back.clone());
        s.forward_button = Some(forward.clone());
        s.devices = Some(devices.clone());
        s.bookmarks = Some(bookmarks.clone());
        s.mount_parent = Some(window.clone());
        s.show_hidden_check = Some(show_hidden.clone());
        s.bookmark_button = Some(bookmark_button.clone());
    }

    let volume_monitor = gio::VolumeMonitor::get();
    state.borrow_mut().volume_monitor = Some(volume_monitor.clone());
    refresh_devices(&state, &window);
    refresh_bookmarks(&state);
    connect_file_rows(&rows, &state);
    connect_background_context_menu(&rows, &state);
    {
        let state = state.clone();
        notebook.connect_switch_page(move |notebook, _, page| {
            switch_tab(&state, notebook, page as usize);
        });
    }
    {
        let state = state.clone();
        let notebook = notebook.clone();
        let page = scroll.clone();
        tab_close.connect_clicked(move |_| close_tab(&state, &notebook, &page));
    }
    for connect in [
        volume_monitor.connect_volume_added({
            let state = state.clone();
            let window = window.clone();
            move |_, _| refresh_devices(&state, &window)
        }),
        volume_monitor.connect_volume_removed({
            let state = state.clone();
            let window = window.clone();
            move |_, _| refresh_devices(&state, &window)
        }),
        volume_monitor.connect_mount_added({
            let state = state.clone();
            let window = window.clone();
            move |_, _| refresh_devices(&state, &window)
        }),
        volume_monitor.connect_mount_removed({
            let state = state.clone();
            let window = window.clone();
            move |_, _| refresh_devices(&state, &window)
        }),
    ] {
        let _ = connect;
    }

    let st = state.clone();
    back.connect_clicked(move |_| go_back(&st));
    let st = state.clone();
    forward.connect_clicked(move |_| go_forward(&st));
    let st = state.clone();
    up.connect_clicked(move |_| {
        let parent = st.borrow().current.parent();
        if let Some(parent) = parent {
            navigate_to(&st, parent, true);
        }
    });
    let st = state.clone();
    refresh.connect_clicked(move |_| render(&st));
    let st = state.clone();
    terminal.connect_clicked(move |_| {
        let file = st.borrow().current.clone();
        let status = st.borrow().status_label.clone();
        if let Some(path) = file.path() {
            terminal::open_terminal(&path, status.as_ref());
        } else {
            set_status(&st, "A local shell cannot start in a remote location.");
        }
    });
    let st = state.clone();
    search.connect_changed(move |entry| {
        {
            let mut state = st.borrow_mut();
            state.query = entry.text().to_string();
            sync_active_tab(&mut state);
        }
        render(&st);
    });
    let st = state.clone();
    let parent_window = window.clone();
    new_folder.connect_clicked(move |_| create_folder_dialog(&parent_window, &st));
    let st = state.clone();
    bookmark_button.connect_clicked(move |_| toggle_bookmark(&st));
    let st = state.clone();
    show_hidden.connect_toggled(move |check| {
        st.borrow_mut().preferences.show_hidden = check.is_active();
        save_preferences(&st);
        render(&st);
    });
    let st = state.clone();
    directories_first.connect_toggled(move |check| {
        st.borrow_mut().preferences.directories_first = check.is_active();
        save_preferences(&st);
        render(&st);
    });
    let st = state.clone();
    confirm_trash.connect_toggled(move |check| {
        st.borrow_mut().preferences.confirm_trash = check.is_active();
        save_preferences(&st);
    });
    let st = state.clone();
    single_click.connect_toggled(move |check| {
        let mut state = st.borrow_mut();
        state.preferences.open_on_single_click = check.is_active();
        for tab in &state.tabs {
            tab.rows.set_activate_on_single_click(check.is_active());
        }
        drop(state);
        save_preferences(&st);
    });
    let st = state.clone();
    show_thumbnails.connect_toggled(move |check| {
        st.borrow_mut().preferences.show_thumbnails = check.is_active();
        save_preferences(&st);
        render(&st);
    });
    let st = state.clone();
    appearance.connect_selected_notify(move |dropdown| {
        let prefer_dark = dropdown.selected() == 1;
        st.borrow_mut().preferences.prefer_dark = prefer_dark;
        if let Some(gtk_settings) = gtk::Settings::default() {
            gtk_settings.set_gtk_application_prefer_dark_theme(prefer_dark);
        }
        save_preferences(&st);
    });
    let st = state.clone();
    density.connect_selected_notify(move |dropdown| {
        st.borrow_mut().preferences.compact_view = dropdown.selected() == 1;
        save_preferences(&st);
        render(&st);
    });
    let st = state.clone();
    sort_by.connect_selected_notify(move |dropdown| {
        st.borrow_mut().preferences.sort_by = match dropdown.selected() {
            1 => settings::SortBy::Size,
            2 => settings::SortBy::Modified,
            3 => settings::SortBy::Type,
            _ => settings::SortBy::Name,
        };
        save_preferences(&st);
        render(&st);
    });
    let st = state.clone();
    sort_reversed.connect_toggled(move |check| {
        st.borrow_mut().preferences.sort_reversed = check.is_active();
        save_preferences(&st);
        render(&st);
    });
    let st = state.clone();
    location.connect_activate(move |entry| {
        let raw = entry.text().to_string();
        let destination = if raw.contains("://") {
            Some(gio::File::for_uri(&raw))
        } else if raw == "~" {
            std::env::var_os("HOME").map(|home| gio::File::for_path(PathBuf::from(home)))
        } else if let Some(rest) = raw.strip_prefix("~/") {
            std::env::var_os("HOME").map(|home| gio::File::for_path(PathBuf::from(home).join(rest)))
        } else {
            let path = PathBuf::from(&raw);
            if path.is_absolute() {
                Some(gio::File::for_path(path))
            } else {
                Some(st.borrow().current.resolve_relative_path(&raw))
            }
        };
        if let Some(file) = destination {
            navigate_to(&st, file, true);
        }
    });
    window.add_controller(shortcuts_controller(&state, &window));
    render(&state);
    window.present();
    if window_maximized {
        window.maximize();
    }

    let state_close = state.clone();
    let pane_close = pane.clone();
    window.connect_close_request(move |window| {
        let mut state = state_close.borrow_mut();
        if !window.is_maximized() {
            state.preferences.window_width = window.width().max(820);
            state.preferences.window_height = window.height().max(560);
        }
        state.preferences.sidebar_width = pane_close.position().clamp(168, 800);
        state.preferences.window_maximized = window.is_maximized();
        drop(state);
        save_preferences(&state_close);
        glib::Propagation::Proceed
    });
}

fn shortcuts_controller(
    state: &Rc<RefCell<AppState>>,
    parent: &ApplicationWindow,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    let state = state.clone();
    let parent = parent.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
        let alt = modifiers.contains(gdk::ModifierType::ALT_MASK);

        if alt && key == gdk::Key::Up {
            let parent_dir = state.borrow().current.parent();
            if let Some(parent_dir) = parent_dir {
                navigate_to(&state, parent_dir, true);
            }
            return glib::Propagation::Stop;
        }
        if alt && key == gdk::Key::Left {
            go_back(&state);
            return glib::Propagation::Stop;
        }
        if alt && key == gdk::Key::Right {
            go_forward(&state);
            return glib::Propagation::Stop;
        }
        if control && !shift && key == gdk::Key::z {
            perform_file_action(FileAction::Undo, &parent, &state);
            return glib::Propagation::Stop;
        }
        if (control && shift && key == gdk::Key::z) || (control && key == gdk::Key::y) {
            perform_file_action(FileAction::Redo, &parent, &state);
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::d {
            perform_file_action(FileAction::Duplicate, &parent, &state);
            return glib::Propagation::Stop;
        }
        if control && !shift && key == gdk::Key::n {
            perform_file_action(FileAction::NewFile, &parent, &state);
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::t {
            new_tab(&parent, &state);
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::w {
            if let Some(notebook) = state.borrow().notebook.clone() {
                if let Some(page) = notebook.nth_page(notebook.current_page()) {
                    close_tab(&state, &notebook, &page);
                }
            }
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::Tab {
            let (notebook, count, current) = {
                let s = state.borrow();
                (s.notebook.clone(), s.tabs.len(), s.active_tab)
            };
            if let Some(notebook) = notebook {
                if count > 1 {
                    let next = if shift {
                        (current + count - 1) % count
                    } else {
                        (current + 1) % count
                    };
                    notebook.set_current_page(Some(next as u32));
                }
            }
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::l {
            if let Some(entry) = &state.borrow().path_entry {
                entry.grab_focus();
                entry.select_region(0, -1);
            }
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::f {
            if let Some(entry) = &state.borrow().search_entry {
                entry.grab_focus();
                entry.select_region(0, -1);
            }
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::c {
            perform_file_action(FileAction::Copy, &parent, &state);
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::x {
            perform_file_action(FileAction::Cut, &parent, &state);
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::v {
            perform_file_action(FileAction::Paste, &parent, &state);
            return glib::Propagation::Stop;
        }
        if control && !shift && key == gdk::Key::a {
            if let Some(rows) = &state.borrow().rows {
                rows.select_all();
            }
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::s {
            perform_file_action(FileAction::SelectByPattern, &parent, &state);
            return glib::Propagation::Stop;
        }
        if control && shift && key == gdk::Key::a {
            perform_file_action(FileAction::InvertSelection, &parent, &state);
            return glib::Propagation::Stop;
        }
        if control && shift && key == gdk::Key::n {
            create_folder_dialog(&parent, &state);
            return glib::Propagation::Stop;
        }
        if control && key == gdk::Key::h {
            let checkbox = state.borrow().show_hidden_check.clone();
            if let Some(checkbox) = checkbox {
                checkbox.set_active(!checkbox.is_active());
            }
            return glib::Propagation::Stop;
        }
        if key == gdk::Key::F2 {
            perform_file_action(FileAction::Rename, &parent, &state);
            return glib::Propagation::Stop;
        }
        if key == gdk::Key::Delete {
            perform_file_action(
                if shift {
                    FileAction::Delete
                } else {
                    FileAction::Trash
                },
                &parent,
                &state,
            );
            return glib::Propagation::Stop;
        }
        if key == gdk::Key::F5 || (control && key == gdk::Key::r) {
            render(&state);
            return glib::Propagation::Stop;
        }
        if key == gdk::Key::BackSpace {
            go_back(&state);
            return glib::Propagation::Stop;
        }
        if key == gdk::Key::Escape {
            if let Some(search) = &state.borrow().search_entry {
                if !search.text().is_empty() {
                    search.set_text("");
                    return glib::Propagation::Stop;
                }
            }
            if let Some(rows) = &state.borrow().rows {
                rows.unselect_all();
            }
            return glib::Propagation::Stop;
        }
        if (key == gdk::Key::Return || key == gdk::Key::KP_Enter) && !control && !alt && !shift {
            let path_focused = state
                .borrow()
                .path_entry
                .as_ref()
                .map(|e| e.has_focus())
                .unwrap_or(false);
            if !path_focused {
                perform_file_action(FileAction::Open, &parent, &state);
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    controller
}

#[derive(Clone, Copy)]
enum FileAction {
    Open,
    OpenInNewTab,
    OpenWithEditor,
    NewTab,
    CloseTab,
    ConnectToServer,
    NewFolder,
    NewFile,
    Copy,
    Cut,
    Paste,
    Duplicate,
    ExtractHere,
    ExtractTo,
    CompressZip,
    CompressTarGz,
    SelectAll,
    SelectByPattern,
    InvertSelection,
    CopyLocation,
    OpenTerminalHere,
    Rename,
    Trash,
    Delete,
    EmptyTrash,
    RestoreFromTrash,
    Undo,
    Redo,
    Properties,
}

fn selected_files(state: &Rc<RefCell<AppState>>) -> Vec<FileRowData> {
    let rows = state.borrow().rows.clone();
    rows.map(|rows| rows.selected_rows().iter().filter_map(row_path).collect())
        .unwrap_or_default()
}

fn perform_file_action(
    action: FileAction,
    parent: &ApplicationWindow,
    state: &Rc<RefCell<AppState>>,
) {
    match action {
        FileAction::Open => {
            let selected = selected_files(state);
            if selected.len() == 1 && selected[0].is_dir {
                navigate_to(state, selected[0].file.clone(), true);
            } else {
                for item in selected {
                    if !item.is_dir {
                        if let Err(error) = filesystem::open_file(&item.file) {
                            set_status(state, &format!("Could not open file: {error}"));
                        }
                    }
                }
            }
        }
        FileAction::OpenWithEditor => {
            let selected = selected_files(state);
            for item in selected {
                if !item.is_dir {
                    if let Err(error) = filesystem::open_with_text_editor(&item.file) {
                        set_status(state, &format!("Could not open with editor: {error}"));
                    }
                }
            }
        }
        FileAction::NewFolder => create_folder_dialog(parent, state),
        FileAction::NewFile => create_file_dialog(parent, state),
        FileAction::NewTab => new_tab(parent, state),
        FileAction::ConnectToServer => connect_to_server_dialog(parent, state),
        FileAction::OpenInNewTab => {
            if let Some(item) = selected_files(state).first() {
                let target = item.file.clone();
                new_tab(parent, state);
                navigate_to(state, target, false);
            }
        }
        FileAction::CloseTab => {
            if let Some(notebook) = state.borrow().notebook.clone() {
                if let Some(page) = notebook.nth_page(notebook.current_page()) {
                    close_tab(state, &notebook, &page);
                }
            }
        }
        FileAction::Copy | FileAction::Cut => {
            let selected = selected_files(state);
            if selected.is_empty() {
                set_status(state, "Select one or more files first.");
                return;
            }
            let mut s = state.borrow_mut();
            s.clipboard = selected.into_iter().map(|item| item.file).collect();
            s.clipboard_is_cut = matches!(action, FileAction::Cut);
            let count = s.clipboard.len();
            let files = s.clipboard.clone();
            let is_cut = s.clipboard_is_cut;
            drop(s);
            publish_clipboard(&files, is_cut);
            set_status(
                state,
                &format!(
                    "{} {count} item(s)",
                    if matches!(action, FileAction::Cut) {
                        "Cut"
                    } else {
                        "Copied"
                    }
                ),
            );
        }
        FileAction::Paste => paste_items(state),
        FileAction::Duplicate => {
            let selected = selected_files(state);
            if selected.is_empty() {
                set_status(state, "Select one or more files to duplicate.");
                return;
            }
            let state = state.clone();
            glib::MainContext::default().spawn_local(async move {
                let count = selected.len();
                let mut success = 0;
                for item in selected {
                    if let Ok(copy_file) = operations::duplicate(&item.file).await {
                        success += 1;
                        let mut s = state.borrow_mut();
                        s.redo_stack.clear();
                        s.undo_stack.push(operations::UndoAction::Duplicate { copy_file });
                    }
                }
                render(&state);
                set_status(&state, &format!("Duplicated {success} of {count} item(s)"));
            });
        }
        FileAction::ExtractHere => {
            let selected = selected_files(state);
            let target_folder = state.borrow().current.clone();
            let state = state.clone();
            glib::MainContext::default().spawn_local(async move {
                for item in selected {
                    set_status(&state, "Extracting archive…");
                    match operations::extract_archive(&item.file, &target_folder) {
                        Ok(()) => {
                            render(&state);
                            set_status(&state, "Extracted archive successfully.");
                        }
                        Err(err) => {
                            set_status(&state, &format!("Extraction failed: {err}"));
                        }
                    }
                }
            });
        }
        FileAction::ExtractTo => {
            let selected = selected_files(state);
            let current_dir = state.borrow().current.clone();
            let state = state.clone();
            glib::MainContext::default().spawn_local(async move {
                for item in selected {
                    let basename = item
                        .file
                        .basename()
                        .map(|b| b.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "extracted".to_string());
                    let stem = if let Some(stripped) = basename
                        .strip_suffix(".tar.gz")
                        .or_else(|| basename.strip_suffix(".tar.xz"))
                        .or_else(|| basename.strip_suffix(".tar.bz2"))
                        .or_else(|| basename.strip_suffix(".tar.zst"))
                        .or_else(|| basename.strip_suffix(".tgz"))
                        .or_else(|| basename.strip_suffix(".txz"))
                    {
                        stripped
                    } else if let Some(dot) = basename.rfind('.') {
                        &basename[..dot]
                    } else {
                        &basename
                    };
                    let dest_folder = current_dir.child(stem);
                    let _ = dest_folder.make_directory_future(glib::Priority::DEFAULT).await;
                    set_status(&state, "Extracting archive…");
                    match operations::extract_archive(&item.file, &dest_folder) {
                        Ok(()) => {
                            render(&state);
                            set_status(&state, &format!("Extracted to {stem}/"));
                        }
                        Err(err) => {
                            set_status(&state, &format!("Extraction failed: {err}"));
                        }
                    }
                }
            });
        }
        FileAction::CompressZip | FileAction::CompressTarGz => {
            let selected = selected_files(state);
            if selected.is_empty() {
                set_status(state, "Select files or folders to compress.");
                return;
            }
            let format = if matches!(action, FileAction::CompressZip) {
                "zip"
            } else {
                "tar.gz"
            };
            let parent_dir = state.borrow().current.clone();
            let sources: Vec<gio::File> = selected.into_iter().map(|item| item.file).collect();
            let state = state.clone();
            set_status(&state, "Creating archive…");
            glib::MainContext::default().spawn_local(async move {
                match operations::compress_to_archive(&sources, &parent_dir, format) {
                    Ok(arch) => {
                        render(&state);
                        let name = arch
                            .basename()
                            .map(|b| b.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        set_status(&state, &format!("Created archive: {name}"));
                    }
                    Err(err) => {
                        set_status(&state, &format!("Failed to create archive: {err}"));
                    }
                }
            });
        }
        FileAction::SelectAll => {
            if let Some(rows) = &state.borrow().rows {
                rows.select_all();
            }
        }
        FileAction::SelectByPattern => select_by_pattern_dialog(parent, state),
        FileAction::InvertSelection => invert_selection(state),
        FileAction::CopyLocation => {
            let locations = selected_files(state)
                .iter()
                .map(|item| {
                    item.file
                        .path()
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_else(|| item.file.uri().to_string())
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !locations.is_empty() {
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&locations);
                    set_status(state, "Copied location to clipboard.");
                }
            }
        }
        FileAction::OpenTerminalHere => {
            let selected = selected_files(state);
            let target = if let Some(item) = selected.first() {
                if item.is_dir {
                    item.file.clone()
                } else {
                    state.borrow().current.clone()
                }
            } else {
                state.borrow().current.clone()
            };
            if let Some(path) = target.path() {
                let status = state.borrow().status_label.clone();
                terminal::open_terminal(&path, status.as_ref());
            } else {
                set_status(state, "A local shell cannot start in a remote location.");
            }
        }
        FileAction::Rename => rename_selected(parent, state),
        FileAction::Trash => trash_selected(parent, state, false),
        FileAction::Delete => trash_selected(parent, state, true),
        FileAction::EmptyTrash => {
            let dialog = gtk::MessageDialog::builder()
                .transient_for(parent)
                .modal(true)
                .message_type(gtk::MessageType::Question)
                .buttons(gtk::ButtonsType::OkCancel)
                .text("Empty Trash?")
                .secondary_text("All items in the Trash will be permanently deleted.")
                .build();
            let state = state.clone();
            dialog.connect_response(move |dialog, response| {
                if response == gtk::ResponseType::Ok {
                    let state = state.clone();
                    glib::MainContext::default().spawn_local(async move {
                        if let Err(err) = operations::empty_trash().await {
                            set_status(&state, &format!("Could not empty trash: {err}"));
                        } else {
                            render(&state);
                            set_status(&state, "Trash emptied.");
                        }
                    });
                }
                dialog.close();
            });
            dialog.present();
        }
        FileAction::RestoreFromTrash => {
            let selected = selected_files(state);
            let state = state.clone();
            glib::MainContext::default().spawn_local(async move {
                let mut count = 0;
                for item in selected {
                    if operations::restore_from_trash(&item.file).await.is_ok() {
                        count += 1;
                    }
                }
                render(&state);
                set_status(&state, &format!("Restored {count} item(s) from Trash"));
            });
        }
        FileAction::Undo => {
            let action_opt = state.borrow_mut().undo_stack.pop();
            if let Some(act) = action_opt {
                let state = state.clone();
                glib::MainContext::default().spawn_local(async move {
                    match operations::undo_action(&act).await {
                        Ok(msg) => {
                            state.borrow_mut().redo_stack.push(act);
                            render(&state);
                            set_status(&state, &msg);
                        }
                        Err(err) => {
                            set_status(&state, &format!("Undo failed: {err}"));
                        }
                    }
                });
            } else {
                set_status(state, "Nothing to undo.");
            }
        }
        FileAction::Redo => {
            let action_opt = state.borrow_mut().redo_stack.pop();
            if let Some(act) = action_opt {
                let state = state.clone();
                glib::MainContext::default().spawn_local(async move {
                    match operations::redo_action(&act).await {
                        Ok(msg) => {
                            state.borrow_mut().undo_stack.push(act);
                            render(&state);
                            set_status(&state, &msg);
                        }
                        Err(err) => {
                            set_status(&state, &format!("Redo failed: {err}"));
                        }
                    }
                });
            } else {
                set_status(state, "Nothing to redo.");
            }
        }
        FileAction::Properties => show_properties(parent, state),
    }
}

fn invert_selection(state: &Rc<RefCell<AppState>>) {
    let rows = state.borrow().rows.clone();
    let Some(rows) = rows else { return };
    let mut child = rows.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(row) = widget.downcast::<ListBoxRow>() {
            if row.is_selectable() {
                if row.is_selected() {
                    rows.unselect_row(&row);
                } else {
                    rows.select_row(Some(&row));
                }
            }
        }
    }
}

fn select_by_pattern_dialog(parent: &ApplicationWindow, state: &Rc<RefCell<AppState>>) {
    let dialog = gtk::Dialog::builder()
        .title("Select items by pattern")
        .transient_for(parent)
        .modal(true)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Select", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Accept);
    let form = GtkBox::new(Orientation::Vertical, 8);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.set_margin_start(18);
    form.set_margin_end(18);
    let prompt = Label::new(Some("Use * for any characters and ? for one character."));
    prompt.set_xalign(0.0);
    let pattern = Entry::new();
    pattern.set_placeholder_text(Some("For example: *.rs"));
    pattern.set_activates_default(true);
    form.append(&prompt);
    form.append(&pattern);
    dialog.set_default_widget(Some(&pattern));
    dialog.content_area().append(&form);
    let pattern_entry = pattern.clone();
    let state = state.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let pattern = pattern_entry.text().trim().to_string();
            if pattern.is_empty() {
                set_status(&state, "Enter a file name pattern.");
            } else if let Some(rows) = { state.borrow().rows.clone() } {
                rows.unselect_all();
                let mut selected = 0usize;
                let mut child = rows.first_child();
                while let Some(widget) = child {
                    child = widget.next_sibling();
                    if let Ok(row) = widget.downcast::<ListBoxRow>() {
                        if row_path(&row)
                            .is_some_and(|item| glob_matches(&pattern, &item.info.display_name()))
                        {
                            rows.select_row(Some(&row));
                            selected += 1;
                        }
                    }
                }
                set_status(&state, &format!("Selected {selected} item(s)"));
            }
        }
        dialog.close();
    });
    dialog.present();
    pattern.grab_focus();
}

fn glob_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let name: Vec<char> = name.to_lowercase().chars().collect();
    let mut matched = vec![false; name.len() + 1];
    matched[0] = true;
    for token in pattern {
        let mut next = vec![false; name.len() + 1];
        match token {
            '*' => {
                next[0] = matched[0];
                for index in 1..=name.len() {
                    next[index] = matched[index] || next[index - 1];
                }
            }
            '?' => {
                for index in 1..=name.len() {
                    next[index] = matched[index - 1];
                }
            }
            character => {
                for index in 1..=name.len() {
                    next[index] = matched[index - 1] && name[index - 1] == character;
                }
            }
        }
        matched = next;
    }
    matched[name.len()]
}

fn create_file_dialog(parent: &ApplicationWindow, state: &Rc<RefCell<AppState>>) {
    let dialog = gtk::Dialog::builder()
        .title("Create empty file")
        .transient_for(parent)
        .modal(true)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Create", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Accept);
    let form = GtkBox::new(Orientation::Vertical, 8);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.set_margin_start(18);
    form.set_margin_end(18);
    let name = Entry::new();
    name.set_placeholder_text(Some("File name"));
    name.set_activates_default(true);
    dialog.set_default_widget(Some(&name));
    form.append(&name);
    dialog.content_area().append(&form);
    let name_entry = name.clone();
    let state = state.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let file_name = name_entry.text().trim().to_string();
            if file_name.is_empty()
                || file_name == "."
                || file_name == ".."
                || file_name.contains('/')
                || file_name.contains('\0')
            {
                set_status(&state, "Enter a valid file name.");
            } else {
                let file = state.borrow().current.child(&file_name);
                let state = state.clone();
                glib::MainContext::default().spawn_local(async move {
                    match file
                        .create_future(gio::FileCreateFlags::NONE, glib::Priority::DEFAULT)
                        .await
                    {
                        Ok(stream) => match stream.close_future(glib::Priority::DEFAULT).await {
                            Ok(()) => {
                                {
                                    let mut s = state.borrow_mut();
                                    s.redo_stack.clear();
                                    s.undo_stack.push(operations::UndoAction::Create { file: file.clone() });
                                }
                                render(&state);
                                set_status(&state, &format!("Created {file_name}"));
                            }
                            Err(error) => {
                                set_status(&state, &format!("Could not close file: {error}"))
                            }
                        },
                        Err(error) => {
                            set_status(&state, &format!("Could not create file: {error}"))
                        }
                    }
                });
            }
        }
        dialog.close();
    });
    dialog.present();
    name.grab_focus();
}

fn paste_items(state: &Rc<RefCell<AppState>>) {
    let system_clipboard_is_local = gdk::Display::default()
        .map(|display| display.clipboard().is_local())
        .unwrap_or(false);
    let (sources, is_cut, destination) = {
        let s = state.borrow();
        (s.clipboard.clone(), s.clipboard_is_cut, s.current.clone())
    };
    if sources.is_empty() || !system_clipboard_is_local {
        paste_external_clipboard(state);
        return;
    }
    transfer_items(state, sources, is_cut, destination);
}

fn paste_external_clipboard(state: &Rc<RefCell<AppState>>) {
    let Some(display) = gdk::Display::default() else {
        set_status(state, "The system clipboard is unavailable.");
        return;
    };
    let clipboard = display.clipboard();
    let state = state.clone();
    glib::MainContext::default().spawn_local(async move {
        match clipboard
            .read_future(
                &["x-special/gnome-copied-files", "text/uri-list"],
                glib::Priority::DEFAULT,
            )
            .await
        {
            Ok((stream, mime_type)) => {
                let mut bytes = Vec::new();
                loop {
                    match stream
                        .read_bytes_future(16 * 1024, glib::Priority::DEFAULT)
                        .await
                    {
                        Ok(chunk) if chunk.is_empty() => break,
                        Ok(chunk) => bytes.extend_from_slice(chunk.as_ref()),
                        Err(error) => {
                            set_status(&state, &format!("Could not read clipboard: {error}"));
                            return;
                        }
                    }
                }
                let content = String::from_utf8_lossy(&bytes);
                let special_format = mime_type.as_str() == "x-special/gnome-copied-files";
                let mut lines = content.lines();
                let operation = if special_format {
                    lines.next().unwrap_or("copy")
                } else {
                    "copy"
                };
                let sources: Vec<_> = lines
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(gio::File::for_uri)
                    .collect();
                if sources.is_empty() {
                    set_status(&state, "The system clipboard does not contain files.");
                    return;
                }
                let destination = state.borrow().current.clone();
                transfer_items(&state, sources, operation == "cut", destination);
            }
            Err(error) => set_status(&state, &format!("Could not read clipboard: {error}")),
        }
    });
}

fn transfer_items(
    state: &Rc<RefCell<AppState>>,
    sources: Vec<gio::File>,
    is_cut: bool,
    destination: gio::File,
) {
    let state = state.clone();
    glib::MainContext::default().spawn_local(async move {
        set_status(&state, "Transferring files…");
        let mut transferred = 0usize;
        for source in &sources {
            let name = source
                .basename()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "item".to_string());
            let target = destination.child(&name);
            if target.equal(source) || target.has_prefix(source) {
                set_status(&state, &format!("Cannot paste {name} into itself."));
                continue;
            }
            let result = if is_cut {
                operations::move_file(source, &target).await
            } else {
                operations::copy(source, &target).await
            };
            match result {
                Ok(()) => {
                    transferred += 1;
                    let action = if is_cut {
                        operations::UndoAction::Move {
                            source: source.clone(),
                            destination: target.clone(),
                        }
                    } else {
                        operations::UndoAction::Copy {
                            destination: target.clone(),
                        }
                    };
                    let mut s = state.borrow_mut();
                    s.redo_stack.clear();
                    s.undo_stack.push(action);
                }
                Err(error) => set_status(&state, &format!("Could not transfer {name}: {error}")),
            }
        }
        if is_cut && transferred == sources.len() {
            let mut s = state.borrow_mut();
            s.clipboard.clear();
            s.clipboard_is_cut = false;
            if let Some(display) = gdk::Display::default() {
                display.clipboard().set_text("");
            }
        }
        render(&state);
        set_status(
            &state,
            &format!("Transferred {transferred} of {} item(s)", sources.len()),
        );
    });
}

fn publish_clipboard(files: &[gio::File], is_cut: bool) {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let uris: Vec<_> = files.iter().map(|file| file.uri().to_string()).collect();
    let uri_list = format!("{}\r\n", uris.join("\r\n"));
    let copied_files = format!(
        "{}\n{}\n",
        if is_cut { "cut" } else { "copy" },
        uris.join("\n")
    );
    let uri_provider = gdk::ContentProvider::for_bytes(
        "text/uri-list",
        &glib::Bytes::from_owned(uri_list.into_bytes()),
    );
    let gnome_provider = gdk::ContentProvider::for_bytes(
        "x-special/gnome-copied-files",
        &glib::Bytes::from_owned(copied_files.into_bytes()),
    );
    let providers = [uri_provider, gnome_provider];
    let provider = gdk::ContentProvider::new_union(&providers);
    if let Err(error) = display.clipboard().set_content(Some(&provider)) {
        eprintln!("Cairn: could not publish file clipboard: {error}");
    }
}

fn rename_selected(parent: &ApplicationWindow, state: &Rc<RefCell<AppState>>) {
    let mut selected = selected_files(state);
    if selected.len() != 1 {
        set_status(state, "Select exactly one item to rename.");
        return;
    }
    let item = selected.remove(0);
    let dialog = gtk::Dialog::builder()
        .title("Rename")
        .transient_for(parent)
        .modal(true)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Rename", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Accept);
    let form = GtkBox::new(Orientation::Vertical, 8);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.set_margin_start(18);
    form.set_margin_end(18);
    let name = Entry::new();
    name.set_text(&item.info.display_name());
    name.set_activates_default(true);
    dialog.set_default_widget(Some(&name));
    form.append(&name);
    dialog.content_area().append(&form);
    let name_entry = name.clone();
    let state = state.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let new_name = name_entry.text().trim().to_string();
            let file = item.file.clone();
            let state = state.clone();
            glib::MainContext::default().spawn_local(async move {
                match operations::rename(&file, &new_name).await {
                    Ok(new_file) => {
                        {
                            let mut s = state.borrow_mut();
                            s.redo_stack.clear();
                            s.undo_stack.push(operations::UndoAction::Rename {
                                old_file: file.clone(),
                                new_file,
                            });
                        }
                        render(&state);
                        set_status(&state, &format!("Renamed to {new_name}"));
                    }
                    Err(error) => set_status(&state, &format!("Could not rename item: {error}")),
                }
            });
        }
        dialog.close();
    });
    dialog.present();
    name.grab_focus();
    name.select_region(0, -1);
}

fn trash_selected(parent: &ApplicationWindow, state: &Rc<RefCell<AppState>>, permanent: bool) {
    let selected = selected_files(state);
    if selected.is_empty() {
        set_status(state, "Select one or more items first.");
        return;
    }
    let confirm = permanent || state.borrow().preferences.confirm_trash;
    if !confirm {
        remove_items(state, selected, permanent);
        return;
    }
    let count = selected.len();
    let message = if permanent {
        format!("Permanently delete {count} selected item(s)? This cannot be undone.")
    } else {
        format!("Move {count} selected item(s) to Trash?")
    };
    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .message_type(gtk::MessageType::Warning)
        .buttons(gtk::ButtonsType::Cancel)
        .text(&message)
        .build();
    dialog.add_button(
        if permanent { "Delete" } else { "Move to Trash" },
        gtk::ResponseType::Accept,
    );
    let state = state.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            remove_items(&state, selected.clone(), permanent);
        }
        dialog.close();
    });
    dialog.present();
}

fn remove_items(state: &Rc<RefCell<AppState>>, items: Vec<FileRowData>, permanent: bool) {
    let state = state.clone();
    glib::MainContext::default().spawn_local(async move {
        let mut removed = 0usize;
        for item in &items {
            let original_path = (!permanent).then(|| item.file.path()).flatten();
            let result = if permanent {
                operations::delete(&item.file).await
            } else {
                operations::trash(&item.file).await
            };
            match result {
                Ok(()) => {
                    removed += 1;
                    if let Some(original_path) = original_path {
                        if let Some(trash_file) = operations::find_in_trash(&original_path).await {
                            let mut state = state.borrow_mut();
                            state.redo_stack.clear();
                            state.undo_stack.push(operations::UndoAction::Trash {
                                original_path,
                                trash_file,
                            });
                        }
                    }
                }
                Err(error) => set_status(&state, &format!("Could not remove item: {error}")),
            }
        }
        render(&state);
        set_status(
            &state,
            &format!("Removed {removed} of {} item(s)", items.len()),
        );
    });
}

fn show_properties(parent: &ApplicationWindow, state: &Rc<RefCell<AppState>>) {
    let selected = selected_files(state);
    let (item, is_dir) = if selected.is_empty() {
        let current_file = state.borrow().current.clone();
        match current_file.query_info(
            "standard::*,unix::*,time::*",
            gio::FileQueryInfoFlags::NONE,
            None::<&gio::Cancellable>,
        ) {
            Ok(info) => (
                FileRowData {
                    file: current_file,
                    is_dir: true,
                    info,
                },
                true,
            ),
            Err(e) => {
                set_status(state, &format!("Cannot query properties: {e}"));
                return;
            }
        }
    } else if selected.len() == 1 {
        let item = selected[0].clone();
        let is_dir = item.is_dir;
        (item, is_dir)
    } else {
        set_status(state, "Select one item to view its properties.");
        return;
    };

    let dialog = gtk::Dialog::builder()
        .title("Properties")
        .transient_for(parent)
        .modal(true)
        .build();
    dialog.add_button("Close", gtk::ResponseType::Cancel);
    let apply_permissions = dialog.add_button("Apply permissions", gtk::ResponseType::Accept);
    let panel = GtkBox::new(Orientation::Vertical, 12);
    panel.set_margin_top(18);
    panel.set_margin_bottom(18);
    panel.set_margin_start(18);
    panel.set_margin_end(18);

    let display_name = item.info.display_name().to_string();
    let title = Label::new(Some(&display_name));
    title.set_xalign(0.0);
    title.add_css_class("brand-title");

    let kind_str = filesystem::file_type_description(&item.info, is_dir, &display_name);
    let loc_str = filesystem::display_location(&item.file);
    let mod_str = item
        .info
        .modification_date_time()
        .and_then(|dt| dt.format("%Y-%m-%d %H:%M:%S").ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let mut details_text = format!("Type: {kind_str}\nLocation: {loc_str}\nModified: {mod_str}");
    if item.info.is_symlink() {
        if let Some(target) = item.info.symlink_target() {
            details_text.push_str(&format!("\nTarget: {}", target.display()));
        }
    }

    let details = Label::new(Some(&details_text));
    details.set_xalign(0.0);
    details.set_selectable(true);
    details.add_css_class("dim-label");

    let size_label = if is_dir {
        let lbl = Label::new(Some("Size: Calculating…"));
        lbl.set_xalign(0.0);
        let lbl_clone = lbl.clone();
        let file_clone = item.file.clone();
        glib::MainContext::default().spawn_local(async move {
            match operations::calculate_dir_size(&file_clone).await {
                Ok((bytes, items)) => {
                    lbl_clone.set_text(&format!(
                        "Size: {} ({} bytes, {} items)",
                        filesystem::format_size(bytes),
                        bytes,
                        items
                    ));
                }
                Err(_) => {
                    lbl_clone.set_text("Size: Unable to calculate");
                }
            }
        });
        lbl
    } else {
        let bytes = item.info.size().max(0) as u64;
        let lbl = Label::new(Some(&format!(
            "Size: {} ({} bytes)",
            filesystem::format_size(bytes),
            bytes
        )));
        lbl.set_xalign(0.0);
        lbl
    };

    panel.append(&title);
    panel.append(&details);
    panel.append(&size_label);
    let permissions_title = Label::new(Some("UNIX PERMISSIONS"));
    permissions_title.set_xalign(0.0);
    permissions_title.add_css_class("section-label");
    panel.append(&permissions_title);
    let grid = Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(12);
    for (column, label) in ["Read", "Write", "Execute"].iter().enumerate() {
        grid.attach(&Label::new(Some(label)), column as i32 + 1, 0, 1, 1);
    }
    let original_mode = item.info.attribute_uint32("unix::mode");
    apply_permissions.set_sensitive(item.info.has_attribute("unix::mode"));
    let permission_bits = [
        0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
    ];
    let permission_rows = ["Owner", "Group", "Other"];
    let mut checks = Vec::new();
    for (row, label) in permission_rows.iter().enumerate() {
        let heading = Label::new(Some(label));
        heading.set_xalign(0.0);
        grid.attach(&heading, 0, row as i32 + 1, 1, 1);
        for column in 0..3 {
            let check = CheckButton::new();
            check.set_halign(gtk::Align::Center);
            check.set_active(original_mode & permission_bits[row * 3 + column] != 0);
            check.set_sensitive(original_mode != 0);
            grid.attach(&check, column as i32 + 1, row as i32 + 1, 1, 1);
            checks.push(check);
        }
    }
    panel.append(&grid);
    let recursive = CheckButton::with_label("Apply to contained files and folders");
    recursive.set_visible(is_dir);
    panel.append(&recursive);
    dialog.content_area().append(&panel);
    let state = state.clone();
    let file = item.file.clone();
    let recursive_mode = recursive.clone();
    let file_type_bits = original_mode & !0o7777;
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let mut new_mode = file_type_bits;
            for (check, bit) in checks.iter().zip(permission_bits) {
                if check.is_active() {
                    new_mode |= bit;
                }
            }
            match file.set_attribute_uint32(
                "unix::mode",
                new_mode,
                gio::FileQueryInfoFlags::NONE,
                None::<&gio::Cancellable>,
            ) {
                Ok(()) if recursive_mode.is_active() => {
                    let state = state.clone();
                    let file = file.clone();
                    glib::MainContext::default().spawn_local(async move {
                        match operations::set_mode_recursive(&file, new_mode).await {
                            Ok(()) => {
                                render(&state);
                                set_status(&state, "Permissions updated for folder contents.");
                            }
                            Err(error) => set_status(&state, &format!("Could not update folder contents: {error}")),
                        }
                    });
                }
                Ok(()) => {
                    render(&state);
                    set_status(&state, "Permissions updated.");
                }
                Err(error) => set_status(&state, &format!("Could not update permissions: {error}")),
            }
        }
        dialog.close();
    });
    dialog.present();
}

fn save_preferences(state: &Rc<RefCell<AppState>>) {
    if let Err(error) = state.borrow().preferences.save() {
        eprintln!("Cairn: could not save preferences: {error}");
    }
}

fn tool_button(icon: &str, tooltip: &str) -> Button {
    let b = Button::from_icon_name(icon);
    b.set_tooltip_text(Some(tooltip));
    b.add_css_class("flat");
    b
}

fn tab_header(name: &str) -> (GtkBox, Label, Button) {
    let header = GtkBox::new(Orientation::Horizontal, 8);
    let title = Label::new(Some(name));
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(20);
    let close = Button::from_icon_name("window-close-symbolic");
    close.add_css_class("flat");
    close.add_css_class("tab-close");
    close.set_tooltip_text(Some("Close tab"));
    header.append(&title);
    header.append(&close);
    (header, title, close)
}

fn update_status_bar(state: &Rc<RefCell<AppState>>) {
    let s = state.borrow();
    let Some(rows) = &s.rows else { return };
    let Some(status) = &s.status_label else { return };
    let selected = rows.selected_rows();
    if selected.is_empty() {
        let mut total = 0;
        let mut child = rows.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(row) = widget.downcast::<ListBoxRow>() {
                if row.is_selectable() {
                    total += 1;
                }
            }
        }
        let folder = s.current.clone();
        let status = status.clone();
        let query_empty = s.query.is_empty();
        drop(s);

        glib::MainContext::default().spawn_local(async move {
            let free_space = if folder.is_native() {
                folder
                    .query_filesystem_info_future(
                        "filesystem::free",
                        glib::Priority::DEFAULT,
                    )
                    .await
                    .ok()
                    .and_then(|info| {
                        let free = info.attribute_uint64("filesystem::free");
                        if free > 0 {
                            Some(format!("  ·  {} available", filesystem::format_size(free)))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let prefix = if query_empty {
                format!("{total} items")
            } else {
                format!("{total} matching items")
            };
            status.set_text(&format!("{prefix}{free_space}"));
        });
    } else {
        let mut total_size = 0u64;
        let mut file_count = 0;
        let mut dir_count = 0;
        for row in selected {
            if let Some(item) = row_path(&row) {
                if item.is_dir {
                    dir_count += 1;
                } else {
                    file_count += 1;
                    total_size += item.info.size().max(0) as u64;
                }
            }
        }
        let msg = match (dir_count, file_count) {
            (1, 0) => "1 folder selected".to_string(),
            (d, 0) => format!("{d} folders selected"),
            (0, 1) => format!("1 item selected ({})", filesystem::format_size(total_size)),
            (0, f) => format!("{f} items selected ({})", filesystem::format_size(total_size)),
            (d, f) => format!("{d} folders, {f} items selected ({})", filesystem::format_size(total_size)),
        };
        status.set_text(&msg);
    }
}

fn connect_file_rows(rows: &ListBox, state: &Rc<RefCell<AppState>>) {
    let state_activate = state.clone();
    rows.connect_row_activated(move |_, row| {
        if let Some(item) = row_path(row) {
            if item.is_dir {
                navigate_to(&state_activate, item.file, true);
            } else if let Err(error) = filesystem::open_file(&item.file) {
                set_status(&state_activate, &format!("Could not open file: {error}"));
            }
        }
    });
    let state_select = state.clone();
    rows.connect_selected_rows_changed(move |_| {
        update_status_bar(&state_select);
    });
}

fn connect_background_context_menu(
    rows: &ListBox,
    state: &Rc<RefCell<AppState>>,
) {
    let popover = Popover::new();
    popover.set_parent(rows);
    popover.set_has_arrow(false);
    let menu = GtkBox::new(Orientation::Vertical, 2);
    menu.set_margin_top(5);
    menu.set_margin_bottom(5);
    menu.set_margin_start(5);
    menu.set_margin_end(5);

    let popover_weak = popover.downgrade();
    let gesture = GestureClick::new();
    gesture.set_button(3);
    let rows_clone = rows.clone();
    let st = state.clone();
    let menu_clone = menu.clone();

    gesture.connect_pressed(move |_, _, x, y| {
        let hit_selectable_row = rows_clone
            .row_at_y(y as i32)
            .map(|r| r.is_selectable())
            .unwrap_or(false);

        if !hit_selectable_row {
            rows_clone.unselect_all();
            if let Some(popover) = popover_weak.upgrade() {
                while let Some(child) = menu_clone.first_child() {
                    menu_clone.remove(&child);
                }

                let is_trash = st.borrow().current.uri() == "trash:///";
                let has_clipboard = !st.borrow().clipboard.is_empty();

                let mut items: Vec<(&str, &str, FileAction)> = Vec::new();
                if is_trash {
                    items.push(("Empty Trash…", "user-trash-symbolic", FileAction::EmptyTrash));
                    items.push(("Properties", "dialog-information-symbolic", FileAction::Properties));
                } else {
                    items.push(("New Folder…", "folder-new-symbolic", FileAction::NewFolder));
                    items.push(("New Empty File…", "document-new-symbolic", FileAction::NewFile));
                    if has_clipboard {
                        items.push(("Paste", "edit-paste-symbolic", FileAction::Paste));
                    }
                    items.push(("Select All", "edit-select-all-symbolic", FileAction::SelectAll));
                    items.push(("Open in Terminal", "utilities-terminal-symbolic", FileAction::OpenTerminalHere));
                    items.push(("Properties", "dialog-information-symbolic", FileAction::Properties));
                }

                for (label, icon, act) in items {
                    let btn = menu_action_button(icon, label);
                    btn.add_css_class("flat");
                    let p_weak = popover_weak.clone();
                    let st_c = st.clone();
                    btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            p.popdown();
                        }
                        if let Some(parent) = st_c.borrow().mount_parent.clone() {
                            perform_file_action(act, &parent, &st_c);
                        }
                    });
                    menu_clone.append(&btn);
                }

                popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            }
        }
    });

    popover.set_child(Some(&menu));
    rows.add_controller(gesture);
}

fn connect_row_context_menu(row: &ListBoxRow, state: &Rc<RefCell<AppState>>) {
    let popover = Popover::new();
    popover.set_parent(row);
    popover.set_has_arrow(true);
    let menu = GtkBox::new(Orientation::Vertical, 2);
    menu.set_margin_top(5);
    menu.set_margin_bottom(5);
    menu.set_margin_start(5);
    menu.set_margin_end(5);

    let item_opt = row_path(row);
    let is_dir = item_opt.as_ref().map(|item| item.is_dir).unwrap_or(false);
    let name = item_opt
        .as_ref()
        .map(|item| item.info.display_name().to_string())
        .unwrap_or_default();
    let is_trash = state.borrow().current.uri() == "trash:///";

    let mut actions = Vec::new();
    if is_trash {
        actions.push(("Restore from Trash", "edit-undo-symbolic", FileAction::RestoreFromTrash));
        actions.push(("Delete Permanently…", "edit-delete-symbolic", FileAction::Delete));
        actions.push(("Properties", "dialog-information-symbolic", FileAction::Properties));
    } else {
        actions.push(("Open", "document-open-symbolic", FileAction::Open));
        if is_dir {
            actions.push(("Open in New Tab", "tab-new-symbolic", FileAction::OpenInNewTab));
            actions.push(("Open in Terminal Here", "utilities-terminal-symbolic", FileAction::OpenTerminalHere));
        } else {
            actions.push(("Open with Text Editor", "accessories-text-editor-symbolic", FileAction::OpenWithEditor));
        }
        if filesystem::is_archive(&name) {
            actions.push(("Extract Here", "package-x-generic-symbolic", FileAction::ExtractHere));
            actions.push(("Extract to Folder", "folder-new-symbolic", FileAction::ExtractTo));
        }
        actions.push(("Copy", "edit-copy-symbolic", FileAction::Copy));
        actions.push(("Copy Location", "edit-copy-symbolic", FileAction::CopyLocation));
        actions.push(("Cut", "edit-cut-symbolic", FileAction::Cut));
        actions.push(("Duplicate", "edit-copy-symbolic", FileAction::Duplicate));
        actions.push(("Compress to .zip", "package-x-generic-symbolic", FileAction::CompressZip));
        actions.push(("Compress to .tar.gz", "package-x-generic-symbolic", FileAction::CompressTarGz));
        actions.push(("Rename…", "document-edit-symbolic", FileAction::Rename));
        actions.push(("Move to Trash", "user-trash-symbolic", FileAction::Trash));
        actions.push(("Delete Permanently…", "edit-delete-symbolic", FileAction::Delete));
        actions.push(("Properties", "dialog-information-symbolic", FileAction::Properties));
    }

    let popover_weak = popover.downgrade();
    for (label, icon, action) in actions {
        let button = menu_action_button(icon, label);
        button.add_css_class("flat");
        let state = state.clone();
        let popover = popover_weak.clone();
        let row = row.clone();
        button.connect_clicked(move |_| {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
            if matches!(
                action,
                FileAction::Rename
                    | FileAction::Properties
                    | FileAction::OpenInNewTab
                    | FileAction::OpenTerminalHere
                    | FileAction::ExtractHere
                    | FileAction::ExtractTo
            ) {
                if let Some(rows) = state.borrow().rows.clone() {
                    if !row.is_selected() {
                        rows.unselect_all();
                        rows.select_row(Some(&row));
                    }
                }
            }
            let parent = { state.borrow().mount_parent.clone() };
            if let Some(parent) = parent {
                perform_file_action(action, &parent, &state);
            }
        });
        menu.append(&button);
    }
    popover.set_child(Some(&menu));

    let gesture = GestureClick::new();
    gesture.set_button(3);
    let row_ref = row.clone();
    let popover_ref = popover.downgrade();
    gesture.connect_pressed(move |_, _, x, y| {
        if let Some(rows) = row_ref
            .parent()
            .and_then(|parent| parent.downcast::<ListBox>().ok())
        {
            if !row_ref.is_selected() {
                rows.unselect_all();
                rows.select_row(Some(&row_ref));
            }
        }
        if let Some(popover) = popover_ref.upgrade() {
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
        }
    });
    row.add_controller(gesture);
}

fn sync_active_tab(state: &mut AppState) {
    if let Some(tab) = state.tabs.get_mut(state.active_tab) {
        tab.current = state.current.clone();
        tab.back = state.back.clone();
        tab.forward = state.forward.clone();
        tab.query = state.query.clone();
        if let Some(rows) = &state.rows {
            tab.rows = rows.clone();
        }
    }
}

fn switch_tab(state: &Rc<RefCell<AppState>>, _notebook: &Notebook, index: usize) {
    let query = {
        let mut s = state.borrow_mut();
        if index >= s.tabs.len() {
            return;
        }
        if index != s.active_tab {
            sync_active_tab(&mut s);
        }
        s.active_tab = index;
        let (current, back, forward, query, rows) = {
            let tab = &s.tabs[index];
            (
                tab.current.clone(),
                tab.back.clone(),
                tab.forward.clone(),
                tab.query.clone(),
                tab.rows.clone(),
            )
        };
        s.current = current;
        s.back = back;
        s.forward = forward;
        s.query = query;
        s.rows = Some(rows);
        s.query.clone()
    };
    if let Some(search) = &state.borrow().search_entry {
        search.set_text(&query);
    }
    render(state);
}

fn connect_to_server_dialog(parent: &ApplicationWindow, state: &Rc<RefCell<AppState>>) {
    let dialog = gtk::Dialog::builder()
        .title("Connect to Server")
        .transient_for(parent)
        .modal(true)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Connect", gtk::ResponseType::Accept);
    let form = GtkBox::new(Orientation::Vertical, 8);
    form.set_margin_top(16);
    form.set_margin_bottom(16);
    form.set_margin_start(16);
    form.set_margin_end(16);
    let help = Label::new(Some("Enter a server address such as smb://server/share, sftp://host/path, ftp://host/path, or dav://host/path."));
    help.set_wrap(true);
    help.set_xalign(0.0);
    let address = Entry::new();
    address.set_placeholder_text(Some("smb://server/share"));
    address.set_hexpand(true);
    form.append(&help);
    form.append(&address);
    dialog.content_area().append(&form);
    let address_input = address.clone();
    let state = state.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let uri = address_input.text().trim().to_string();
            let scheme = uri.split_once("://").map(|(scheme, _)| scheme.to_ascii_lowercase());
            match scheme.as_deref() {
                Some("smb" | "ftp" | "ftps" | "sftp" | "dav" | "davs") => {
                    navigate_to(&state, gio::File::for_uri(&uri), true);
                }
                _ => set_status(&state, "Enter a server URI using smb, ftp, ftps, sftp, dav, or davs."),
            }
        }
        dialog.close();
    });
    dialog.present();
    address.grab_focus();
}

fn new_tab(parent: &ApplicationWindow, state: &Rc<RefCell<AppState>>) {
    let (notebook, location, query, single_click) = {
        let s = state.borrow();
        let Some(notebook) = s.notebook.clone() else {
            return;
        };
        (
            notebook,
            s.current.clone(),
            s.query.clone(),
            s.preferences.open_on_single_click,
        )
    };
    let rows = ListBox::new();
    rows.set_selection_mode(gtk::SelectionMode::Multiple);
    rows.add_css_class("file-list");
    rows.set_activate_on_single_click(single_click);
    rows.set_vexpand(true);
    let scroll = ScrolledWindow::new();
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_child(Some(&rows));
    let label = location
        .basename()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Home".to_string());
    let (header, title, close) = tab_header(&label);
    let page = notebook.append_page(&scroll, Some(&header));
    state.borrow_mut().tabs.push(TabState {
        current: location,
        back: Vec::new(),
        forward: Vec::new(),
        query,
        rows: rows.clone(),
        title,
    });
    connect_file_rows(&rows, state);
    connect_background_context_menu(&rows, state);
    let state_close = state.clone();
    let notebook_close = notebook.clone();
    let scroll_close = scroll.clone();
    close.connect_clicked(move |_| close_tab(&state_close, &notebook_close, &scroll_close));
    notebook.set_current_page(Some(page));
    parent.present();
}

fn close_tab(state: &Rc<RefCell<AppState>>, notebook: &Notebook, page: &impl IsA<gtk::Widget>) {
    if notebook.n_pages() <= 1 {
        set_status(state, "Cairn keeps at least one tab open.");
        return;
    }
    let Some(page_index) = notebook.page_num(page) else {
        return;
    };
    let index = page_index as usize;
    {
        let mut s = state.borrow_mut();
        if index == s.active_tab {
            sync_active_tab(&mut s);
            s.active_tab = if index > 0 { index - 1 } else { 0 };
        } else if index < s.active_tab {
            s.active_tab -= 1;
        }
        s.tabs.remove(index);
    }
    notebook.remove_page(Some(page_index));
    let current = notebook.current_page();
    if let Some(current) = current {
        switch_tab(state, notebook, current as usize);
    }
}

fn create_folder_dialog(parent: &ApplicationWindow, state: &Rc<RefCell<AppState>>) {
    let dialog = gtk::Dialog::builder()
        .title("Create folder")
        .transient_for(parent)
        .modal(true)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Create folder", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Accept);

    let form = GtkBox::new(Orientation::Vertical, 8);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.set_margin_start(18);
    form.set_margin_end(18);
    let prompt = Label::new(Some("Choose a name for the new folder."));
    prompt.set_xalign(0.0);
    let name = Entry::new();
    name.set_placeholder_text(Some("Folder name"));
    name.set_activates_default(true);
    dialog.set_default_widget(Some(&name));
    form.append(&prompt);
    form.append(&name);
    dialog.content_area().append(&form);

    let name_entry = name.clone();
    let state = state.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let folder_name = name_entry.text().trim().to_string();
            if folder_name.is_empty()
                || folder_name == "."
                || folder_name == ".."
                || folder_name.contains('/')
                || folder_name.contains('\0')
            {
                if let Some(status) = &state.borrow().status_label {
                    status.set_text("Enter a valid folder name.");
                }
            } else {
                let folder = state.borrow().current.child(&folder_name);
                let state = state.clone();
                glib::MainContext::default().spawn_local(async move {
                    match folder.make_directory_future(glib::Priority::DEFAULT).await {
                        Ok(()) => {
                            render(&state);
                            set_status(
                                &state,
                                &format!("Created {}", filesystem::display_location(&folder)),
                            );
                        }
                        Err(error) => {
                            set_status(&state, &format!("Could not create folder: {error}"))
                        }
                    }
                });
            }
        }
        dialog.close();
    });
    dialog.present();
    name.grab_focus();
}

fn place_row(icon: &str, name: &str) -> Button {
    let row = Button::new();
    row.set_child(Some(&icon_label(icon, name)));
    row.set_hexpand(true);
    row.set_halign(gtk::Align::Fill);
    row.add_css_class("flat");
    row.add_css_class("place-button");
    row
}

fn menu_action_button(icon: &str, label: &str) -> Button {
    let button = Button::new();
    let content = GtkBox::new(Orientation::Horizontal, 8);
    content.set_margin_top(5);
    content.set_margin_bottom(5);
    content.set_margin_start(8);
    content.set_margin_end(8);
    let image = Image::from_icon_name(icon);
    image.set_pixel_size(16);
    image.set_halign(gtk::Align::Start);
    let text = Label::new(Some(label));
    text.set_xalign(0.0);
    content.append(&image);
    content.append(&text);
    button.set_child(Some(&content));
    button.set_halign(gtk::Align::Fill);
    button.set_hexpand(true);
    button
}

fn connect_place_context_menu(
    row: &Button,
    location: gio::File,
    state: &Rc<RefCell<AppState>>,
) {
    let popover = Popover::new();
    popover.set_parent(row);
    let menu = GtkBox::new(Orientation::Vertical, 2);
    menu.set_margin_top(5);
    menu.set_margin_bottom(5);
    menu.set_margin_start(5);
    menu.set_margin_end(5);
    let open = menu_action_button("tab-new-symbolic", "Open in New Tab");
    open.add_css_class("flat");
    let target = location.clone();
    let state_open = state.clone();
    let popover_weak = popover.downgrade();
    open.connect_clicked(move |_| {
        if let Some(popover) = popover_weak.upgrade() {
            popover.popdown();
        }
        if let Some(parent) = state_open.borrow().mount_parent.clone() {
            new_tab(&parent, &state_open);
            navigate_to(&state_open, target.clone(), false);
        }
    });
    menu.append(&open);
    popover.set_child(Some(&menu));

    let context = GestureClick::new();
    context.set_button(3);
    let popover_weak = popover.downgrade();
    context.connect_pressed(move |_, _, x, y| {
        if let Some(popover) = popover_weak.upgrade() {
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
        }
    });
    row.add_controller(context);

    let middle = GestureClick::new();
    middle.set_button(2);
    let target = location;
    let state = state.clone();
    middle.connect_pressed(move |_, _, _, _| {
        if let Some(parent) = state.borrow().mount_parent.clone() {
            new_tab(&parent, &state);
            navigate_to(&state, target.clone(), false);
        }
    });
    row.add_controller(middle);
}

fn connect_trash_place_menu(
    row: &Button,
    parent: &ApplicationWindow,
    state: &Rc<RefCell<AppState>>,
) {
    let popover = Popover::new();
    popover.set_parent(row);
    popover.set_has_arrow(true);
    let actions = GtkBox::new(Orientation::Vertical, 2);
    actions.set_margin_top(5);
    actions.set_margin_bottom(5);
    actions.set_margin_start(5);
    actions.set_margin_end(5);
    let open = menu_action_button("tab-new-symbolic", "Open in New Tab");
    open.add_css_class("flat");
    let state_open = state.clone();
    let popover_open = popover.downgrade();
    open.connect_clicked(move |_| {
        if let Some(popover) = popover_open.upgrade() {
            popover.popdown();
        }
        if let Some(parent) = state_open.borrow().mount_parent.clone() {
            new_tab(&parent, &state_open);
            navigate_to(&state_open, gio::File::for_uri("trash:///"), false);
        }
    });
    actions.append(&open);
    let empty = menu_action_button("user-trash-symbolic", "Empty Trash…");
    empty.add_css_class("flat");
    let popover_weak = popover.downgrade();
    let state_empty = state.clone();
    let parent = parent.clone();
    empty.connect_clicked(move |_| {
        if let Some(popover) = popover_weak.upgrade() {
            popover.popdown();
        }
        perform_file_action(FileAction::EmptyTrash, &parent, &state_empty);
    });
    actions.append(&empty);
    popover.set_child(Some(&actions));
    let gesture = GestureClick::new();
    gesture.set_button(3);
    let popover_weak = popover.downgrade();
    gesture.connect_pressed(move |_, _, x, y| {
        if let Some(popover) = popover_weak.upgrade() {
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
        }
    });
    row.add_controller(gesture);

    let middle = GestureClick::new();
    middle.set_button(2);
    let state_middle = state.clone();
    middle.connect_pressed(move |_, _, _, _| {
        if let Some(parent) = state_middle.borrow().mount_parent.clone() {
            new_tab(&parent, &state_middle);
            navigate_to(&state_middle, gio::File::for_uri("trash:///"), false);
        }
    });
    row.add_controller(middle);
}

fn refresh_devices(state: &Rc<RefCell<AppState>>, parent: &ApplicationWindow) {
    let (Some(devices), Some(monitor)) = ({
        let state = state.borrow();
        (state.devices.clone(), state.volume_monitor.clone())
    }) else {
        return;
    };
    while let Some(child) = devices.first_child() {
        devices.remove(&child);
    }

    let volumes = monitor.volumes();
    for volume in &volumes {
        let name = volume.name().to_string();
        let mount = volume.get_mount();
        if mount.is_none() && !volume.can_mount() {
            continue;
        }
        let row = GtkBox::new(Orientation::Horizontal, 2);
        let open = place_row("drive-removable-media-symbolic", &name);
        if let Some(mount) = mount.as_ref() {
            let root = mount.root();
            if root.query_file_type(gio::FileQueryInfoFlags::NONE, None::<&gio::Cancellable>)
                == gio::FileType::Directory
            {
                let state = state.clone();
                open.connect_clicked(move |_| navigate_to(&state, root.clone(), true));
            } else {
                open.set_tooltip_text(Some("Open this mounted volume"));
                let state = state.clone();
                open.connect_clicked(move |_| navigate_to(&state, root.clone(), true));
            }
        } else if volume.can_mount() {
            let volume = volume.clone();
            let volume_name = name.clone();
            let state = state.clone();
            let parent = parent.clone();
            open.connect_clicked(move |_| {
                let operation = gtk::MountOperation::new(Some(&parent));
                let state = state.clone();
                let parent = parent.clone();
                let request_volume = volume.clone();
                let mounted_volume = volume.clone();
                let name = volume_name.clone();
                request_volume.mount(
                    gio::MountMountFlags::NONE,
                    Some(&operation),
                    None::<&gio::Cancellable>,
                    move |result| match result {
                        Ok(()) => {
                            refresh_devices(&state, &parent);
                            if let Some(mounted) = mounted_volume.get_mount() {
                                navigate_to(&state, mounted.root(), true);
                            }
                        }
                        Err(error) => {
                            set_status(&state, &format!("Could not mount {name}: {error}"))
                        }
                    },
                );
            });
        }
        row.append(&open);

        if let Some(mount) = mount {
            if mount.can_eject() || mount.can_unmount() {
                let eject = tool_button("media-eject-symbolic", "Safely remove");
                let volume_name = name.clone();
                let state = state.clone();
                let parent = parent.clone();
                eject.connect_clicked(move |_| {
                    let operation = gtk::MountOperation::new(Some(&parent));
                    let state = state.clone();
                    let parent = parent.clone();
                    let name = volume_name.clone();
                    let done = move |result| {
                        match result {
                            Ok(()) => set_status(&state, &format!("Removed {name}")),
                            Err(error) => {
                                set_status(&state, &format!("Could not remove {name}: {error}"))
                            }
                        }
                        refresh_devices(&state, &parent);
                    };
                    if mount.can_eject() {
                        mount.eject_with_operation(
                            gio::MountUnmountFlags::NONE,
                            Some(&operation),
                            None::<&gio::Cancellable>,
                            done,
                        );
                    } else {
                        mount.unmount_with_operation(
                            gio::MountUnmountFlags::NONE,
                            Some(&operation),
                            None::<&gio::Cancellable>,
                            done,
                        );
                    }
                });
                row.append(&eject);
            }
        }
        devices.append(&row);
    }

    let other_mounts: Vec<_> = monitor
        .mounts()
        .into_iter()
        .filter(|mount| mount.volume().is_none())
        .collect();
    if !other_mounts.is_empty() {
        let mounted_title = Label::new(Some("MOUNTED LOCATIONS"));
        mounted_title.set_xalign(0.0);
        mounted_title.add_css_class("section-label");
        mounted_title.set_margin_top(12);
        devices.append(&mounted_title);
    }
    for mount in other_mounts {
        let name = mount.name().to_string();
        let row = GtkBox::new(Orientation::Horizontal, 2);
        let open = place_row("drive-harddisk-symbolic", &name);
        let root = mount.root();
        if root.query_file_type(gio::FileQueryInfoFlags::NONE, None::<&gio::Cancellable>)
            == gio::FileType::Directory
        {
            let state = state.clone();
            open.connect_clicked(move |_| navigate_to(&state, root.clone(), true));
        } else {
            open.set_tooltip_text(Some("Open this mounted location"));
            let state = state.clone();
            open.connect_clicked(move |_| navigate_to(&state, root.clone(), true));
        }
        row.append(&open);
        if mount.can_eject() || mount.can_unmount() {
            let eject = tool_button("media-eject-symbolic", "Unmount location");
            let state = state.clone();
            let parent = parent.clone();
            eject.connect_clicked(move |_| {
                let operation = gtk::MountOperation::new(Some(&parent));
                let state = state.clone();
                let parent = parent.clone();
                let mount = mount.clone();
                let name = name.clone();
                let done = move |result| {
                    match result {
                        Ok(()) => set_status(&state, &format!("Unmounted {name}")),
                        Err(error) => {
                            set_status(&state, &format!("Could not unmount {name}: {error}"))
                        }
                    }
                    refresh_devices(&state, &parent);
                };
                if mount.can_eject() {
                    mount.eject_with_operation(
                        gio::MountUnmountFlags::NONE,
                        Some(&operation),
                        None::<&gio::Cancellable>,
                        done,
                    );
                } else {
                    mount.unmount_with_operation(
                        gio::MountUnmountFlags::NONE,
                        Some(&operation),
                        None::<&gio::Cancellable>,
                        done,
                    );
                }
            });
            row.append(&eject);
        }
        devices.append(&row);
    }

    if volumes.is_empty() && monitor.mounts().is_empty() {
        let empty = Label::new(Some("No removable devices"));
        empty.set_xalign(0.0);
        empty.set_margin_start(10);
        empty.add_css_class("dim-label");
        devices.append(&empty);
    }
}

fn toggle_bookmark(state: &Rc<RefCell<AppState>>) {
    let uri = state.borrow().current.uri().to_string();
    let added = {
        let mut s = state.borrow_mut();
        if let Some(index) = s
            .preferences
            .bookmarks
            .iter()
            .position(|entry| entry == &uri)
        {
            s.preferences.bookmarks.remove(index);
            false
        } else {
            s.preferences.bookmarks.push(uri);
            true
        }
    };
    save_preferences(state);
    refresh_bookmarks(state);
    set_status(
        state,
        if added {
            "Location bookmarked."
        } else {
            "Bookmark removed."
        },
    );
}

fn refresh_bookmarks(state: &Rc<RefCell<AppState>>) {
    let (Some(bookmarks), entries) = ({
        let s = state.borrow();
        (s.bookmarks.clone(), s.preferences.bookmarks.clone())
    }) else {
        return;
    };
    while let Some(child) = bookmarks.first_child() {
        bookmarks.remove(&child);
    }
    for uri in entries {
        let file = gio::File::for_uri(&uri);
        let name = file
            .basename()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| filesystem::display_location(&file));
        let row = GtkBox::new(Orientation::Horizontal, 2);
        let open = place_row("user-bookmarks-symbolic", &name);
        connect_place_context_menu(&open, file.clone(), state);
        let state_open = state.clone();
        let file_to_open = file.clone();
        open.connect_clicked(move |_| navigate_to(&state_open, file_to_open.clone(), true));
        row.append(&open);
        let remove = tool_button("window-close-symbolic", "Remove bookmark");
        let state = state.clone();
        remove.connect_clicked(move |_| {
            state
                .borrow_mut()
                .preferences
                .bookmarks
                .retain(|bookmark| bookmark != &uri);
            save_preferences(&state);
            refresh_bookmarks(&state);
        });
        row.append(&remove);
        bookmarks.append(&row);
    }
}

fn set_status(state: &Rc<RefCell<AppState>>, message: &str) {
    if let Some(status) = &state.borrow().status_label {
        status.set_text(message);
    }
}

fn icon_label(icon: &str, name: &str) -> GtkBox {
    let line = GtkBox::new(Orientation::Horizontal, 10);
    line.set_margin_top(7);
    line.set_margin_bottom(7);
    line.set_margin_start(8);
    line.set_margin_end(8);
    let image = Image::from_icon_name(icon);
    image.set_pixel_size(18);
    let label = Label::new(Some(name));
    label.set_xalign(0.0);
    line.append(&image);
    line.append(&label);
    line
}

fn navigate_to(state: &Rc<RefCell<AppState>>, file: gio::File, remember: bool) {
    {
        let mut s = state.borrow_mut();
        if file.equal(&s.current) {
            return;
        }
        if remember {
            let old = s.current.clone();
            s.back.push(old);
            s.forward.clear();
        }
        s.current = file;
        sync_active_tab(&mut s);
    }
    render(state);
}

fn go_back(state: &Rc<RefCell<AppState>>) {
    let target = {
        let mut s = state.borrow_mut();
        let target = s.back.pop();
        if let Some(target) = &target {
            let current = s.current.clone();
            s.forward.push(current);
            s.current = target.clone();
        }
        sync_active_tab(&mut s);
        target
    };
    if target.is_some() {
        render(state);
    }
}

fn go_forward(state: &Rc<RefCell<AppState>>) {
    let target = {
        let mut s = state.borrow_mut();
        let target = s.forward.pop();
        if let Some(target) = &target {
            let current = s.current.clone();
            s.back.push(current);
            s.current = target.clone();
        }
        sync_active_tab(&mut s);
        target
    };
    if target.is_some() {
        render(state);
    }
}

fn row_path(row: &ListBoxRow) -> Option<FileRowData> {
    unsafe {
        row.data::<FileRowData>("cairn-file")
            .map(|p| p.as_ref().clone())
    }
}

fn render(state: &Rc<RefCell<AppState>>) {
    let (
        folder,
        query,
        preferences,
        rows,
        location,
        status,
        back_button,
        forward_button,
        bookmark_button,
        tab_title,
        generation,
    ) = {
        let mut s = state.borrow_mut();
        s.load_generation = s.load_generation.wrapping_add(1);
        (
            s.current.clone(),
            s.query.to_lowercase(),
            s.preferences.clone(),
            s.rows.clone(),
            s.path_entry.clone(),
            s.status_label.clone(),
            s.back_button.clone(),
            s.forward_button.clone(),
            s.bookmark_button.clone(),
            s.tabs.get(s.active_tab).map(|tab| tab.title.clone()),
            s.load_generation,
        )
    };
    let Some(rows) = rows else {
        return;
    };
    let mut child = rows.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(row) = widget.downcast::<ListBoxRow>() {
            rows.remove(&row);
        }
    }
    if let Some(entry) = location {
        entry.set_text(&filesystem::display_location(&folder));
    }
    if let Some(title) = tab_title {
        let name = folder
            .basename()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| filesystem::display_location(&folder));
        title.set_text(&name);
    }
    if let Some(button) = back_button {
        button.set_sensitive(!state.borrow().back.is_empty());
    }
    if let Some(button) = forward_button {
        button.set_sensitive(!state.borrow().forward.is_empty());
    }
    if let Some(button) = bookmark_button {
        let is_bookmarked = preferences.bookmarks.contains(&folder.uri().to_string());
        button.set_icon_name(if is_bookmarked {
            "bookmark-symbolic"
        } else {
            "bookmark-new-symbolic"
        });
        button.set_tooltip_text(Some(if is_bookmarked {
            "Remove bookmark"
        } else {
            "Bookmark this location"
        }));
    }

    if let Some(status) = &status {
        status.set_text("Loading…");
    }
    let state = state.clone();
    glib::MainContext::default().spawn_local(async move {
        const ATTRIBUTES: &str = "standard::name,standard::display-name,standard::type,standard::size,standard::is-symlink,standard::is-hidden,standard::symlink-target,unix::mode,time::modified";
        if !folder.is_native() {
            let parent = state.borrow().mount_parent.clone();
            let operation = gtk::MountOperation::new(parent.as_ref());
            if let Err(error) = folder
                .mount_enclosing_volume_future(
                    gio::MountMountFlags::NONE,
                    Some(&operation),
                )
                .await
            {
                if !error.matches(gio::IOErrorEnum::AlreadyMounted)
                    && !error.matches(gio::IOErrorEnum::NotSupported)
                {
                    if state.borrow().load_generation == generation {
                        show_listing_error(&rows, status.as_ref(), &error.to_string());
                    }
                    return;
                }
            }
        }

        let enumerator = match folder
            .enumerate_children_future(
                ATTRIBUTES,
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT,
            )
            .await
        {
            Ok(enumerator) => enumerator,
            Err(error) => {
                if state.borrow().load_generation == generation {
                    show_listing_error(&rows, status.as_ref(), &error.to_string());
                }
                return;
            }
        };

        let mut entries = Vec::new();
        loop {
            match enumerator
                .next_files_future(128, glib::Priority::DEFAULT)
                .await
            {
                Ok(batch) if batch.is_empty() => break,
                Ok(batch) => entries.extend(batch),
                Err(error) => {
                    if state.borrow().load_generation == generation {
                        show_listing_error(&rows, status.as_ref(), &error.to_string());
                    }
                    return;
                }
            }
        }
        if state.borrow().load_generation != generation {
            return;
        }
        entries.sort_by(|a, b| {
            let a_dir = a.file_type() == gio::FileType::Directory;
            let b_dir = b.file_type() == gio::FileType::Directory;
            let directory_order = if preferences.directories_first {
                b_dir.cmp(&a_dir)
            } else {
                std::cmp::Ordering::Equal
            };
            directory_order.then_with(|| {
                let ordering = match preferences.sort_by {
                    settings::SortBy::Name => a
                        .display_name()
                        .to_lowercase()
                        .cmp(&b.display_name().to_lowercase()),
                    settings::SortBy::Size => a.size().cmp(&b.size()),
                    settings::SortBy::Modified => a
                        .attribute_uint64("time::modified")
                        .cmp(&b.attribute_uint64("time::modified")),
                    settings::SortBy::Type => {
                        let ext_a = a
                            .name()
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        let ext_b = b
                            .name()
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        ext_a.cmp(&ext_b).then_with(|| {
                            a.display_name()
                                .to_lowercase()
                                .cmp(&b.display_name().to_lowercase())
                        })
                    }
                };
                if preferences.sort_reversed {
                    ordering.reverse()
                } else {
                    ordering
                }
            })
        });
        if !preferences.show_hidden {
            entries.retain(|info| !info.is_hidden());
        }
        let total = entries.len();
        let mut shown = 0;
        for info in entries {
            let name = info.display_name().to_string();
            if !query.is_empty() && !name.to_lowercase().contains(&query) {
                continue;
            }
            let file = enumerator.child(&info);
            let is_dir = info.file_type() == gio::FileType::Directory;
            let icon = if is_dir {
                "folder-symbolic"
            } else {
                filesystem::icon_for_file(&name)
            };
            let row = ListBoxRow::new();
            row.add_css_class("file-row");
            if preferences.compact_view {
                row.add_css_class("compact-row");
            }
            let line = GtkBox::new(Orientation::Horizontal, 12);
            let vertical_margin = if preferences.compact_view { 3 } else { 8 };
            line.set_margin_top(vertical_margin);
            line.set_margin_bottom(vertical_margin);
            line.set_margin_start(14);
            line.set_margin_end(14);
            let image: gtk::Widget = if preferences.show_thumbnails
                && !is_dir
                && filesystem::is_image(&name)
            {
                let picture = gtk::Picture::for_file(&file);
                picture.set_can_shrink(true);
                picture.set_keep_aspect_ratio(true);
                let thumbnail_size = if preferences.compact_view { 30 } else { 44 };
                picture.set_size_request(thumbnail_size, thumbnail_size);
                picture.add_css_class("file-thumbnail");
                picture.upcast()
            } else {
                let image = Image::from_icon_name(icon);
                image.set_pixel_size(if preferences.compact_view { 18 } else { 22 });
                image.upcast()
            };
            let text = GtkBox::new(Orientation::Vertical, 2);
            text.set_hexpand(true);
            let title = Label::new(Some(&name));
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title.add_css_class("file-name");
            let detail = Label::new(Some(&filesystem::entry_details(&info, &file, is_dir)));
            detail.set_xalign(0.0);
            detail.add_css_class("dim-label");
            detail.add_css_class("file-detail");
            text.append(&title);
            text.append(&detail);
            line.append(&image);
            line.append(&text);
            if info.is_symlink() {
                let badge = Label::new(Some("LINK"));
                badge.add_css_class("badge");
                line.append(&badge);
            }
            row.set_child(Some(&line));
            let is_cut = {
                let s = state.borrow();
                s.clipboard_is_cut && s.clipboard.iter().any(|cut| cut.equal(&file))
            };
            if is_cut {
                row.add_css_class("file-cut");
            }
            unsafe {
                row.set_data(
                    "cairn-file",
                    FileRowData {
                        file,
                        is_dir,
                        info,
                    },
                );
            }
            row.set_tooltip_text(Some("Double click to open"));
            connect_row_context_menu(&row, &state);
            rows.append(&row);
            shown += 1;
        }
        if shown == 0 {
            let row = ListBoxRow::new();
            row.set_selectable(false);
            row.set_activatable(false);
            let message = if total == 0 {
                "This folder is empty"
            } else {
                "No files match your search"
            };
            let label = Label::new(Some(message));
            label.set_margin_top(32);
            label.add_css_class("dim-label");
            row.set_child(Some(&label));
            rows.append(&row);
        }
        if let Some(status) = status {
            let shown_text = if query.is_empty() {
                format!("{total} items")
            } else {
                format!("{shown} of {total} items")
            };
            status.set_text(&shown_text);
        }
        update_status_bar(&state);
    });
}

fn show_listing_error(rows: &ListBox, status: Option<&Label>, message: &str) {
    let mut child = rows.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(row) = widget.downcast::<ListBoxRow>() {
            rows.remove(&row);
        }
    }
    let row = ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    let label = Label::new(Some(&format!("Could not open this location: {message}")));
    label.set_margin_top(24);
    label.set_margin_bottom(24);
    label.add_css_class("dim-label");
    row.set_child(Some(&label));
    rows.append(&row);
    if let Some(status) = status {
        status.set_text("Location unavailable");
    }
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data("\
        .app-shell { background-color: alpha(@theme_bg_color, 0.94); border: 1px solid alpha(@theme_fg_color, 0.13); border-radius: 22px; }\
        .workspace-pane { border-radius: 20px; }\
        .brand-title { font-size: 1.08em; font-weight: 700; letter-spacing: 0.01em; }\
        .sidebar-panel { background-color: alpha(@theme_bg_color, 0.76); border-right: 1px solid alpha(@theme_fg_color, 0.12); border-top-left-radius: 20px; border-bottom-left-radius: 20px; }\
        .section-label { font-size: 0.72em; font-weight: 750; letter-spacing: 0.10em; opacity: 0.68; padding: 5px 10px; }\
        .place-button { border-radius: 12px; min-height: 36px; }\
        .place-button:hover { background: alpha(@theme_selected_bg_color, 0.12); }\
        .address-toolbar { padding: 7px 10px; margin: 10px 12px 6px; border-radius: 16px; background: alpha(@theme_fg_color, 0.035); border: 1px solid alpha(@theme_fg_color, 0.07); }\
        .location-entry { font-weight: 550; min-width: 140px; }\
        .file-list { padding: 12px 6px; background: transparent; }\
        .file-row { border-radius: 13px; border: 1px solid transparent; margin: 2px 8px; }\
        .file-row:hover { background: alpha(@theme_selected_bg_color, 0.09); border-color: alpha(@theme_fg_color, 0.08); }\
        .file-list row:selected { background: alpha(@theme_selected_bg_color, 0.20); border-color: alpha(@theme_selected_bg_color, 0.32); }\
        .file-thumbnail { border-radius: 9px; }\
        .file-name { font-weight: 550; }\
        .file-cut { opacity: 0.48; }\
        .file-detail { font-size: 0.83em; }\
        .statusbar { padding: 10px 18px; color: @theme_unfocused_fg_color; font-size: 0.88em; }\
        .badge { padding: 3px 8px; border-radius: 999px; background: alpha(@theme_selected_bg_color, 0.18); color: @theme_selected_fg_color; font-size: 0.72em; font-weight: 700; }\
    ");
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

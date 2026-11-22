use super::*;

pub(super) fn build_ui(app: &Application) {
    install_css();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let preferences = settings::Preferences::load();
    let window_width = preferences.window_width;
    let window_height = preferences.window_height;
    let sidebar_width = preferences.sidebar_width;
    let window_maximized = preferences.window_maximized;
    apply_prefer_dark_theme(preferences.prefer_dark);
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
    terminal.add_css_class("terminal-button");
    terminal.set_hexpand(false);
    terminal.set_halign(gtk::Align::Center);

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
    pane.set_resize_start_child(false);
    pane.set_resize_end_child(true);
    pane.set_position(sidebar_width);

    let sidebar = GtkBox::new(Orientation::Vertical, 8);
    sidebar.set_margin_top(14);
    sidebar.set_margin_bottom(14);
    sidebar.set_margin_start(10);
    sidebar.set_margin_end(10);
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
        (
            "Connect to Server…",
            "network-server-symbolic",
            FileAction::ConnectToServer,
        ),
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
        (
            "Empty Trash…",
            "user-trash-symbolic",
            FileAction::EmptyTrash,
        ),
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
    appearance.set_selected(if state.borrow().preferences.prefer_dark {
        1
    } else {
        0
    });
    let density_label = Label::new(Some("List density"));
    density_label.set_xalign(0.0);
    let density = DropDown::from_strings(&["Comfortable", "Compact"]);
    density.set_selected(if state.borrow().preferences.compact_view {
        1
    } else {
        0
    });
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
        apply_prefer_dark_theme(prefer_dark);
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
        state.preferences.sidebar_width = pane_close.position().clamp(168, 320);
        state.preferences.window_maximized = window.is_maximized();
        drop(state);
        save_preferences(&state_close);
        glib::Propagation::Proceed
    });
}

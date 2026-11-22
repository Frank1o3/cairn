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

mod actions;
mod listing;
mod window;
use actions::{FileAction, perform_file_action, save_preferences};
use listing::render;

const APP_ID: &str = "dev.cairn.FileManager";

#[allow(deprecated)]
fn apply_prefer_dark_theme(prefer_dark: bool) {
    let Some(settings) = gtk::Settings::default() else {
        return;
    };

    if let Some(property) = settings.find_property("gtk-interface-color-scheme") {
        if let Some(enum_class) = glib::EnumClass::with_type(property.value_type()) {
            let nick = if prefer_dark { "dark" } else { "default" };
            if let Some(value) = enum_class.value_by_nick(nick) {
                settings.set_property("gtk-interface-color-scheme", value.to_value(&enum_class));
                return;
            }
        }
    }

    // Older GTK versions do not expose gtk-interface-color-scheme yet.
    settings.set_gtk_application_prefer_dark_theme(prefer_dark);
}

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
    app.connect_activate(window::build_ui);
    app.run();
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
    let Some(status) = &s.status_label else {
        return;
    };
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
                    .query_filesystem_info_future("filesystem::free", glib::Priority::DEFAULT)
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
            (0, f) => format!(
                "{f} items selected ({})",
                filesystem::format_size(total_size)
            ),
            (d, f) => format!(
                "{d} folders, {f} items selected ({})",
                filesystem::format_size(total_size)
            ),
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

fn connect_background_context_menu(rows: &ListBox, state: &Rc<RefCell<AppState>>) {
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
                    items.push((
                        "Empty Trash…",
                        "user-trash-symbolic",
                        FileAction::EmptyTrash,
                    ));
                    items.push((
                        "Properties",
                        "dialog-information-symbolic",
                        FileAction::Properties,
                    ));
                } else {
                    items.push(("New Folder…", "folder-new-symbolic", FileAction::NewFolder));
                    items.push((
                        "New Empty File…",
                        "document-new-symbolic",
                        FileAction::NewFile,
                    ));
                    if has_clipboard {
                        items.push(("Paste", "edit-paste-symbolic", FileAction::Paste));
                    }
                    items.push((
                        "Select All",
                        "edit-select-all-symbolic",
                        FileAction::SelectAll,
                    ));
                    items.push((
                        "Open in Terminal",
                        "utilities-terminal-symbolic",
                        FileAction::OpenTerminalHere,
                    ));
                    items.push((
                        "Properties",
                        "dialog-information-symbolic",
                        FileAction::Properties,
                    ));
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
        actions.push((
            "Restore from Trash",
            "edit-undo-symbolic",
            FileAction::RestoreFromTrash,
        ));
        actions.push((
            "Delete Permanently…",
            "edit-delete-symbolic",
            FileAction::Delete,
        ));
        actions.push((
            "Properties",
            "dialog-information-symbolic",
            FileAction::Properties,
        ));
    } else {
        actions.push(("Open", "document-open-symbolic", FileAction::Open));
        if is_dir {
            actions.push((
                "Open in New Tab",
                "tab-new-symbolic",
                FileAction::OpenInNewTab,
            ));
            actions.push((
                "Open in Terminal Here",
                "utilities-terminal-symbolic",
                FileAction::OpenTerminalHere,
            ));
        } else {
            if item_opt
                .as_ref()
                .is_some_and(|item| filesystem::is_executable(&item.info))
            {
                actions.push((
                    "Run in Terminal",
                    "utilities-terminal-symbolic",
                    FileAction::RunInTerminal,
                ));
            }
            actions.push((
                "Open with Text Editor",
                "accessories-text-editor-symbolic",
                FileAction::OpenWithEditor,
            ));
        }
        if filesystem::is_archive(&name) {
            actions.push((
                "Extract Here",
                "package-x-generic-symbolic",
                FileAction::ExtractHere,
            ));
            actions.push((
                "Extract to Folder",
                "folder-new-symbolic",
                FileAction::ExtractTo,
            ));
        }
        actions.push(("Copy", "edit-copy-symbolic", FileAction::Copy));
        actions.push((
            "Copy Location",
            "edit-copy-symbolic",
            FileAction::CopyLocation,
        ));
        actions.push(("Cut", "edit-cut-symbolic", FileAction::Cut));
        actions.push(("Duplicate", "edit-copy-symbolic", FileAction::Duplicate));
        actions.push((
            "Compress to .zip",
            "package-x-generic-symbolic",
            FileAction::CompressZip,
        ));
        actions.push((
            "Compress to .tar.gz",
            "package-x-generic-symbolic",
            FileAction::CompressTarGz,
        ));
        actions.push(("Rename…", "document-edit-symbolic", FileAction::Rename));
        actions.push(("Move to Trash", "user-trash-symbolic", FileAction::Trash));
        actions.push((
            "Delete Permanently…",
            "edit-delete-symbolic",
            FileAction::Delete,
        ));
        actions.push((
            "Properties",
            "dialog-information-symbolic",
            FileAction::Properties,
        ));
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
                    | FileAction::RunInTerminal
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
    let help = Label::new(Some(
        "Enter a server address such as smb://server/share, sftp://host/path, ftp://host/path, or dav://host/path.",
    ));
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
            let scheme = uri
                .split_once("://")
                .map(|(scheme, _)| scheme.to_ascii_lowercase());
            match scheme.as_deref() {
                Some("smb" | "ftp" | "ftps" | "sftp" | "dav" | "davs") => {
                    navigate_to(&state, gio::File::for_uri(&uri), true);
                }
                _ => set_status(
                    &state,
                    "Enter a server URI using smb, ftp, ftps, sftp, dav, or davs.",
                ),
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
                            {
                                let mut state = state.borrow_mut();
                                state.redo_stack.clear();
                                state.undo_stack.push(operations::UndoAction::Create {
                                    file: folder.clone(),
                                    is_dir: true,
                                });
                            }
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

fn connect_place_context_menu(row: &Button, location: gio::File, state: &Rc<RefCell<AppState>>) {
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

fn install_css() {
    const EMBEDDED_STYLE: &str = include_str!("ui/style/style.css");
    const EMBEDDED_DEFINITIONS: &str = include_str!("ui/style/definitions.css");
    let config_dir = settings::Preferences::config_dir();
    let definitions_path = config_dir.join("definitions.css");
    let style_path = config_dir.join("style.css");
    let install_styles = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&config_dir)?;
        if !definitions_path.exists() {
            std::fs::write(&definitions_path, EMBEDDED_DEFINITIONS)?;
        }
        if !style_path.exists() {
            std::fs::write(&style_path, EMBEDDED_STYLE)?;
        } else {
            let mut style = std::fs::read_to_string(&style_path)?;
            let mut migrated = false;
            if !style.contains("@import url(\"definitions.css\")") {
                style = format!("@import url(\"definitions.css\");\n{style}");
                migrated = true;
            }
            // Earlier generated styles used translucent surfaces, which let
            // other windows bleed through. Migrate only these shipped default
            // values while preserving unrelated user stylesheet edits.
            let opaque_defaults = [
                (
                    "background-color: alpha(@cairn_bg_color, 0.94);",
                    "background-color: @cairn_bg_color;",
                ),
                (
                    "background-color: alpha(@cairn_bg_color, 0.76);",
                    "background-color: @cairn_bg_color;",
                ),
            ];
            for (old, new) in opaque_defaults {
                if style.contains(old) {
                    style = style.replace(old, new);
                    migrated = true;
                }
            }
            if migrated {
                std::fs::write(&style_path, style)?;
            }
        }
        Ok(())
    })();
    let can_load_user_css = match install_styles {
        Ok(()) => true,
        Err(error) => {
            eprintln!("Cairn: could not install default stylesheets: {error}");
            false
        }
    };

    let provider = gtk::CssProvider::new();
    provider.connect_parsing_error(|_, section, error| {
        let location = section.start_location();
        eprintln!(
            "Cairn: CSS error at {}:{}: {error}",
            location.lines() + 1,
            location.line_chars() + 1
        );
    });
    if can_load_user_css {
        provider.load_from_file(&gio::File::for_path(style_path));
    } else {
        let embedded_css =
            EMBEDDED_STYLE.replacen("@import url(\"definitions.css\");", EMBEDDED_DEFINITIONS, 1);
        provider.load_from_data(&embedded_css);
    }
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

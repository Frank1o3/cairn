use super::*;

#[derive(Clone, Copy)]
pub(super) enum FileAction {
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
    RunInTerminal,
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

pub(super) fn perform_file_action(
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
                        s.undo_stack.push(operations::UndoAction::Duplicate {
                            source: item.file.clone(),
                            copy_file,
                        });
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
                    let _ = dest_folder
                        .make_directory_future(glib::Priority::DEFAULT)
                        .await;
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
        FileAction::RunInTerminal => {
            let Some(item) = selected_files(state).into_iter().next() else {
                set_status(state, "Select an executable file first.");
                return;
            };
            let Some(path) = item.file.path() else {
                set_status(
                    state,
                    "Running remote files in a local terminal is not supported.",
                );
                return;
            };
            let Some(parent_dir) = path.parent() else {
                set_status(state, "Could not determine the executable's folder.");
                return;
            };
            let status = state.borrow().status_label.clone();
            terminal::open_executable_in_terminal(&path, parent_dir, status.as_ref());
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
                                    s.undo_stack.push(operations::UndoAction::Create {
                                        file: file.clone(),
                                        is_dir: false,
                                    });
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

pub(super) fn transfer_items(
    state: &Rc<RefCell<AppState>>,
    sources: Vec<gio::File>,
    is_cut: bool,
    destination: gio::File,
) {
    let state = state.clone();
    glib::MainContext::default().spawn_local(async move {
        set_status(&state, "Transferring files…");
        let mut transferred = 0usize;
        let mut remaining_cut_sources = Vec::new();
        for source in &sources {
            let name = source
                .basename()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "item".to_string());
            let target = destination.child(&name);
            if target.equal(source) || target.has_prefix(source) {
                set_status(&state, &format!("Cannot paste {name} into itself."));
                if is_cut {
                    remaining_cut_sources.push(source.clone());
                }
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
                            source: source.clone(),
                            destination: target.clone(),
                        }
                    };
                    let mut s = state.borrow_mut();
                    s.redo_stack.clear();
                    s.undo_stack.push(action);
                }
                Err(error) => {
                    set_status(&state, &format!("Could not transfer {name}: {error}"));
                    if is_cut {
                        remaining_cut_sources.push(source.clone());
                    }
                }
            }
        }
        if is_cut {
            let mut s = state.borrow_mut();
            s.clipboard = remaining_cut_sources.clone();
            s.clipboard_is_cut = !remaining_cut_sources.is_empty();
            drop(s);
            if remaining_cut_sources.is_empty() {
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text("");
                }
            } else {
                publish_clipboard(&remaining_cut_sources, true);
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
    if filesystem::is_symlink(&item.info) {
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
                            Err(error) => set_status(
                                &state,
                                &format!("Could not update folder contents: {error}"),
                            ),
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

pub(super) fn save_preferences(state: &Rc<RefCell<AppState>>) {
    if let Err(error) = state.borrow().preferences.save() {
        eprintln!("Cairn: could not save preferences: {error}");
    }
}

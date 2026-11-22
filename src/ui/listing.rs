use super::*;

pub(super) fn render(state: &Rc<RefCell<AppState>>) {
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
            let (image, thumbnail): (gtk::Widget, Option<(gtk::Picture, gio::File)>) = if preferences.show_thumbnails
                && !is_dir
                && filesystem::is_image(&name)
            {
                let picture = gtk::Picture::new();
                picture.set_can_shrink(true);
                picture.set_keep_aspect_ratio(true);
                let thumbnail_size = if preferences.compact_view { 30 } else { 44 };
                picture.set_size_request(thumbnail_size, thumbnail_size);
                picture.add_css_class("file-thumbnail");
                (picture.clone().upcast(), Some((picture, file.clone())))
            } else {
                let image = Image::from_icon_name(icon);
                image.set_pixel_size(if preferences.compact_view { 18 } else { 22 });
                (image.upcast(), None)
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
            if filesystem::is_symlink(&info) {
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
            if let Some((picture, image_file)) = thumbnail {
                glib::MainContext::default().spawn_local(async move {
                    let Ok(stream) = image_file
                        .read_future(glib::Priority::DEFAULT)
                        .await
                    else {
                        return;
                    };
                    let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale_future(
                        &stream, 128, 128, true,
                    )
                    .await
                    else {
                        return;
                    };
                    picture.set_pixbuf(Some(&pixbuf));
                });
            }
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

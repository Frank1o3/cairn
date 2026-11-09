use gtk::glib;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Clone, Debug)]
pub struct Preferences {
    pub show_hidden: bool,
    pub directories_first: bool,
    pub confirm_trash: bool,
    pub open_on_single_click: bool,
    pub show_thumbnails: bool,
    pub compact_view: bool,
    pub prefer_dark: bool,
    pub window_width: i32,
    pub window_height: i32,
    pub sidebar_width: i32,
    pub window_maximized: bool,
    pub sort_by: SortBy,
    pub sort_reversed: bool,
    pub bookmarks: Vec<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_hidden: true,
            directories_first: true,
            confirm_trash: true,
            open_on_single_click: false,
            show_thumbnails: true,
            compact_view: false,
            prefer_dark: false,
            window_width: 1280,
            window_height: 820,
            sidebar_width: 225,
            window_maximized: false,
            sort_by: SortBy::Name,
            sort_reversed: false,
            bookmarks: Vec::new(),
        }
    }
}

impl Preferences {
    fn path() -> PathBuf {
        glib::user_config_dir().join("cairn/settings.ini")
    }

    pub fn load() -> Self {
        let key_file = glib::KeyFile::new();
        if key_file
            .load_from_file(Self::path(), glib::KeyFileFlags::NONE)
            .is_err()
        {
            return Self::default();
        }
        let sort_by = match key_file.string("Files", "sort-by").as_deref() {
            Ok("size") => SortBy::Size,
            Ok("modified") => SortBy::Modified,
            Ok("type") => SortBy::Type,
            _ => SortBy::Name,
        };
        Self {
            show_hidden: key_file.boolean("Files", "show-hidden").unwrap_or(true),
            directories_first: key_file
                .boolean("Files", "directories-first")
                .unwrap_or(true),
            confirm_trash: key_file.boolean("Files", "confirm-trash").unwrap_or(true),
            open_on_single_click: key_file
                .boolean("Files", "single-click-open")
                .unwrap_or(false),
            show_thumbnails: key_file.boolean("Files", "show-thumbnails").unwrap_or(true),
            compact_view: key_file.boolean("Files", "compact-view").unwrap_or(false),
            prefer_dark: key_file.boolean("Appearance", "prefer-dark").unwrap_or(false),
            window_width: key_file.integer("Window", "width").unwrap_or(1280).clamp(820, 8192),
            window_height: key_file.integer("Window", "height").unwrap_or(820).clamp(560, 8192),
            sidebar_width: key_file.integer("Window", "sidebar-width").unwrap_or(225).clamp(168, 800),
            window_maximized: key_file.boolean("Window", "maximized").unwrap_or(false),
            sort_by,
            sort_reversed: key_file.boolean("Files", "sort-reversed").unwrap_or(false),
            bookmarks: key_file
                .string("Places", "bookmarks")
                .map(|value| {
                    value
                        .split('\n')
                        .filter(|entry| !entry.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub fn save(&self) -> Result<(), glib::Error> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| glib::Error::new(glib::FileError::Failed, &error.to_string()))?;
        }
        let key_file = glib::KeyFile::new();
        key_file.set_boolean("Files", "show-hidden", self.show_hidden);
        key_file.set_boolean("Files", "directories-first", self.directories_first);
        key_file.set_boolean("Files", "confirm-trash", self.confirm_trash);
        key_file.set_boolean("Files", "single-click-open", self.open_on_single_click);
        key_file.set_boolean("Files", "show-thumbnails", self.show_thumbnails);
        key_file.set_boolean("Files", "compact-view", self.compact_view);
        key_file.set_boolean("Appearance", "prefer-dark", self.prefer_dark);
        key_file.set_integer("Window", "width", self.window_width);
        key_file.set_integer("Window", "height", self.window_height);
        key_file.set_integer("Window", "sidebar-width", self.sidebar_width);
        key_file.set_boolean("Window", "maximized", self.window_maximized);
        key_file.set_boolean("Files", "sort-reversed", self.sort_reversed);
        key_file.set_string(
            "Files",
            "sort-by",
            match self.sort_by {
                SortBy::Name => "name",
                SortBy::Size => "size",
                SortBy::Modified => "modified",
                SortBy::Type => "type",
            },
        );
        key_file.set_string("Places", "bookmarks", &self.bookmarks.join("\n"));
        key_file.save_to_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preferences_defaults() {
        let prefs = Preferences::default();
        assert!(prefs.show_hidden);
        assert!(prefs.directories_first);
        assert!(prefs.confirm_trash);
        assert!(!prefs.open_on_single_click);
        assert!(prefs.show_thumbnails);
        assert_eq!(prefs.sort_by, SortBy::Name);
        assert!(!prefs.sort_reversed);
        assert!(prefs.bookmarks.is_empty());
    }
}

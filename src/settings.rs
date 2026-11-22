use gtk::glib;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
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
    pub fn config_dir() -> PathBuf {
        glib::user_config_dir().join("cairn")
    }

    fn path() -> PathBuf {
        Self::config_dir().join("settings.json")
    }

    fn legacy_path() -> PathBuf {
        Self::config_dir().join("settings.ini")
    }

    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<Self>(&contents) {
                Ok(preferences) => preferences.clamped(),
                Err(error) => {
                    eprintln!("Cairn: could not parse {}: {error}", path.display());
                    Self::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(preferences) = Self::load_legacy() {
                    preferences
                } else {
                    let preferences = Self::default();
                    if let Err(error) = preferences.save() {
                        eprintln!("Cairn: could not create default settings: {error}");
                    }
                    preferences
                }
            }
            Err(error) => {
                eprintln!("Cairn: could not read {}: {error}", path.display());
                Self::default()
            }
        }
    }

    fn load_legacy() -> Option<Self> {
        let key_file = glib::KeyFile::new();
        key_file
            .load_from_file(Self::legacy_path(), glib::KeyFileFlags::NONE)
            .ok()?;
        let sort_by = match key_file.string("Files", "sort-by").as_deref() {
            Ok("size") => SortBy::Size,
            Ok("modified") => SortBy::Modified,
            Ok("type") => SortBy::Type,
            _ => SortBy::Name,
        };
        let preferences = Self {
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
            prefer_dark: key_file
                .boolean("Appearance", "prefer-dark")
                .unwrap_or(false),
            window_width: key_file
                .integer("Window", "width")
                .unwrap_or(1280)
                .clamp(820, 8192),
            window_height: key_file
                .integer("Window", "height")
                .unwrap_or(820)
                .clamp(560, 8192),
            sidebar_width: key_file
                .integer("Window", "sidebar-width")
                .unwrap_or(225)
                .clamp(168, 320),
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
        };
        let _ = preferences.save();
        Some(preferences)
    }

    fn clamped(mut self) -> Self {
        self.window_width = self.window_width.clamp(820, 8192);
        self.window_height = self.window_height.clamp(560, 8192);
        self.sidebar_width = self.sidebar_width.clamp(168, 320);
        self
    }

    pub fn save(&self) -> Result<(), glib::Error> {
        let path = Self::path();
        std::fs::create_dir_all(Self::config_dir())
            .map_err(|error| glib::Error::new(glib::FileError::Failed, &error.to_string()))?;
        let contents = serde_json::to_vec_pretty(self)
            .map_err(|error| glib::Error::new(glib::FileError::Failed, &error.to_string()))?;
        let temporary_path = path.with_extension("json.tmp");
        fs::write(&temporary_path, contents)
            .and_then(|()| fs::rename(&temporary_path, &path))
            .map_err(|error| glib::Error::new(glib::FileError::Failed, &error.to_string()))
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

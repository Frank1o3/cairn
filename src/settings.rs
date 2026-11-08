use gtk::glib;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Name,
    Size,
    Modified,
}

#[derive(Clone, Debug)]
pub struct Preferences {
    pub show_hidden: bool,
    pub directories_first: bool,
    pub confirm_trash: bool,
    pub open_on_single_click: bool,
    pub sort_by: SortBy,
    pub bookmarks: Vec<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_hidden: true,
            directories_first: true,
            confirm_trash: true,
            open_on_single_click: false,
            sort_by: SortBy::Name,
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
            sort_by,
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
        key_file.set_string(
            "Files",
            "sort-by",
            match self.sort_by {
                SortBy::Name => "name",
                SortBy::Size => "size",
                SortBy::Modified => "modified",
            },
        );
        key_file.set_string("Places", "bookmarks", &self.bookmarks.join("\n"));
        key_file.save_to_file(path)
    }
}

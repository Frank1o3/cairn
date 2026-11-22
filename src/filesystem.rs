use gtk::gio;
use gtk::gio::prelude::*;

/// Some GIO backends omit optional attributes even when requested. Reading
/// `is_symlink()` without the attribute present triggers a GLib critical.
pub fn is_symlink(info: &gio::FileInfo) -> bool {
    info.file_type() == gio::FileType::SymbolicLink
        || (info.has_attribute("standard::is-symlink") && info.is_symlink())
}

pub fn is_executable(info: &gio::FileInfo) -> bool {
    info.has_attribute("unix::mode") && info.attribute_uint32("unix::mode") & 0o111 != 0
}
use gtk::glib;
use std::path::Path;

pub fn entry_details(info: &gio::FileInfo, file: &gio::File, is_dir: bool) -> String {
    let mut parts = Vec::new();
    if is_dir {
        parts.push("Folder".to_string());
    } else {
        parts.push(format_size(info.size().max(0) as u64));
    }
    if is_symlink(info) {
        parts.push("Symbolic link".to_string());
    }
    if is_dir {
        if let Some(path) = file.path() {
            if path.join(".git").exists() {
                parts.push("Git repository".to_string());
            }
        }
    }
    let mode = info.attribute_uint32("unix::mode");
    if mode != 0 {
        parts.push(format!(
            "{}{}{}{}{}{}{}{}{}",
            if mode & 0o400 != 0 { 'r' } else { '-' },
            if mode & 0o200 != 0 { 'w' } else { '-' },
            if mode & 0o100 != 0 { 'x' } else { '-' },
            if mode & 0o040 != 0 { 'r' } else { '-' },
            if mode & 0o020 != 0 { 'w' } else { '-' },
            if mode & 0o010 != 0 { 'x' } else { '-' },
            if mode & 0o004 != 0 { 'r' } else { '-' },
            if mode & 0o002 != 0 { 'w' } else { '-' },
            if mode & 0o001 != 0 { 'x' } else { '-' }
        ));
    }
    if let Some(dt) = info.modification_date_time() {
        if let Ok(formatted) = dt.format("%Y-%m-%d %H:%M") {
            parts.push(formatted.to_string());
        }
    }
    parts.join("  ·  ")
}

pub fn display_location(file: &gio::File) -> String {
    file.path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| file.uri().to_string())
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

pub fn is_archive(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".zip")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".txz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tar.zst")
        || lower.ends_with(".tar")
}

pub fn is_text_or_code(name: &str) -> bool {
    let lower = name.to_lowercase();
    [
        ".rs",
        ".c",
        ".h",
        ".cpp",
        ".hpp",
        ".cc",
        ".py",
        ".js",
        ".mjs",
        ".ts",
        ".jsx",
        ".tsx",
        ".go",
        ".java",
        ".sh",
        ".bash",
        ".zsh",
        ".toml",
        ".yaml",
        ".yml",
        ".json",
        ".xml",
        ".html",
        ".htm",
        ".css",
        ".scss",
        ".md",
        ".markdown",
        ".txt",
        ".conf",
        ".ini",
        ".log",
        ".desktop",
        ".env",
        ".diff",
        ".patch",
        ".sql",
        ".lua",
        ".vim",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
        || lower.starts_with('.')
}

pub fn is_image(name: &str) -> bool {
    let lower = name.to_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".avif", ".bmp", ".ico",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

pub fn file_type_description(info: &gio::FileInfo, is_dir: bool, name: &str) -> String {
    if is_dir {
        return "Folder".to_string();
    }
    if is_symlink(info) {
        return "Symbolic Link".to_string();
    }
    let lower = name.to_lowercase();
    if is_archive(name) {
        if lower.ends_with(".zip") {
            "ZIP Archive".to_string()
        } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            "Gzip-compressed Tar Archive".to_string()
        } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
            "XZ-compressed Tar Archive".to_string()
        } else {
            "Archive".to_string()
        }
    } else if let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "rs" => "Rust Source File".to_string(),
            "py" => "Python Script".to_string(),
            "js" => "JavaScript File".to_string(),
            "ts" => "TypeScript File".to_string(),
            "c" | "h" => "C Source File".to_string(),
            "cpp" | "hpp" => "C++ Source File".to_string(),
            "sh" | "bash" => "Shell Script".to_string(),
            "json" => "JSON Document".to_string(),
            "toml" => "TOML Configuration".to_string(),
            "yaml" | "yml" => "YAML Configuration".to_string(),
            "md" => "Markdown Document".to_string(),
            "txt" => "Plain Text Document".to_string(),
            "pdf" => "PDF Document".to_string(),
            "png" => "PNG Image".to_string(),
            "jpg" | "jpeg" => "JPEG Image".to_string(),
            "svg" => "SVG Vector Image".to_string(),
            "gif" => "GIF Image".to_string(),
            "webp" => "WebP Image".to_string(),
            "mp3" | "flac" | "ogg" | "wav" => "Audio File".to_string(),
            "mp4" | "mkv" | "webm" => "Video File".to_string(),
            _ => format!("{ext} file"),
        }
    } else {
        "File".to_string()
    }
}

pub fn icon_for_file(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.starts_with('.') {
        return "file-hidden-symbolic";
    }
    if is_archive(name)
        || [".gz", ".xz", ".7z", ".rar", ".zst"]
            .iter()
            .any(|ext| lower.ends_with(ext))
    {
        return "package-x-generic-symbolic";
    }
    if is_text_or_code(name) {
        return "text-x-script-symbolic";
    }
    if is_image(name) {
        return "image-x-generic-symbolic";
    }
    if [".pdf", ".djvu", ".epub"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return "x-office-document-symbolic";
    }
    if [".csv", ".tsv", ".ods", ".xlsx", ".xls"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return "x-office-spreadsheet-symbolic";
    }
    if [".mp3", ".flac", ".ogg", ".wav", ".m4a", ".aac", ".opus"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return "audio-x-generic-symbolic";
    }
    if [".mp4", ".mkv", ".webm", ".mov", ".avi"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return "video-x-generic-symbolic";
    }
    "text-x-generic-symbolic"
}

pub fn open_file(file: &gio::File) -> Result<(), glib::Error> {
    gio::AppInfo::launch_default_for_uri(file.uri().as_str(), None::<&gio::AppLaunchContext>)
}

pub fn open_with_text_editor(file: &gio::File) -> Result<(), glib::Error> {
    if let Some(path) = file.path() {
        let editors = [
            "gnome-text-editor",
            "gedit",
            "kate",
            "xed",
            "mousepad",
            "kwrite",
            "code",
        ];
        for editor in editors {
            if std::process::Command::new("which")
                .arg(editor)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                if std::process::Command::new(editor)
                    .arg(&path)
                    .spawn()
                    .is_ok()
                {
                    return Ok(());
                }
            }
        }
    }
    open_file(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn test_is_archive() {
        assert!(is_archive("test.zip"));
        assert!(is_archive("archive.tar.gz"));
        assert!(is_archive("data.tar.xz"));
        assert!(is_archive("backup.tgz"));
        assert!(!is_archive("code.rs"));
        assert!(!is_archive("image.png"));
    }

    #[test]
    fn test_is_text_or_code() {
        assert!(is_text_or_code("main.rs"));
        assert!(is_text_or_code("script.py"));
        assert!(is_text_or_code(".bashrc"));
        assert!(is_text_or_code("Cargo.toml"));
        assert!(!is_text_or_code("picture.jpg"));
        assert!(!is_text_or_code("video.mp4"));
    }

    #[test]
    fn test_is_image() {
        assert!(is_image("photo.png"));
        assert!(is_image("image.jpg"));
        assert!(is_image("vector.svg"));
        assert!(is_image("icon.webp"));
        assert!(!is_image("document.pdf"));
        assert!(!is_image("code.rs"));
    }

    #[test]
    fn test_icon_for_file() {
        assert_eq!(icon_for_file(".hidden"), "file-hidden-symbolic");
        assert_eq!(icon_for_file("file.zip"), "package-x-generic-symbolic");
        assert_eq!(icon_for_file("doc.pdf"), "x-office-document-symbolic");
        assert_eq!(icon_for_file("sheet.xlsx"), "x-office-spreadsheet-symbolic");
        assert_eq!(icon_for_file("photo.jpg"), "image-x-generic-symbolic");
        assert_eq!(icon_for_file("song.mp3"), "audio-x-generic-symbolic");
        assert_eq!(icon_for_file("clip.mp4"), "video-x-generic-symbolic");
    }
}

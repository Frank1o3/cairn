use gtk::gio;
use gtk::gio::prelude::*;
use gtk::glib;

pub fn entry_details(info: &gio::FileInfo, file: &gio::File, is_dir: bool) -> String {
    let mut parts = Vec::new();
    if is_dir {
        parts.push("Folder".to_string());
    } else {
        parts.push(format_size(info.size().max(0) as u64));
    }
    if info.is_symlink() {
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
    parts.join("  ·  ")
}

pub fn display_location(file: &gio::File) -> String {
    file.path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| file.uri().to_string())
}

fn format_size(bytes: u64) -> String {
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

pub fn icon_for_file(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.starts_with('.') {
        return "file-hidden-symbolic";
    }
    if [
        ".rs", ".c", ".h", ".cpp", ".py", ".js", ".ts", ".jsx", ".tsx", ".go", ".java", ".sh",
        ".toml", ".yaml", ".yml", ".json", ".xml", ".html", ".css", ".md",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
    {
        return "text-x-script-symbolic";
    }
    if [".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".avif"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return "image-x-generic-symbolic";
    }
    if [".zip", ".tar", ".gz", ".xz", ".7z", ".rar", ".zst"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return "package-x-generic-symbolic";
    }
    if [".mp3", ".flac", ".ogg", ".wav", ".m4a"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return "audio-x-generic-symbolic";
    }
    if [".mp4", ".mkv", ".webm", ".mov"]
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

use crate::filesystem;
use gtk::gio;
use gtk::gio::prelude::*;
use gtk::glib;
use std::future::Future;
use std::pin::Pin;

const INFO_ATTRIBUTES: &str = "standard::type,standard::is-symlink";

pub async fn copy(source: &gio::File, destination: &gio::File) -> Result<(), glib::Error> {
    copy_tree(source, destination).await
}

fn copy_tree<'a>(
    source: &'a gio::File,
    destination: &'a gio::File,
) -> Pin<Box<dyn Future<Output = Result<(), glib::Error>> + 'a>> {
    Box::pin(async move {
        let info = source
            .query_info_future(
                INFO_ATTRIBUTES,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                glib::Priority::DEFAULT,
            )
            .await?;
        if info.file_type() != gio::FileType::Directory || filesystem::is_symlink(&info) {
            let (future, _progress) = source.copy_future(
                destination,
                gio::FileCopyFlags::NOFOLLOW_SYMLINKS | gio::FileCopyFlags::ALL_METADATA,
                glib::Priority::DEFAULT,
            );
            return future.await;
        }

        destination
            .make_directory_future(glib::Priority::DEFAULT)
            .await?;
        let enumerator = source
            .enumerate_children_future(
                "standard::name",
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT,
            )
            .await?;
        loop {
            let batch = enumerator
                .next_files_future(64, glib::Priority::DEFAULT)
                .await?;
            if batch.is_empty() {
                break;
            }
            for child_info in batch {
                let source_child = enumerator.child(&child_info);
                let destination_child = destination.child(child_info.name());
                copy_tree(&source_child, &destination_child).await?;
            }
        }
        Ok(())
    })
}

pub async fn move_file(source: &gio::File, destination: &gio::File) -> Result<(), glib::Error> {
    copy_tree(source, destination).await?;
    delete_tree(source).await
}

pub async fn trash(file: &gio::File) -> Result<(), glib::Error> {
    file.trash_future(glib::Priority::DEFAULT).await
}

pub async fn delete(file: &gio::File) -> Result<(), glib::Error> {
    delete_tree(file).await
}

fn delete_tree(file: &gio::File) -> Pin<Box<dyn Future<Output = Result<(), glib::Error>> + '_>> {
    Box::pin(async move {
        let info = file
            .query_info_future(
                INFO_ATTRIBUTES,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                glib::Priority::DEFAULT,
            )
            .await?;
        if info.file_type() == gio::FileType::Directory && !filesystem::is_symlink(&info) {
            let enumerator = file
                .enumerate_children_future(
                    "standard::name",
                    gio::FileQueryInfoFlags::NONE,
                    glib::Priority::DEFAULT,
                )
                .await?;
            loop {
                let batch = enumerator
                    .next_files_future(64, glib::Priority::DEFAULT)
                    .await?;
                if batch.is_empty() {
                    break;
                }
                for child_info in batch {
                    let child = enumerator.child(&child_info);
                    delete_tree(&child).await?;
                }
            }
        }
        file.delete_future(glib::Priority::DEFAULT).await
    })
}

pub async fn rename(file: &gio::File, new_name: &str) -> Result<gio::File, glib::Error> {
    file.set_display_name_future(new_name, glib::Priority::DEFAULT)
        .await
}

pub async fn duplicate(source: &gio::File) -> Result<gio::File, glib::Error> {
    let parent = source.parent().ok_or_else(|| {
        glib::Error::new(
            gio::IOErrorEnum::Failed,
            "No parent directory found for duplication",
        )
    })?;
    let basename = source
        .basename()
        .map(|b| b.to_string_lossy().into_owned())
        .unwrap_or_else(|| "copy".to_string());

    let (stem, ext) = if let Some(dot_idx) = basename.rfind('.') {
        if dot_idx > 0 {
            (&basename[..dot_idx], &basename[dot_idx..])
        } else {
            (basename.as_str(), "")
        }
    } else {
        (basename.as_str(), "")
    };

    let mut candidate_name = format!("{stem} (copy){ext}");
    let mut candidate_file = parent.child(&candidate_name);
    let mut count = 2;
    while candidate_file.query_exists(None::<&gio::Cancellable>) {
        candidate_name = format!("{stem} (copy {count}){ext}");
        candidate_file = parent.child(&candidate_name);
        count += 1;
    }

    copy(source, &candidate_file).await?;
    Ok(candidate_file)
}

pub fn calculate_dir_size(
    file: &gio::File,
) -> Pin<Box<dyn Future<Output = Result<(u64, u64), glib::Error>> + '_>> {
    Box::pin(async move {
        let mut total_bytes = 0u64;
        let mut total_items = 0u64;
        let enumerator = file
            .enumerate_children_future(
                "standard::name,standard::type,standard::size,standard::is-symlink",
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                glib::Priority::DEFAULT,
            )
            .await?;
        loop {
            let batch = enumerator
                .next_files_future(128, glib::Priority::DEFAULT)
                .await?;
            if batch.is_empty() {
                break;
            }
            for child_info in batch {
                total_items += 1;
                let child_type = child_info.file_type();
                let is_symlink = filesystem::is_symlink(&child_info);
                if child_type == gio::FileType::Directory && !is_symlink {
                    let child = enumerator.child(&child_info);
                    if let Ok((sub_bytes, sub_items)) = calculate_dir_size(&child).await {
                        total_bytes += sub_bytes;
                        total_items += sub_items;
                    }
                } else {
                    total_bytes += child_info.size().max(0) as u64;
                }
            }
        }
        Ok((total_bytes, total_items))
    })
}

pub async fn empty_trash() -> Result<(), glib::Error> {
    let trash_root = gio::File::for_uri("trash:///");
    let enumerator = trash_root
        .enumerate_children_future(
            "standard::name",
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await?;
    let mut first_error = None;
    loop {
        let batch = enumerator
            .next_files_future(64, glib::Priority::DEFAULT)
            .await?;
        if batch.is_empty() {
            break;
        }
        for item in batch {
            let child = enumerator.child(&item);
            if let Err(error) = delete(&child).await {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

pub async fn restore_from_trash(file: &gio::File) -> Result<(), glib::Error> {
    let info = file
        .query_info_future(
            "trash::orig-path,standard::display-name",
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await?;
    if let Some(orig_path) = info.attribute_byte_string("trash::orig-path") {
        let dest = gio::File::for_path(orig_path.as_str());
        move_file(file, &dest).await
    } else {
        Err(glib::Error::new(
            gio::IOErrorEnum::Failed,
            "Original path not found in trash metadata",
        ))
    }
}

pub async fn find_in_trash(original_path: &std::path::Path) -> Option<gio::File> {
    let trash = gio::File::for_uri("trash:///");
    let enumerator = trash
        .enumerate_children_future(
            "standard::name,trash::orig-path",
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await
        .ok()?;
    loop {
        let batch = enumerator
            .next_files_future(64, glib::Priority::DEFAULT)
            .await
            .ok()?;
        if batch.is_empty() {
            return None;
        }
        for info in batch {
            let Some(path) = info.attribute_byte_string("trash::orig-path") else {
                continue;
            };
            if std::path::Path::new(path.as_str()) == original_path {
                return Some(enumerator.child(&info));
            }
        }
    }
}

pub fn extract_archive(archive_file: &gio::File, destination: &gio::File) -> Result<(), String> {
    let archive_path = archive_file
        .path()
        .ok_or_else(|| "Only local archives can be extracted".to_string())?;
    let dest_path = destination
        .path()
        .ok_or_else(|| "Only local extraction destinations are supported".to_string())?;

    let name = archive_file
        .basename()
        .map(|b| b.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let output = if name.ends_with(".zip") {
        std::process::Command::new("unzip")
            // Never replace an existing file during extraction. This keeps
            // "Extract Here" safe when the destination already has files
            // with names from the archive.
            .arg("-n")
            .arg(&archive_path)
            .arg("-d")
            .arg(&dest_path)
            .output()
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        std::process::Command::new("tar")
            .arg("--keep-old-files")
            .arg("-xzf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&dest_path)
            .output()
    } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        std::process::Command::new("tar")
            .arg("--keep-old-files")
            .arg("-xJf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&dest_path)
            .output()
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
        std::process::Command::new("tar")
            .arg("--keep-old-files")
            .arg("-xjf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&dest_path)
            .output()
    } else if name.ends_with(".tar.zst") {
        std::process::Command::new("tar")
            .arg("--zstd")
            .arg("--keep-old-files")
            .arg("-xf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&dest_path)
            .output()
    } else if name.ends_with(".tar") {
        std::process::Command::new("tar")
            .arg("--keep-old-files")
            .arg("-xf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&dest_path)
            .output()
    } else {
        return Err("Unsupported archive format".to_string());
    };

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
        Err(err) => Err(err.to_string()),
    }
}

pub fn compress_to_archive(
    sources: &[gio::File],
    parent_dir: &gio::File,
    format: &str,
) -> Result<gio::File, String> {
    let parent_path = parent_dir
        .path()
        .ok_or_else(|| "Only local folders can be compressed".to_string())?;

    let default_stem = if sources.len() == 1 {
        sources[0]
            .basename()
            .map(|b| b.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Archive".to_string())
    } else {
        "Archive".to_string()
    };

    let extension = match format {
        "zip" => ".zip",
        "tar.gz" => ".tar.gz",
        _ => return Err("Unsupported archive format".to_string()),
    };

    let mut archive_name = format!("{default_stem}{extension}");
    let mut candidate = parent_dir.child(&archive_name);
    let mut count = 2;
    while candidate.query_exists(None::<&gio::Cancellable>) {
        archive_name = format!("{default_stem} ({count}){extension}");
        candidate = parent_dir.child(&archive_name);
        count += 1;
    }

    let archive_path = candidate
        .path()
        .ok_or_else(|| "Failed to get archive path".to_string())?;

    let mut relative_names = Vec::new();
    for src in sources {
        if let Some(src_path) = src.path() {
            if let Ok(rel) = src_path.strip_prefix(&parent_path) {
                relative_names.push(rel.to_string_lossy().into_owned());
            } else {
                relative_names.push(src_path.to_string_lossy().into_owned());
            }
        }
    }

    if relative_names.is_empty() {
        return Err("No files selected for compression".to_string());
    }

    let output = if format == "zip" {
        let mut cmd = std::process::Command::new("zip");
        cmd.current_dir(&parent_path)
            .arg("-r")
            .arg(&archive_path)
            .args(&relative_names);
        cmd.output()
    } else {
        let mut cmd = std::process::Command::new("tar");
        cmd.current_dir(&parent_path)
            .arg("-czf")
            .arg(&archive_path)
            .args(&relative_names);
        cmd.output()
    };

    match output {
        Ok(out) if out.status.success() => Ok(candidate),
        Ok(out) => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
        Err(err) => Err(err.to_string()),
    }
}

pub fn set_mode_recursive<'a>(
    file: &'a gio::File,
    mode: u32,
) -> Pin<Box<dyn Future<Output = Result<(), glib::Error>> + 'a>> {
    Box::pin(async move {
        file.set_attribute_uint32(
            "unix::mode",
            mode,
            gio::FileQueryInfoFlags::NONE,
            None::<&gio::Cancellable>,
        )?;
        let info = file
            .query_info_future(
                "standard::type,standard::is-symlink",
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                glib::Priority::DEFAULT,
            )
            .await?;
        if info.file_type() == gio::FileType::Directory && !filesystem::is_symlink(&info) {
            let enumerator = file
                .enumerate_children_future(
                    "standard::name",
                    gio::FileQueryInfoFlags::NONE,
                    glib::Priority::DEFAULT,
                )
                .await?;
            loop {
                let batch = enumerator
                    .next_files_future(64, glib::Priority::DEFAULT)
                    .await?;
                if batch.is_empty() {
                    break;
                }
                for child_info in batch {
                    let child = enumerator.child(&child_info);
                    set_mode_recursive(&child, mode).await?;
                }
            }
        }
        Ok(())
    })
}

#[derive(Clone, Debug)]
pub enum UndoAction {
    Create {
        file: gio::File,
        is_dir: bool,
    },
    Rename {
        old_file: gio::File,
        new_file: gio::File,
    },
    Move {
        source: gio::File,
        destination: gio::File,
    },
    Copy {
        source: gio::File,
        destination: gio::File,
    },
    Trash {
        original_path: std::path::PathBuf,
        trash_file: gio::File,
    },
    Duplicate {
        source: gio::File,
        copy_file: gio::File,
    },
}

pub async fn undo_action(action: &UndoAction) -> Result<String, glib::Error> {
    match action {
        UndoAction::Create { file, .. } => {
            delete(file).await?;
            let name = file
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Undid create: deleted {name}"))
        }
        UndoAction::Rename { old_file, new_file } => {
            if let Some(old_name) = old_file.basename() {
                let old_str = old_name.to_string_lossy();
                rename(new_file, &old_str).await?;
                Ok(format!("Undid rename: restored {old_str}"))
            } else {
                Err(glib::Error::new(
                    gio::IOErrorEnum::Failed,
                    "Could not determine original name",
                ))
            }
        }
        UndoAction::Move {
            source,
            destination,
        } => {
            move_file(destination, source).await?;
            let name = source
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Undid move: returned {name}"))
        }
        UndoAction::Copy { destination, .. } => {
            delete(destination).await?;
            let name = destination
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Undid copy: removed {name}"))
        }
        UndoAction::Trash {
            original_path,
            trash_file,
        } => {
            let dest = gio::File::for_path(original_path);
            move_file(trash_file, &dest).await?;
            let name = dest
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Undid trash: restored {name}"))
        }
        UndoAction::Duplicate { copy_file, .. } => {
            delete(copy_file).await?;
            let name = copy_file
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Undid duplicate: removed {name}"))
        }
    }
}

pub async fn redo_action(action: &UndoAction) -> Result<String, glib::Error> {
    match action {
        UndoAction::Create { file, is_dir } => {
            if *is_dir {
                file.make_directory_future(glib::Priority::DEFAULT).await?;
            } else {
                let stream = file
                    .create_future(gio::FileCreateFlags::NONE, glib::Priority::DEFAULT)
                    .await?;
                stream.close_future(glib::Priority::DEFAULT).await?;
            }
            let name = file
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Redid create: {name}"))
        }
        UndoAction::Rename { old_file, new_file } => {
            if let Some(new_name) = new_file.basename() {
                let new_str = new_name.to_string_lossy();
                rename(old_file, &new_str).await?;
                Ok(format!("Redid rename: {new_str}"))
            } else {
                Err(glib::Error::new(
                    gio::IOErrorEnum::Failed,
                    "Could not determine target name",
                ))
            }
        }
        UndoAction::Move {
            source,
            destination,
        } => {
            move_file(source, destination).await?;
            let name = destination
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Redid move: {name}"))
        }
        UndoAction::Copy {
            source,
            destination,
        } => {
            copy(source, destination).await?;
            let name = destination
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Cannot redo copy for {name}"))
        }
        UndoAction::Trash { original_path, .. } => {
            let orig = gio::File::for_path(original_path);
            trash(&orig).await?;
            let name = orig
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Redid trash: {name}"))
        }
        UndoAction::Duplicate { source, copy_file } => {
            copy(source, copy_file).await?;
            let name = copy_file
                .basename()
                .map(|b| b.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(format!("Redid duplicate: {name}"))
        }
    }
}

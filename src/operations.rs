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
        if info.file_type() != gio::FileType::Directory || info.is_symlink() {
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
        if info.file_type() == gio::FileType::Directory && !info.is_symlink() {
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

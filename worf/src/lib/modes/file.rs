use crate::config::{Config, SortOrder, expand_path};
use crate::desktop::spawn_fork;
use crate::gui::{ItemProvider, MenuItem};
use crate::{Error, gui};
use regex::Regex;
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct FileItemProvider<T: Clone> {
    last_result: Option<Vec<MenuItem<T>>>,
    menu_item_data: T,
    sort_order: SortOrder,
}

impl<T: Clone> FileItemProvider<T> {
    pub(crate) fn new(menu_item_data: T, sort_order: SortOrder) -> Self {
        FileItemProvider {
            last_result: None,
            menu_item_data,
            sort_order,
        }
    }

    fn resolve_icon_for_name(path: &Path) -> String {
        let type_result = fs::symlink_metadata(path)
            .map(|meta| meta.file_type())
            .map(|file_type| {
                if file_type.is_symlink() {
                    Some("edit-redo")
                } else if file_type.is_char_device() {
                    Some("input-keyboard")
                } else if file_type.is_block_device() {
                    Some("drive-harddisk")
                } else if file_type.is_socket() {
                    Some("network-transmit-receive")
                } else if file_type.is_fifo() {
                    Some("rotation-allowed")
                } else {
                    None
                }
            })
            .unwrap_or(Some("system-lock-screen"));

        if let Some(tr) = type_result {
            return tr.to_owned();
        }

        let Some(mime) = tree_magic_mini::from_filepath(path) else {
            return "image-not-found".to_string();
        };

        if mime.starts_with("image") {
            return "image-x-generic".to_string();
        }

        if mime.starts_with("inode") {
            return mime.replace('/', "-");
        }

        if mime.starts_with("text") {
            return if mime.contains("plain") {
                "text-x-generic".to_string()
            } else if mime.contains("python") {
                "text-x-script".to_string()
            } else if mime.contains("html") {
                "text-html".to_string()
            } else {
                "text-x-generic".to_string()
            };
        }

        if mime.starts_with("application") {
            return if mime.contains("octet") {
                "application-x-executable".to_string()
            } else if mime.contains("tar")
                || mime.contains("lz")
                || mime.contains("zip")
                || mime.contains("7z")
                || mime.contains("xz")
            {
                "package-x-generic".to_string()
            } else {
                "text-html".to_string()
            };
        }

        log::debug!("unsupported mime type {mime}");
        "application-x-generic".to_string()
    }
}

impl<T: Clone> ItemProvider<T> for FileItemProvider<T> {
    fn get_elements(&mut self, search: Option<&str>) -> (bool, Vec<MenuItem<T>>) {
        let default_path = if let Some(home) = dirs::home_dir() {
            home.display().to_string()
        } else {
            "/".to_string()
        };

        let mut trimmed_search = search.unwrap_or(&default_path).to_owned();
        if !trimmed_search.starts_with('/')
            && !trimmed_search.starts_with('~')
            && !trimmed_search.starts_with('$')
        {
            trimmed_search = format!("{default_path}/{trimmed_search}");
        }

        let path = expand_path(&trimmed_search);
        let mut items: Vec<MenuItem<T>> = Vec::new();

        if !path.exists() {
            if let Some(last) = &self.last_result {
                return (false, last.clone());
            }

            return (true, vec![]);
        }

        if path.is_dir() {
            items.push(MenuItem::new(
                trimmed_search.clone(),
                Some(FileItemProvider::<T>::resolve_icon_for_name(&path)),
                Some(format!("xdg-open {}", path.display())),
                vec![],
                None,
                100.0,
                Some(self.menu_item_data.clone()),
            ));

            if let Ok(entries) = path.read_dir() {
                for entry in entries.flatten() {
                    if let Some(mut path_str) =
                        entry.path().to_str().map(std::string::ToString::to_string)
                    {
                        if trimmed_search.starts_with('~') {
                            if let Some(home_dir) = dirs::home_dir() {
                                if let Some(home_str) = home_dir.to_str() {
                                    path_str = path_str.replace(home_str, "~");
                                }
                            }
                        }

                        if entry.path().is_dir() {
                            path_str.push('/');
                        }

                        items.push(MenuItem::new(
                            path_str.clone(),
                            Some(FileItemProvider::<T>::resolve_icon_for_name(&entry.path())),
                            Some(format!("xdg-open {path_str}")),
                            vec![],
                            None,
                            0.0,
                            Some(self.menu_item_data.clone()),
                        ));
                    }
                }
            }
        } else {
            items.push({
                MenuItem::new(
                    trimmed_search.clone(),
                    Some(FileItemProvider::<T>::resolve_icon_for_name(
                        &PathBuf::from(&trimmed_search),
                    )),
                    Some(format!("xdg-open {trimmed_search}")),
                    vec![],
                    None,
                    0.0,
                    Some(self.menu_item_data.clone()),
                )
            });
        }

        gui::apply_sort(&mut items, &self.sort_order);

        self.last_result = Some(items.clone());
        (true, items)
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> (bool, Option<Vec<MenuItem<T>>>) {
        (false, self.last_result.clone())
    }
}

/// Shows the file browser mode
/// # Errors
///
/// Will return `Err`
/// * if it was not able to spawn the process
///
/// # Panics
/// In case an internal regex does not parse anymore, this should never happen
pub fn show(config: &Config) -> Result<(), Error> {
    let provider = FileItemProvider::new(0, config.sort_order());

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(
        config.clone(),
        provider,
        false,
        Some(vec![Regex::new("^\\$\\w+").unwrap()]),
        None,
    )?;
    if let Some(action) = selection_result.menu.action {
        spawn_fork(&action, selection_result.menu.working_dir.as_ref())
    } else {
        Err(Error::MissingAction)
    }
}

use std::{
    fs,
    os::unix::fs::FileTypeExt,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, RwLock},
};

use regex::Regex;

use crate::{
    Error,
    config::{Config, SortOrder, expand_path},
    desktop::spawn_fork,
    gui::{self, ExpandMode, ItemProvider, MenuItem, ProviderData},
};

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
            .map_or(Some("system-lock-screen"), |file_type| {
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
            });

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
    fn get_elements(&mut self, search: Option<&str>) -> ProviderData<T> {
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
            return ProviderData { items: None };
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
                        if trimmed_search.starts_with('~')
                            && let Some(home_dir) = dirs::home_dir()
                            && let Some(home_str) = home_dir.to_str()
                        {
                            path_str = path_str.replace(home_str, "~");
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
        ProviderData { items: Some(items) }
    }

    fn get_sub_elements(&mut self, item: &MenuItem<T>) -> ProviderData<T> {
        if self.last_result.as_ref().is_some_and(|lr| lr.len() == 1) {
            ProviderData { items: None }
        } else {
            self.get_elements(Some(&item.label))
        }
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
pub fn show(config: &Arc<RwLock<Config>>) -> Result<(), Error> {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\$\w+").unwrap());

    let provider = Arc::new(Mutex::new(FileItemProvider::new(
        0,
        config.read().unwrap().sort_order(),
    )));

    let selection_result = gui::show(
        config,
        provider,
        None,
        Some(vec![RE.clone()]),
        ExpandMode::Verbatim,
        None,
    )?;
    if let Some(action) = selection_result.menu.action {
        spawn_fork(&action, selection_result.menu.working_dir.as_ref())
    } else {
        Err(Error::MissingAction)
    }
}

use gtk4::prelude::*;
use gtk4::{IconLookupFlags, IconTheme, TextDirection};
use home::home_dir;
use ini::configparser::ini::Ini;
use log::{info, warn};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::{fs, string};

pub struct IconResolver {
    cache: HashMap<String, String>,
}

impl IconResolver {
    #![allow(clippy::single_call_fn)]
    pub fn new() -> IconResolver {
        IconResolver {
            cache: HashMap::new(),
        }
    }
    pub fn icon_path(&mut self, icon_name: &str) -> String {
        if let Some(icon_path) = self.cache.get(icon_name) {
            info!("Fetching {icon_name} from cache");
            return icon_path.to_owned();
        }

        info!("Loading icon for {icon_name}");

        let icon = fetch_icon_from_theme(icon_name)
            .or_else(|| fetch_icon_from_common_dirs(icon_name))
            .or_else(|| fetch_icon_from_desktop_file(icon_name))
            .unwrap_or_else(|| {
                warn!("Missing icon for {icon_name}, using fallback");
                default_icon()
            });

        self.cache.insert(icon_name.to_owned(), icon.clone());
        self.cache.get(icon_name).unwrap().to_owned()
    }
}

pub fn default_icon() -> String {
    fetch_icon_from_theme("image-missing").unwrap()
}

fn fetch_icon_from_desktop_file(icon_name: &str) -> Option<String> {
    find_desktop_files().into_iter().find_map(|desktop_file| {
        desktop_file
            .get("Desktop Entry")
            .filter(|desktop_entry| {
                desktop_entry
                    .get("Exec")
                    .and_then(|opt| opt.as_ref())
                    .is_some_and(|exec| exec.to_lowercase().contains(icon_name))
            })
            .map(|desktop_entry| {
                desktop_entry
                    .get("Icon")
                    .and_then(|opt| opt.as_ref())
                    .map(ToOwned::to_owned)
                    .unwrap_or_default()
            })
    })
}

fn fetch_icon_from_theme(icon_name: &str) -> Option<String> {
    let display = gtk4::gdk::Display::default();
    if display.is_none() {
        log::error!("Failed to get display");
    }

    let display = display.unwrap();
    let theme = IconTheme::for_display(&display);
    let icon = theme.lookup_icon(
        icon_name,
        &[],
        32,
        1,
        TextDirection::None,
        IconLookupFlags::empty(),
    );

    icon.file()
        .and_then(|file| file.path())
        .and_then(|path| path.to_str().map(string::ToString::to_string))
}

fn fetch_icon_from_common_dirs(icon_name: &str) -> Option<String> {
    let mut paths = vec![
        PathBuf::from("/usr/local/share/icons"),
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/share/pixmaps"),
        // /usr/share/icons contains the theme icons, handled via separate function
    ];

    if let Some(home) = home_dir() {
        paths.push(home.join(".local/share/icons"));
    }

    let extensions = ["png", "jpg", "gif", "svg"].join("|"); // Create regex group for extensions
    let formatted_name = Regex::new(&format!(r"(?i){icon_name}\.({extensions})$")).unwrap();

    paths
        .into_iter()
        .filter(|dir| dir.exists())
        .find_map(|dir| {
            find_file_case_insensitive(dir.as_path(), &formatted_name)
                .and_then(|files| files.first().map(|f| f.to_string_lossy().into_owned()))
        })
}

fn find_file_case_insensitive(folder: &Path, file_name: &Regex) -> Option<Vec<PathBuf>> {
    if !folder.exists() || !folder.is_dir() {
        return None;
    }
    fs::read_dir(folder).ok().map(|entries| {
        entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_type()
                    .ok()
                    .is_some_and(|file_type| file_type.is_file())
            })
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| file_name.is_match(name))
            })
            .map(|entry| entry.path())
            .collect()
    })
}

pub(crate) fn find_desktop_files() -> Vec<HashMap<String, HashMap<String, Option<String>>>> {
    let mut paths = vec![PathBuf::from("/usr/share/applications")];

    if let Some(home) = home_dir() {
        paths.push(home.join(".local/share/applications"));
    }

    let p: Vec<_> = paths
        .into_iter()
        .filter(|icon_dir| icon_dir.exists())
        .filter_map(|icon_dir| {
            find_file_case_insensitive(&icon_dir, &Regex::new("(?i).*\\.desktop$").unwrap())
        })
        .flat_map(|desktop_files| {
            desktop_files.into_iter().filter_map(|desktop_file| {
                let mut conf = Ini::new();
                conf.load(desktop_file.as_path().to_str().unwrap()).ok()
            })
        })
        .collect();
    p
}

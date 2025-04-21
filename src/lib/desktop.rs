use anyhow::anyhow;
use freedesktop_file_parser::DesktopFile;
use gdk4::Display;
use gtk4::prelude::*;
use gtk4::{IconLookupFlags, IconTheme, TextDirection};
use home::home_dir;
use log::{debug, info, warn};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::{env, fs, string};

#[derive(Debug)]
pub enum DesktopError {
    MissingIcon,
}

//
// #[derive(Clone)]
// pub struct IconResolver {
//     cache: HashMap<String, String>,
// }
//
// impl Default for IconResolver {
//     #[must_use]
//     fn default() -> IconResolver {
//         Self::new()
//     }
// }
//
// impl IconResolver {
//     #[must_use]
//     pub fn new() -> IconResolver {
//         IconResolver {
//             cache: HashMap::new(),
//         }
//     }
//
//     pub fn icon_path_no_cache(&self, icon_name: &str) -> Result<String, DesktopError> {
//         let icon = fetch_icon_from_theme(icon_name)
//             .or_else(|_|
//                 fetch_icon_from_common_dirs(icon_name)
//                     .or_else(|_| default_icon()));
//
//         icon
//     }
//
//     pub fn icon_path(&mut self, icon_name: &str) -> String {
//         if let Some(icon_path) = self.cache.get(icon_name) {
//             return icon_path.to_owned();
//         }
//
//         let icon = self.icon_path_no_cache(icon_name);
//
//         self.cache
//             .entry(icon_name.to_owned())
//             .or_insert_with(|| icon.unwrap_or_default())
//             .to_owned()
//     }
// }

/// # Errors
///
/// Will return `Err` if no icon can be found
pub fn default_icon() -> Result<String, DesktopError> {
    fetch_icon_from_theme("image-missing").map_err(|e| DesktopError::MissingIcon)
}

fn fetch_icon_from_theme(icon_name: &str) -> Result<String, DesktopError> {
    let display = gtk4::gdk::Display::default();
    if display.is_none() {
        log::error!("Failed to get display");
    }

    let display = Display::default().expect("Failed to get default display");
    let theme = IconTheme::for_display(&display);

    let icon = theme.lookup_icon(
        icon_name,
        &[],
        32,
        1,
        TextDirection::None,
        IconLookupFlags::empty(),
    );

    match icon
        .file()
        .and_then(|file| file.path())
        .and_then(|path| path.to_str().map(string::ToString::to_string))
    {
        None => {
            let path = PathBuf::from("/usr/share/icons")
                .join(theme.theme_name())
                .join(format!("{icon_name}.svg"));
            if path.exists() {
                Ok(path.display().to_string())
            } else {
                Err(DesktopError::MissingIcon)
            }
        }
        Some(i) => Ok(i),
    }
}

pub fn fetch_icon_from_common_dirs(icon_name: &str) -> Result<String, DesktopError> {
    let mut paths = vec![
        PathBuf::from("/usr/local/share/icons"),
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/share/pixmaps"),
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
        .ok_or_else(|| DesktopError::MissingIcon)
}

fn find_file_case_insensitive(folder: &Path, file_name: &Regex) -> Option<Vec<PathBuf>> {
    if !folder.exists() || !folder.is_dir() {
        return None;
    }
    fs::read_dir(folder).ok().map(|entries| {
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.path().canonicalize().ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .and_then(|e| e.to_str())
                    .is_some_and(|name| file_name.is_match(name))
            })
            .collect()
    })
}

/// # Errors
///
/// Will return Err when it cannot parse the internal regex
pub fn find_desktop_files() -> Vec<DesktopFile> {
    let mut paths = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];

    if let Some(home) = home_dir() {
        paths.push(home.join(".local/share/applications"));
    }

    if let Ok(xdg_data_home) = env::var("XDG_DATA_HOME") {
        // todo use dirs:: instead
        paths.push(PathBuf::from(xdg_data_home).join(".applications"));
    }

    if let Ok(xdg_data_dir) = env::var("XDG_DATA_DIRS") {
        paths.push(PathBuf::from(xdg_data_dir).join(".applications"));
    }

    let regex = &Regex::new("(?i).*\\.desktop$").unwrap();

    let p: Vec<_> = paths
        .into_iter()
        .filter(|desktop_dir| desktop_dir.exists())
        .filter_map(|icon_dir| find_file_case_insensitive(&icon_dir, regex))
        .flat_map(|desktop_files| {
            desktop_files.into_iter().filter_map(|desktop_file| {
                debug!("loading desktop file {desktop_file:?}");
                fs::read_to_string(desktop_file)
                    .ok()
                    .and_then(|content| freedesktop_file_parser::parse(&content).ok())
            })
        })
        .collect();
    p
}

#[must_use]
pub fn get_locale_variants() -> Vec<String> {
    let locale = env::var("LC_ALL")
        .or_else(|_| env::var("LC_MESSAGES"))
        .or_else(|_| env::var("LANG"))
        .unwrap_or_else(|_| "c".to_string());

    let lang = locale.split('.').next().unwrap_or(&locale).to_lowercase();
    let mut variants = vec![];

    if let Some((lang_part, region)) = lang.split_once('_') {
        variants.push(format!("{lang_part}_{region}")); // en_us
        variants.push(lang_part.to_string()); // en
    } else {
        variants.push(lang.clone()); // e.g. "fr"
    }

    variants
}

// implicit hasher does not make sense here, it is only for desktop files
#[allow(clippy::implicit_hasher)]
#[must_use]
pub fn lookup_name_with_locale(
    locale_variants: &[String],
    variants: &HashMap<String, String>,
    fallback: &str,
) -> Option<String> {
    locale_variants
        .iter()
        .find_map(|local| variants.get(local))
        .map(std::borrow::ToOwned::to_owned)
        .or_else(|| Some(fallback.to_owned()))
}

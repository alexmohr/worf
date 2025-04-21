use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::{env, fs, string};

use freedesktop_file_parser::DesktopFile;
use gdk4::Display;
use gtk4::prelude::*;
use gtk4::{IconLookupFlags, IconTheme, TextDirection};
use home::home_dir;
use log;
use regex::Regex;

#[derive(Debug)]
pub enum DesktopError {
    MissingIcon,
    ParsingError(String),
}

/// # Errors
///
/// Will return `Err` if no icon can be found
pub fn default_icon() -> Result<String, DesktopError> {
    fetch_icon_from_theme("image-missing").map_err(|_| DesktopError::MissingIcon)
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

/// # Errors
///
/// Will return `Err`
/// * if it was not able to find any icon
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
    let formatted_name = Regex::new(&format!(r"(?i){icon_name}\.({extensions})$"));
    if let Ok(formatted_name) = formatted_name {
        paths
            .into_iter()
            .filter(|dir| dir.exists())
            .find_map(|dir| {
                find_file_case_insensitive(dir.as_path(), &formatted_name)
                    .and_then(|files| files.first().map(|f| f.to_string_lossy().into_owned()))
            })
            .ok_or(DesktopError::MissingIcon)
    } else {
        Err(DesktopError::ParsingError(
            "Failed to get formatted icon, likely the internal regex did not parse properly"
                .to_string(),
        ))
    }
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

/// # Panics
///
/// When it cannot parse the internal regex
#[must_use]
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
                log::debug!("loading desktop file {desktop_file:?}");
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

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::{env, fs, string};

use freedesktop_file_parser::DesktopFile;
use futures::stream;
use gdk4::Display;
use gdk4::prelude::FileExt;
use gtk4::{IconLookupFlags, IconTheme, TextDirection};
use rayon::prelude::*;

use futures::StreamExt;
use log;
use regex::Regex;

#[derive(Debug)]
pub enum DesktopError {
    MissingIcon,
    ParsingError(String),
}
//
// /// # Errors
// ///
// /// Will return `Err` if no icon can be found
// pub fn default_icon() -> Result<String, DesktopError> {
//     fetch_icon_from_theme("image-missing").map_err(|_| DesktopError::MissingIcon)
// }
//
// fn fetch_icon_from_theme(icon_name: &str) -> Result<String, DesktopError> {
//     let display = Display::default();
//     if display.is_none() {
//         log::error!("Failed to get display");
//     }
//
//     let display = Display::default().expect("Failed to get default display");
//     let theme = IconTheme::for_display(&display);
//
//     let icon = theme.lookup_icon(
//         icon_name,
//         &[],
//         32,
//         1,
//         TextDirection::None,
//         IconLookupFlags::empty(),
//     );
//
//     match icon
//         .file()
//         .and_then(|file| file.path())
//         .and_then(|path| path.to_str().map(string::ToString::to_string))
//     {
//         None => {
//             let path = PathBuf::from("/usr/share/icons")
//                 .join(theme.theme_name())
//                 .join(format!("{icon_name}.svg"));
//             if path.exists() {
//                 Ok(path.display().to_string())
//             } else {
//                 Err(DesktopError::MissingIcon)
//             }
//         }
//         Some(i) => Ok(i),
//     }
// }

pub fn known_image_extension_regex_pattern() -> Regex {
    Regex::new(&format!(
        r"(?i).*{}",
        format!("\\.({})$", ["png", "jpg", "gif", "svg", "jpeg"].join("|"))
    ))
    .expect("Internal image regex is not valid anymore.")
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

    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local/share/icons"));
    }

    let formatted_name = Regex::new(icon_name);
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

    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local/share/applications"));
    }

    if let Ok(xdg_data_home) = env::var("XDG_DATA_HOME") {
        paths.push(PathBuf::from(xdg_data_home).join(".applications"));
    }

    if let Ok(xdg_data_dirs) = env::var("XDG_DATA_DIRS") {
        for dir in xdg_data_dirs.split(':') {
            paths.push(PathBuf::from(dir).join(".applications"));
        }
    }

    let regex = &Regex::new("(?i).*\\.desktop$").unwrap();

    let p: Vec<_> = paths
        .into_par_iter()
        .filter(|desktop_dir| desktop_dir.exists())
        .filter_map(|icon_dir| find_file_case_insensitive(&icon_dir, regex))
        .flat_map(|desktop_files| {
            desktop_files.into_par_iter().filter_map(|desktop_file| {
                fs::read_to_string(desktop_file)
                    .ok()
                    .and_then(|content| freedesktop_file_parser::parse(&content).ok())
            })
        })
        .collect();
    p
}

pub fn lookup_icon(name: &str, size: i32) -> gtk4::Image {
    let img_regex = Regex::new(&format!(
        r"((?i).*{})|(^/.*)",
        known_image_extension_regex_pattern()
    ));
    let image = if img_regex.unwrap().is_match(name) {
        if let Ok(img) = fetch_icon_from_common_dirs(&name) {
            gtk4::Image::from_file(img)
        } else {
            gtk4::Image::from_icon_name(name)
        }
    } else {
        gtk4::Image::from_icon_name(name)
    };

    image.set_pixel_size(size);

    image
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

use freedesktop_file_parser::DesktopFile;
use gtk4::prelude::*;
use gtk4::{IconLookupFlags, IconTheme, TextDirection};
use home::home_dir;
use ini::configparser::ini::Ini;
use log::{debug, info, warn};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::{env, fs, string};

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
    // find_desktop_files().into_iter().find_map(|desktop_file| {
    //     desktop_file
    //         .get("Desktop Entry")
    //         .filter(|desktop_entry| {
    //             desktop_entry
    //                 .get("Exec")
    //                 .and_then(|opt| opt.as_ref())
    //                 .is_some_and(|exec| exec.to_lowercase().contains(icon_name))
    //         })
    //         .map(|desktop_entry| {
    //             desktop_entry
    //                 .get("Icon")
    //                 .and_then(|opt| opt.as_ref())
    //                 .map(ToOwned::to_owned)
    //                 .unwrap_or_default()
    //         })
    // })
    //todo
    None
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

pub(crate) fn find_desktop_files() -> Vec<DesktopFile> {
    let mut paths = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];

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
                debug!("loading desktop file {:?}", desktop_file);
                fs::read_to_string(desktop_file)
                    .ok()
                    .and_then(|content| freedesktop_file_parser::parse(&content).ok())
            })
        })
        .collect();
    p
}


pub fn get_locale_variants() -> Vec<String> {
    let locale = env::var("LC_ALL")
        .or_else(|_| env::var("LC_MESSAGES"))
        .or_else(|_| env::var("LANG"))
        .unwrap_or_else(|_| "c".to_string());

    let lang = locale.split('.').next().unwrap_or(&locale).to_lowercase();
    let mut variants = vec![];

    if let Some((lang_part, region)) = lang.split_once('_') {
        variants.push(format!("{}_{region}", lang_part)); // en_us
        variants.push(lang_part.to_string()); // en
    } else {
        variants.push(lang.clone()); // e.g. "fr"
    }

    variants
}

pub fn extract_desktop_fields(
    category: &str,
    //keys: Vec<String>,
    desktop_map: &HashMap<String, HashMap<String, Option<String>>>,
) -> HashMap<String, String> {
    let mut result: HashMap<String, String> = HashMap::new();
    let category_map = desktop_map.get(category);
    if category_map.is_none() {
        debug!("No desktop map for category {category}, map data: {desktop_map:?}");
        return result;
    }

    let keys_needed = ["name", "exec", "icon"];
    let locale_variants = get_locale_variants();

    for (map_key, map_value) in category_map.unwrap() {
        for key in keys_needed {
            if result.contains_key(key) || map_value.is_none() {
                continue;
            }

            let (k, v) = locale_variants
                .iter()
                .find(|locale| {
                    let localized_key = format!("{}[{}]", key, locale);
                    key == localized_key
                })
                .map(|_| (Some(key), map_value))
                .unwrap_or_else(|| {
                    if key == map_key {
                        (Some(key), map_value)
                    } else {
                        (None, &None)
                    }
                });
            if let Some(k) = k {
                if let Some(v) = v {
                    result.insert(k.to_owned(), v.clone());
                }
            }
        }

        if result.len() == keys_needed.len() {
            break;
        }
    }

    result
}

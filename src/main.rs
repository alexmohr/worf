#![warn(clippy::pedantic)]
#![allow(clippy::implicit_return)]

use crate::args::{Args, Mode};
use crate::config::{Config, merge_config_with_args};
use crate::desktop::find_desktop_files;
use crate::gui::EntryElement;
use clap::Parser;
use gdk4::prelude::Cast;
use gtk4::prelude::{
    ApplicationExt, ApplicationExtManual, BoxExt, ButtonExt, EditableExt, EntryExt,
    FlowBoxChildExt, GtkWindowExt, ListBoxRowExt, NativeExt, ObjectExt, SurfaceExt, WidgetExt,
};
use gtk4_layer_shell::LayerShell;
use log::{debug, warn};
use merge::Merge;
use std::collections::HashMap;
use std::ops::Deref;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread::sleep;
use std::{env, fs, time};

mod args;
mod config;
mod desktop;
mod gui;

fn main() -> anyhow::Result<()> {
    gtk4::init()?;

    env_logger::Builder::new()
        // todo change to info as default
        .parse_filters(&std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_owned()))
        .init();
    let args = Args::parse();

    let home_dir = std::env::var("HOME")?;
    let config_path = args
        .config
        .as_ref()
        .map(|c| PathBuf::from(c))
        .unwrap_or_else(|| {
            std::env::var("XDG_CONF_HOME")
                .map_or(
                    PathBuf::from(home_dir.clone()).join(".config"),
                    |xdg_conf_home| PathBuf::from(&xdg_conf_home),
                )
                .join("wofi") // todo change to worf
                .join("config")
        });
    // todo use this?
    let colors_dir = std::env::var("XDG_CACHE_HOME")
        .map_or(
            PathBuf::from(home_dir.clone()).join(".cache"),
            |xdg_conf_home| PathBuf::from(&xdg_conf_home),
        )
        .join("wal")
        .join("colors");

    let drun_cache = std::env::var("XDG_CACHE_HOME")
        .map_or(
            PathBuf::from(home_dir.clone()).join(".cache"),
            |xdg_conf_home| PathBuf::from(&xdg_conf_home),
        )
        .join("worf-drun"); // todo change to worf

    let toml_content = fs::read_to_string(config_path)?;
    let mut config: Config = toml::from_str(&toml_content)?; // todo bail out properly
    let config = merge_config_with_args(&mut config, &args)?;

    match args.mode {
        Mode::Run => {}
        Mode::Drun => {
            drun(config)?;
        }
        Mode::Dmenu => {}
    }

    Ok(())
}

fn get_locale_variants() -> Vec<String> {
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

fn extract_desktop_fields(
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
fn drun(mut config: Config) -> anyhow::Result<()> {
    let mut entries: Vec<EntryElement> = Vec::new();
    for file in &find_desktop_files() {
        let n = get_locale_variants()
            .iter()
            .filter_map(|local| file.entry.name.variants.get(local))
            .next()
            .map(|name| name.deref().clone())
            .or_else(|| Some(&file.entry.name.default));

        debug!("{n:?}")

        // let desktop = Some("desktop entry");
        // let locale =
        //     env::var("LC_ALL")
        //     .or_else(|_| env::var("LC_MESSAGES"))
        //     .or_else(|_| env::var("LANG"))
        //     .unwrap_or_else(|_| "en_US.UTF-8".to_string()).split_once(".").map(|(k,_)| k.to_owned().to_lowercase());
        //
        //
        //
        //
        // if let Some(desktop_entry) = file.get("desktop entry") {
        //     let icon = desktop_entry
        //         .get("icon")
        //         .and_then(|x| x.as_ref().map(|x| x.to_owned()));
        //
        //
        //     let Some(exec) = desktop_entry.get("exec")
        //
        //
        //
        //         .and_then(|x| x.as_ref()) else {
        //         warn!("Skipping desktop file {file:#?}");
        //         continue;
        //     };
        //
        //     if let Some((cmd, _)) = exec.split_once(' ') {
        //         if !PathBuf::from(cmd).exists() {
        //             continue;
        //         }
        //     }
        //
        //     let name = desktop_entry
        //         .get("name")
        //         .and_then(|x| x.as_ref().map(|x| x.to_owned()));
        //
        //     if let Some(name) = name {
        //         entries.push({
        //             EntryElement {
        //                 label: name,
        //                 icon_path: icon,
        //                 action: Some(exec.clone()),
        //                 sub_elements: None,
        //             }
        //         })
        //     }
        // }
    }

    entries.sort_by(|l, r| l.label.cmp(&r.label));
    if config.prompt.is_none() {
        config.prompt = Some("drun".to_owned());
    }

    // todo ues a arc instead of cloning the config
    let selected_index = gui::show(config.clone(), entries.clone())?;
    entries.get(selected_index as usize).map(|e| {
        e.action.as_ref().map(|a| {
            spawn_fork(&a);
        })
    });

    Ok(())
}

fn spawn_fork(cmd: &str) {
    // todo fork this for real
    // todo probably remove arguments?
    // Unix-like systems (Linux, macOS)
    let _ = Command::new(cmd)
        .stdin(Stdio::null()) // Disconnect stdin
        .stdout(Stdio::null()) // Disconnect stdout
        .stderr(Stdio::null()) // Disconnect stderr
        .spawn();
    sleep(time::Duration::from_secs(30));
}
//
// fn main() -> anyhow::Result<()> {
//     env_logger::Builder::new()
//         // todo change to info as default
//         .parse_filters(&std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_owned()))
//         .init();
//     let args = Args::parse();
//
//     let home_dir = std::env::var("HOME")?;
//     let config_path = args.config.as_ref().map(|c| PathBuf::from(c)).unwrap_or_else(||{
//         std::env::var("XDG_CONF_HOME")
//             .map_or(
//                 PathBuf::from(home_dir.clone()).join(".config"),
//                 |xdg_conf_home| PathBuf::from(&xdg_conf_home),
//             )
//             .join("wofi")// todo change to ravi
//             .join("config")
//     });
//
//     let colors_dir = std::env::var("XDG_CACHE_HOME")
//         .map_or(
//             PathBuf::from(home_dir.clone()).join(".cache"),
//             |xdg_conf_home| PathBuf::from(&xdg_conf_home),
//         )
//         .join("wal")
//         .join("colors");
//
//     let toml_content = fs::read_to_string(config_path)?;
//     let config: Config = toml::from_str(&toml_content).unwrap_or_default();
//
//
//
//     gtk4::init()?;
//
//     let application = Application::builder()
//         .application_id("com.example.FirstGtkApp")
//         .build();
//
//     application.connect_activate(|app| {
//         let window = ApplicationWindow::builder()
//             .application(app)
//             .title("First GTK Program")
//             .name("window")
//             .default_width(config.x.clone().unwrap())
//             .default_height(config.y.clone().unwrap())
//             .resizable(false)
//             .decorated(false)
//             .build();
//
//
//
//         // Create a dialog window
//         let dialog = Dialog::new();
//         dialog.set_title(Some("Custom Dialog"));
//         dialog.set_default_size(300, 150);
//
//         // Create a vertical box container for the dialog content
//         let mut vbox =gtk4:: Box::new(Orientation::Horizontal, 10);
//
//         // Add a label to the dialog
//         let label = Label::new(Some("This is a custom dialog!"));
//         vbox.append(&label);
//
//         // Set the dialog content
//         dialog.set_child(Some(&vbox));
//
//         // Show the dialog
//         dialog.present();
//     });
//
//     let empty_array: [&str; 0] = [];;
//
//
//     application.run_with_args(&empty_array);
//
//     debug!("merged config result {:#?}", config);
//
//
//     Ok(())
// }

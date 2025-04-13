#![warn(clippy::pedantic)]
#![allow(clippy::implicit_return)]

// todo resolve paths like ~/

use crate::args::{Args, Mode};
use crate::lib::config::{Config, merge_config_with_args};
use crate::lib::desktop::{default_icon, find_desktop_files, get_locale_variants};
use crate::lib::gui;
use crate::lib::gui::MenuItem;
use anyhow::{Error, anyhow};
use clap::Parser;
use freedesktop_file_parser::{DesktopAction, EntryType};
use gdk4::prelude::Cast;
use gtk4::prelude::{
    ApplicationExt, ApplicationExtManual, BoxExt, ButtonExt, EditableExt, EntryExt,
    FlowBoxChildExt, GtkWindowExt, ListBoxRowExt, NativeExt, ObjectExt, SurfaceExt, WidgetExt,
};
use gtk4_layer_shell::LayerShell;
use log::{debug, info, warn};
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
mod lib;

fn main() -> anyhow::Result<()> {
    gtk4::init()?;

    env_logger::Builder::new()
        // todo change to error as default
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_owned()))
        .init();
    let args = Args::parse();

    let home_dir = env::var("HOME")?;
    let config_path = args
        .config
        .as_ref()
        .map(|c| PathBuf::from(c))
        .unwrap_or_else(|| {
            env::var("XDG_CONF_HOME")
                .map_or(
                    PathBuf::from(home_dir.clone()).join(".config"),
                    |xdg_conf_home| PathBuf::from(&xdg_conf_home),
                )
                .join("worf")
                .join("config")
        });

    let drun_cache = env::var("XDG_CACHE_HOME")
        .map_or(
            PathBuf::from(home_dir.clone()).join(".cache"),
            |xdg_conf_home| PathBuf::from(&xdg_conf_home),
        )
        .join("worf-drun");

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

fn lookup_name_with_locale(
    locale_variants: &Vec<String>,
    variants: &HashMap<String, String>,
    fallback: &str,
) -> Option<String> {
    locale_variants
        .iter()
        .filter_map(|local| variants.get(local))
        .next()
        .map(|name| name.to_owned())
        .or_else(|| Some(fallback.to_owned()))
}

fn drun(mut config: Config) -> anyhow::Result<()> {
    let mut entries: Vec<MenuItem> = Vec::new();
    let locale_variants = get_locale_variants();
    let default_icon = default_icon();

    for file in find_desktop_files().iter().filter(|f| {
        f.entry.hidden.map_or(true, |hidden| !hidden)
            && f.entry.no_display.map_or(true, |no_display| !no_display)
    }) {
        let (action, working_dir) = match &file.entry.entry_type {
            EntryType::Application(app) => (app.exec.clone(), app.path.clone()),
            _ => (None, None),
        };

        let name = lookup_name_with_locale(
            &locale_variants,
            &file.entry.name.variants,
            &file.entry.name.default,
        );
        if name.is_none() {
            debug!("Skipping desktop entry without name {file:?}")
        }

        let icon = file
            .entry
            .icon
            .as_ref()
            .map(|s| s.content.clone())
            .or(Some(default_icon.clone()));
        debug!("file, name={name:?}, icon={icon:?}, action={action:?}");
        let mut sort_score = 0.0;
        if name.as_ref().unwrap().contains("ox") {
            sort_score = 999.0;
        }

        let mut entry = MenuItem {
            label: name.unwrap(),
            icon_path: icon.clone(),
            action,
            sub_elements: Vec::default(),
            working_dir: working_dir.clone(),
            initial_sort_score: 0,
            search_sort_score: sort_score,
        };

        file.actions.iter().for_each(|(_, action)| {
            let action_name = lookup_name_with_locale(
                &locale_variants,
                &action.name.variants,
                &action.name.default,
            );
            let action_icon = action
                .icon
                .as_ref()
                .map(|s| s.content.clone())
                .or(icon.as_ref().map(|s| s.clone()));

            debug!("sub, action_name={action_name:?}, action_icon={action_icon:?}");

            let sub_entry = MenuItem {
                label: action_name.unwrap().trim().to_owned(),
                icon_path: action_icon,
                action: action.exec.clone(),
                sub_elements: Vec::default(),
                working_dir: working_dir.clone(),
                initial_sort_score: 0,
                search_sort_score: 0.0,
            };
            entry.sub_elements.push(sub_entry);
        });

        entries.push(entry);
    }

    gui::initialize_sort_scores(&mut entries);

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(config.clone(), entries.clone());
    match selection_result {
        Ok(selected_item) => {
            if let Some(action) = selected_item.action {
                spawn_fork(&action, &selected_item.working_dir)?
            }
        }
        Err(e) => {
            log::error!("{e}");
        }
    }

    Ok(())
}

fn spawn_fork(cmd: &str, working_dir: &Option<String>) -> anyhow::Result<()> {
    // todo probably remove arguments?
    // todo support working dir
    // todo fix actions
    // todo graphical disk map icon not working
    // Unix-like systems (Linux, macOS)

    let parts = cmd.split(' ').collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(anyhow!("empty command passed"));
    }

    if let Some(dir) = working_dir {
        env::set_current_dir(dir)?;
    }

    let exec = parts[0];
    let args: Vec<_> = parts
        .iter()
        .skip(1)
        .filter(|arg| !arg.starts_with("%"))
        .collect();

    unsafe {
        let _ = Command::new(exec)
            .args(args)
            .stdin(Stdio::null()) // Disconnect stdin
            .stdout(Stdio::null()) // Disconnect stdout
            .stderr(Stdio::null()) // Disconnect stderr
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn();
    }
    Ok(())
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

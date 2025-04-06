#![warn(clippy::pedantic)]
#![allow(clippy::implicit_return)]

use crate::args::{Args, Mode};
use crate::config::Config;
use crate::desktop::find_desktop_files;
use crate::gui::EntryElement;
use clap::Parser;
use gdk4::prelude::Cast;
use gtk4::prelude::{
    ApplicationExt, ApplicationExtManual, BoxExt, ButtonExt, EditableExt, EntryExt,
    FlowBoxChildExt, GtkWindowExt, ListBoxRowExt, NativeExt, ObjectExt, SurfaceExt, WidgetExt,
};
use gtk4_layer_shell::LayerShell;
use merge::Merge;
use std::fs;
use std::ops::Deref;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

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
                .join("wofi") // todo change to ravi
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

    let toml_content = fs::read_to_string(config_path)?;
    let mut config: Config = toml::from_str(&toml_content)?; // todo bail out properly

    let icon_resolver = desktop::IconResolver::new();
    match args.mode {
        Mode::Run => {}
        Mode::Drun => {
            let mut entries: Vec<EntryElement> = Vec::new();
            for file in &find_desktop_files() {
                if let Some(desktop_entry) = file.get("desktop entry") {
                    let icon = desktop_entry
                        .get("icon")
                        .and_then(|x| x.as_ref().map(|x| x.to_owned()));
                    let Some(exec) = desktop_entry.get("exec").and_then(|x| x.as_ref().cloned())
                    else {
                        continue;
                    };

                    if let Some((cmd, _)) = exec.split_once(' ') {
                        if !PathBuf::from(cmd).exists() {
                            continue;
                        }
                    }

                    let exec: Arc<String> = Arc::new(exec.into());
                    let action: Box<dyn Fn() + Send> = {
                        let exec = Arc::clone(&exec); // ✅ now it's correct
                        Box::new(move || {
                            spawn_fork(&exec);
                        })
                    };

                    let name = desktop_entry
                        .get("name")
                        .and_then(|x| x.as_ref().map(|x| x.to_owned()));
                    if let Some(name) = name {
                        entries.push({
                            EntryElement {
                                label: name,
                                icon_path: icon,
                                action,
                                sub_elements: None,
                            }
                        })
                    }
                }
            }
            entries.sort_by(|l, r| l.label.cmp(&r.label));
            if config.prompt.is_none() {
                config.prompt = Some("dmenu".to_owned());
            }
            gui::init(config.clone(), entries)?;
        }
        Mode::Dmenu => {}
    }

    Ok(())
}

fn spawn_fork(cmd: &str) {
    // Unix-like systems (Linux, macOS)
    let _ = Command::new(cmd)
        .stdin(Stdio::null()) // Disconnect stdin
        .stdout(Stdio::null()) // Disconnect stdout
        .stderr(Stdio::null()) // Disconnect stderr
        .spawn();
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

#![warn(clippy::pedantic)]
#![allow(clippy::implicit_return)]

// todo resolve paths like ~/

use crate::lib::config::Config;
use crate::lib::desktop::{default_icon, find_desktop_files, get_locale_variants};
use crate::lib::{config, gui, mode};
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
use std::collections::HashMap;
use std::ops::Deref;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread::sleep;
use std::{env, fs, time};

mod lib;


fn main() -> anyhow::Result<()> {
    gtk4::init()?;

    env_logger::Builder::new()
        // todo change to error as default
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_owned()))
        .init();

    let args = config::parse_args();
    let config = config::load_config(Some(args))?;

    if let Some(show) = &config.show {
        match show {
            config::Mode::Run => {}
            config::Mode::Drun => {
                mode::d_run(config)?;
            }
            config::Mode::Dmenu => {}
        }

        Ok(())
    } else {
        Err(anyhow!("No mode provided"))
    }
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

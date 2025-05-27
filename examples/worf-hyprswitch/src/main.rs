use std::{env, sync::Arc};

use hyprland::{
    dispatch::{DispatchType, WindowIdentifier},
    prelude::HyprData,
    shared::Address,
};
use rayon::prelude::*;
use sysinfo::{Pid, System};
use worf_lib::{
    config::{self, Config},
    desktop::EntryType,
    gui::{self, ItemProvider, MenuItem},
};

#[derive(Clone)]
struct WindowProvider {
    windows: Vec<MenuItem<String>>,
}

impl WindowProvider {
    fn new(cfg: &Config) -> Result<Self, String> {
        let clients = hyprland::data::Clients::get().map_err(|e| e.to_string())?;
        let clients: Vec<_> = clients.iter().cloned().collect();

        let desktop_files = Arc::new(worf_lib::desktop::find_desktop_files());

        let mut sys = System::new_all();
        sys.refresh_all();
        let sys = Arc::new(sys);

        let menu_items: Vec<MenuItem<String>> = clients
            .par_iter()
            .filter_map(|c| {
                let sys = Arc::clone(&sys);
                let desktop_files = Arc::clone(&desktop_files);

                let process_name = sys
                    .process(Pid::from_u32(c.pid as u32))
                    .map(|x| x.name().to_string_lossy().into_owned());

                process_name.map(|process_name| {
                    let icon = freedesktop_icons::lookup(&process_name)
                        .with_size(cfg.image_size())
                        .with_scale(1)
                        .find()
                        .map(|icon| icon.to_string_lossy().to_string())
                        .or_else(|| {
                            desktop_files
                                .iter()
                                .find_map(|d| match &d.entry.entry_type {
                                    EntryType::Application(app) => {
                                        if app.startup_wm_class.as_ref().is_some_and(|wm_class| {
                                            *wm_class.to_lowercase()
                                                == c.initial_class.to_lowercase()
                                        }) {
                                            d.entry.icon.as_ref().map(|icon| icon.content.clone())
                                        } else {
                                            None
                                        }
                                    }
                                    _ => None,
                                })
                        });

                    MenuItem::new(
                        format!("{} - {}", c.initial_class, c.title),
                        icon,
                        None,
                        vec![].into_iter().collect(),
                        None,
                        0.0,
                        Some(c.address.to_string()),
                    )
                })
            })
            .collect();

        Ok(Self {
            windows: menu_items,
        })
    }
}

impl ItemProvider<String> for WindowProvider {
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<String>>) {
        (false, self.windows.clone())
    }

    fn get_sub_elements(&mut self, _: &MenuItem<String>) -> (bool, Option<Vec<MenuItem<String>>>) {
        (false, None)
    }
}

fn main() -> Result<(), String> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let args = config::parse_args();
    let config = config::load_config(Some(&args)).unwrap_or(args);

    let provider = WindowProvider::new(&config)?;
    let result = gui::show(config, provider, false, None, None).map_err(|e| e.to_string())?;
    if let Some(window_id) = result.menu.data {
        Ok(
            hyprland::dispatch::Dispatch::call(DispatchType::FocusWindow(
                WindowIdentifier::Address(Address::new(window_id)),
            ))
            .map_err(|e| e.to_string())?,
        )
    } else {
        Err("No window data found".to_owned())
    }
}

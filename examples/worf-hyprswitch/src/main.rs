use hyprland::{
    dispatch::{DispatchType, WindowIdentifier},
    prelude::HyprData,
    shared::Address,
};
use rayon::prelude::*;
use std::collections::HashMap;
use std::{env, fs, sync::Arc, thread};
use sysinfo::{Pid, System};
use worf::{
    Error,
    config::{self, Config},
    desktop,
    desktop::EntryType,
    gui::{self, ItemProvider, MenuItem},
};

#[derive(Clone)]
struct Window {
    process: String,
    address: Address,
    icon: Option<String>,
}

#[derive(Clone)]
struct WindowProvider {
    windows: Vec<MenuItem<Window>>,
}

impl WindowProvider {
    fn new(cfg: &Config, cache: &HashMap<String, String>) -> Result<Self, String> {
        let clients = hyprland::data::Clients::get().map_err(|e| e.to_string())?;
        let clients: Vec<_> = clients.iter().cloned().collect();

        let desktop_files = Arc::new(desktop::find_desktop_files());

        let mut sys = System::new_all();
        sys.refresh_all();
        let sys = Arc::new(sys);

        let menu_items: Vec<MenuItem<_>> = clients
            .par_iter()
            .filter_map(|c| {
                let sys = Arc::clone(&sys);
                let desktop_files = Arc::clone(&desktop_files);

                let process_name = sys
                    .process(Pid::from_u32(c.pid as u32))
                    .map(|x| x.name().to_string_lossy().into_owned());

                process_name.map(|process_name| {
                    let icon = cache.get(&process_name).cloned().or_else(|| {
                        freedesktop_icons::lookup(&process_name)
                            .with_size(cfg.image_size())
                            .with_scale(1)
                            .find()
                            .map(|icon| icon.to_string_lossy().to_string())
                            .or_else(|| {
                                desktop_files
                                    .iter()
                                    .find_map(|d| match &d.entry.entry_type {
                                        EntryType::Application(app) => {
                                            if app.startup_wm_class.as_ref().is_some_and(
                                                |wm_class| {
                                                    *wm_class.to_lowercase()
                                                        == c.initial_class.to_lowercase()
                                                },
                                            ) {
                                                d.entry
                                                    .icon
                                                    .as_ref()
                                                    .map(|icon| icon.content.clone())
                                            } else {
                                                None
                                            }
                                        }
                                        _ => None,
                                    })
                            })
                    });

                    MenuItem::new(
                        format!(
                            "[{}] \t {} \t {}",
                            c.workspace.name, c.initial_class, c.title
                        ),
                        icon.clone(),
                        None,
                        vec![].into_iter().collect(),
                        None,
                        0.0,
                        Some(Window {
                            process: process_name,
                            address: c.address.clone(),
                            icon,
                        }),
                    )
                })
            })
            .collect();

        Ok(Self {
            windows: menu_items,
        })
    }
}

impl ItemProvider<Window> for WindowProvider {
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<Window>>) {
        (false, self.windows.clone())
    }

    fn get_sub_elements(&mut self, _: &MenuItem<Window>) -> (bool, Option<Vec<MenuItem<Window>>>) {
        (false, None)
    }
}

fn load_icon_cache(cache_path: &String) -> Result<HashMap<String, String>, Error> {
    let toml_content =
        fs::read_to_string(cache_path).map_err(|e| Error::UpdateCacheError(format!("{e}")))?;
    let cache: HashMap<String, String> = toml::from_str(&toml_content)
        .map_err(|_| Error::ParsingError("failed to parse cache".to_owned()))?;
    Ok(cache)
}

fn cache_path() -> Result<String, Error> {
    let path = dirs::cache_dir()
        .map(|x| x.join("worf-hyprswitch"))
        .ok_or_else(|| Error::UpdateCacheError("cannot read cache file".to_owned()))?;

    desktop::create_file_if_not_exists(&path)?;
    Ok(path.to_string_lossy().into_owned())
}

fn main() -> Result<(), String> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let args = config::parse_args();
    let config = config::load_config(Some(&args)).unwrap_or(args);

    let cache_path = cache_path().map_err(|err| err.to_string())?;
    let mut cache = load_icon_cache(&cache_path).map_err(|e| e.to_string())?;

    let provider = WindowProvider::new(&config, &cache)?;
    let windows = provider.windows.clone();
    let update_cache = thread::spawn(move || {
        windows.iter().for_each(|item| {
            if let Some(window) = &item.data {
                if let Some(icon) = &window.icon {
                    cache.insert(window.process.clone(), icon.clone());
                }
            }
        });
        let updated_toml = toml::to_string(&cache);
        match updated_toml {
            Ok(toml) => {
                fs::write(cache_path, toml).map_err(|e| Error::UpdateCacheError(e.to_string()))
            }
            Err(e) => Err(Error::UpdateCacheError(e.to_string())),
        }
    });
    let result = gui::show(config, provider, false, None, None).map_err(|e| e.to_string())?;

    if let Some(window) = result.menu.data {
        hyprland::dispatch::Dispatch::call(DispatchType::FocusWindow(WindowIdentifier::Address(
            window.address,
        )))
        .map_err(|e| e.to_string())?;
        Ok(update_cache.join().unwrap().map_err(|e| e.to_string())?)
    } else {
        Err("No window data found".to_owned())
    }
}

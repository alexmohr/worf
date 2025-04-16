use crate::lib::config::Config;
use crate::lib::desktop::{default_icon, find_desktop_files, get_locale_variants};
use crate::lib::gui;
use crate::lib::gui::MenuItem;
use crate::lookup_name_with_locale;
use anyhow::{Context, anyhow};
use freedesktop_file_parser::EntryType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::prelude::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, fs, io};

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DRunCache {
    desktop_entry: String,
    run_count: usize,
}

pub fn d_run(mut config: Config) -> anyhow::Result<()> {
    let locale_variants = get_locale_variants();
    let default_icon = default_icon();

    let cache_path = dirs::cache_dir().map(|x| x.join("worf-drun"));
    let mut d_run_cache = {
        if let Some(ref cache_path) = cache_path {
            if let Err(e) = create_file_if_not_exists(cache_path) {
                log::warn!("No drun cache file and cannot create: {e:?}");
            }
        }

        load_cache_file(&cache_path).unwrap_or_default()
    };

    let mut entries: Vec<MenuItem<String>> = Vec::new();
    for file in find_desktop_files().iter().filter(|f| {
        f.entry.hidden.map_or(true, |hidden| !hidden)
            && f.entry.no_display.map_or(true, |no_display| !no_display)
    }) {
        let (action, working_dir) = match &file.entry.entry_type {
            EntryType::Application(app) => (app.exec.clone(), app.path.clone()),
            _ => (None, None),
        };

        let name = match lookup_name_with_locale(
            &locale_variants,
            &file.entry.name.variants,
            &file.entry.name.default,
        ) {
            Some(name) => name,
            None => {
                log::debug!("Skipping desktop entry without name {file:?}");
                continue;
            }
        };

        let icon = file
            .entry
            .icon
            .as_ref()
            .map(|s| s.content.clone())
            .or(Some(default_icon.clone()));
        log::debug!("file, name={name:?}, icon={icon:?}, action={action:?}");
        let sort_score = d_run_cache.get(&name).unwrap_or(&0);

        let mut entry: MenuItem<String> = MenuItem {
            label: name,
            icon_path: icon.clone(),
            action,
            sub_elements: Vec::default(),
            working_dir: working_dir.clone(),
            initial_sort_score: -(*sort_score),
            search_sort_score: 0.0,
            data: None,
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

            log::debug!("sub, action_name={action_name:?}, action_icon={action_icon:?}");

            let sub_entry = MenuItem {
                label: action_name.unwrap().trim().to_owned(),
                icon_path: action_icon,
                action: action.exec.clone(),
                sub_elements: Vec::default(),
                working_dir: working_dir.clone(),
                initial_sort_score: 0, // subitems are never sorted right now.
                search_sort_score: 0.0,
                data: None,
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
            if let Some(cache) = cache_path {
                *d_run_cache.entry(selected_item.label).or_insert(0) += 1;
                if let Err(e) = save_cache_file(&cache, d_run_cache) {
                    log::warn!("cannot save drun cache {e:?}");
                }
            }

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

fn save_cache_file(path: &PathBuf, data: HashMap<String, i64>) -> anyhow::Result<()> {
    // Convert the HashMap to TOML string
    let toml_string = toml::ser::to_string(&data).map_err(|e| anyhow::anyhow!(e))?;
    fs::write(path, toml_string).map_err(|e| anyhow::anyhow!(e))
}

fn load_cache_file(cache_path: &Option<PathBuf>) -> anyhow::Result<HashMap<String, i64>> {
    let path = match cache_path {
        Some(p) => p,
        None => return Err(anyhow!("Cache is missing")),
    };

    let toml_content = fs::read_to_string(path)?;
    let parsed: toml::Value = toml_content.parse().expect("Failed to parse TOML");

    let mut result: HashMap<String, i64> = HashMap::new();
    if let toml::Value::Table(table) = parsed {
        for (key, val) in table {
            if let toml::Value::Integer(i) = val {
                result.insert(key, i);
            } else {
                log::warn!("Skipping key '{}' because it's not an integer", key);
            }
        }
    }
    Ok(result)
}

fn create_file_if_not_exists(path: &PathBuf) -> anyhow::Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path);

    match file {
        Ok(_) => Ok(()),

        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e).context(format!("Failed to create file {}", path.display()))?,
    }
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
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn();
    }
    Ok(())
}

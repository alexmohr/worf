use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Instant,
};

use freedesktop_file_parser::EntryType;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    Error,
    config::{Config, SortOrder},
    desktop::{
        find_desktop_files, get_locale_variants, lookup_name_with_locale, save_cache_file,
        spawn_fork,
    },
    gui::{self, ItemProvider, MenuItem},
    modes::load_cache,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DRunCache {
    desktop_entry: String,
    run_count: usize,
}

#[derive(Clone)]
pub(crate) struct DRunProvider<T: Clone> {
    items: Option<Vec<MenuItem<T>>>,
    pub(crate) cache_path: PathBuf,
    pub(crate) cache: HashMap<String, i64>,
    data: T,
    no_actions: bool,
    sort_order: SortOrder,
    terminal: Option<String>,
}

impl<T: Clone + Send + Sync> ItemProvider<T> for DRunProvider<T> {
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<T>>) {
        if self.items.is_none() {
            self.items = Some(self.load().clone());
        }
        (false, self.items.clone().unwrap())
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> (bool, Option<Vec<MenuItem<T>>>) {
        (false, None)
    }
}

impl<T: Clone + Send + Sync> DRunProvider<T> {
    pub(crate) fn new(menu_item_data: T, config: &Config) -> Self {
        let (cache_path, d_run_cache) = load_cache("drun_cache", config).unwrap();
        DRunProvider {
            items: None,
            cache_path,
            cache: d_run_cache,
            data: menu_item_data,
            no_actions: config.no_actions(),
            sort_order: config.sort_order(),
            terminal: config.term(),
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_precision_loss)]
    fn load(&self) -> Vec<MenuItem<T>> {
        let locale_variants = get_locale_variants();
        let default_icon = "application-x-executable".to_string();
        let start = Instant::now();

        let entries: Vec<MenuItem<T>> = find_desktop_files()
            .into_par_iter()
            .filter(|file| {
                !file.entry.no_display.unwrap_or(false)
                    && !file.entry.hidden.unwrap_or(false)
            })
            .filter_map(|file| {
                let name = lookup_name_with_locale(
                    &locale_variants,
                    &file.entry.name.variants,
                    &file.entry.name.default,
                )?;

                let (action, working_dir, in_terminal) = match &file.entry.entry_type {
                    EntryType::Application(app) => (app.exec.clone(), app.path.clone(), app.terminal.unwrap_or(false)),
                    _ => return None,
                };

                let cmd_exists = action
                    .as_ref()
                    .and_then(|a| {
                        a.split(' ')
                            .next()
                            .map(|cmd| cmd.replace('"', ""))
                            .map(|cmd| PathBuf::from(&cmd).exists() || which::which(&cmd).is_ok())
                    })
                    .unwrap_or(false);

                if !cmd_exists {
                    log::warn!("Skipping desktop entry for {name:?} because action {action:?} does not exist");
                    return None;
                }

                let icon = file
                    .entry
                    .icon
                    .as_ref()
                    .map(|s| s.content.clone())
                    .or(Some(default_icon.clone()));

                let sort_score = *self.cache.get(&name).unwrap_or(&0) as f64;

                let mut entry = MenuItem::new(
                    name.clone(),
                    icon.clone(),
                    self.get_action(in_terminal, action, &name),
                    Vec::new(),
                    working_dir.clone(),
                    sort_score,
                    Some(self.data.clone()),
                );

                if !self.no_actions {
                    for action in file.actions.values() {
                        if let Some(action_name) = lookup_name_with_locale(
                            &locale_variants,
                            &action.name.variants,
                            &action.name.default,
                        ) {
                            let action_icon = action
                                .icon
                                .as_ref()
                                .map(|s| s.content.clone())
                                .or(icon.clone())
                                .unwrap_or("application-x-executable".to_string());


                            let action = self.get_action(in_terminal, action.exec.clone(), &action_name);

                            entry.sub_elements.push(MenuItem::new(
                                action_name,
                                Some(action_icon),
                                action,
                                Vec::new(),
                                working_dir.clone(),
                                0.0,
                                Some(self.data.clone()),
                            ));
                        }
                    }
                }
                Some(entry)
            })
            .collect();

        let mut seen_actions = HashSet::new();
        let mut entries: Vec<MenuItem<T>> = entries
            .into_iter()
            .filter(|entry| seen_actions.insert(entry.action.clone()))
            .collect();

        log::info!(
            "parsing desktop files took {}ms",
            start.elapsed().as_millis()
        );

        gui::apply_sort(&mut entries, &self.sort_order);
        entries
    }

    fn get_action(
        &self,
        in_terminal: bool,
        action: Option<String>,
        action_name: &String,
    ) -> Option<String> {
        if in_terminal {
            match self.terminal.as_ref() {
                None => {
                    log::warn!("No terminal configured for terminal app {action_name}");
                    None
                }
                Some(terminal) => action.map(|cmd| format!("{terminal} {cmd}")),
            }
        } else {
            action
        }
    }
}

pub(crate) fn update_drun_cache_and_run<T: Clone>(
    cache_path: &PathBuf,
    cache: &mut HashMap<String, i64>,
    selection_result: MenuItem<T>,
) -> Result<(), crate::Error> {
    *cache.entry(selection_result.label).or_insert(0) += 1;
    if let Err(e) = save_cache_file(cache_path, cache) {
        log::warn!("cannot save drun cache {e:?}");
    }

    if let Some(action) = selection_result.action {
        spawn_fork(&action, selection_result.working_dir.as_ref())
    } else {
        Err(Error::MissingAction)
    }
}

/// Shows the drun mode
/// # Errors
///
/// Will return `Err` if it was not able to spawn the process
pub fn show(config: &Config) -> Result<(), Error> {
    let provider = DRunProvider::new(0, config);
    let cache_path = provider.cache_path.clone();
    let mut cache = provider.cache.clone();

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(config.clone(), provider, false, None, None);
    match selection_result {
        Ok(s) => update_drun_cache_and_run(&cache_path, &mut cache, s.menu)?,
        Err(_) => {
            log::error!("No item selected");
        }
    }

    Ok(())
}

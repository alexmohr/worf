use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::CString,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    Error,
    config::{Config, SortOrder},
    desktop::{is_executable, save_cache_file},
    gui::{self, ArcProvider, ExpandMode, ItemProvider, MenuItem, ProviderData},
    modes::load_cache,
};

impl ItemProvider<()> for RunProvider {
    fn get_elements(&mut self, query: Option<&str>) -> ProviderData<()> {
        if self.items.is_none() {
            self.items = Some(self.load().clone());
        }
        if query.is_some() {
            ProviderData { items: None }
        } else {
            ProviderData {
                items: self.items.clone(),
            }
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<()>) -> ProviderData<()> {
        ProviderData {
            items: self.items.clone(),
        }
    }
}

#[derive(Clone)]
struct RunProvider {
    items: Option<Vec<MenuItem<()>>>,
    cache_path: PathBuf,
    cache: HashMap<String, i64>,
    sort_order: SortOrder,
}

impl RunProvider {
    fn new(config: &Config) -> Result<Self, Error> {
        let (cache_path, d_run_cache) = load_cache("worf-run", config)?;
        Ok(RunProvider {
            items: None,
            cache_path,
            cache: d_run_cache,
            sort_order: config.sort_order(),
        })
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_precision_loss)]
    fn load(&self) -> Vec<MenuItem<()>> {
        let path_var = env::var("PATH").unwrap_or_default();
        let paths = env::split_paths(&path_var);

        let entries: Vec<_> = paths
            .filter(|dir| dir.is_dir())
            .flat_map(|dir| {
                fs::read_dir(dir)
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .filter_map(|entry| {
                        let path = entry.path();
                        if !is_executable(&path) {
                            return None;
                        }

                        let label = path.file_name()?.to_str()?.to_string();
                        let sort_score = *self.cache.get(&label).unwrap_or(&0) as f64;

                        Some(MenuItem::new(
                            label,
                            None,
                            path.to_str().map(ToString::to_string),
                            vec![],
                            None,
                            sort_score,
                            None,
                        ))
                    })
            })
            .collect();

        let mut seen_actions = HashSet::new();
        let mut entries: Vec<MenuItem<()>> = entries
            .into_iter()
            .filter(|entry| {
                entry
                    .action
                    .as_ref()
                    .and_then(|action| action.split('/').next_back())
                    .is_some_and(|cmd| seen_actions.insert(cmd.to_string()))
            })
            .collect();

        gui::apply_sort(&mut entries, &self.sort_order);
        entries
    }
}

fn update_run_cache_and_run<T: Clone>(
    cache_path: &PathBuf,
    cache: &mut HashMap<String, i64>,
    selection_result: MenuItem<T>,
) -> Result<(), Error> {
    *cache.entry(selection_result.label).or_insert(0) += 1;
    if let Err(e) = save_cache_file(cache_path, cache) {
        log::warn!("cannot save run cache {e:?}");
    }

    if let Some(action) = selection_result.action {
        let program = CString::new(action).unwrap();
        let args = [program.clone()];

        // This replaces the current process image
        nix::unistd::execvp(&program, &args).map_err(|e| Error::RunFailed(e.to_string()))?;
        Ok(())
    } else {
        Err(Error::MissingAction)
    }
}

/// Shows the run mode
/// # Errors
///
/// Will return `Err` if it was not able to spawn the process
/// # Panics
/// When failing to unwrap the arc lock
pub fn show(config: &Arc<RwLock<Config>>) -> Result<(), Error> {
    let provider = Arc::new(Mutex::new(RunProvider::new(&config.read().unwrap())?));
    let arc_provider = Arc::clone(&provider) as ArcProvider<()>;

    let selection_result = gui::show(config, arc_provider, None, None, ExpandMode::Verbatim, None);
    match selection_result {
        Ok(s) => {
            let prov = provider.lock().unwrap();
            update_run_cache_and_run(&prov.cache_path, &mut prov.cache.clone(), s.menu)?;
        }
        Err(_) => {
            log::error!("No item selected");
        }
    }

    Ok(())
}

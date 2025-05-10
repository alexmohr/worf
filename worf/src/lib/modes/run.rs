use std::collections::{HashMap, HashSet};
use std::{env, fs};
use std::ffi::CString;
use std::path::PathBuf;
use crate::config::{Config, SortOrder};
use crate::desktop::{is_executable, save_cache_file};
use crate::{gui, Error};
use crate::gui::{ItemProvider, MenuItem};
use crate::modes::load_cache;

impl ItemProvider<i32> for RunProvider {
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<i32>>) {
        if self.items.is_none() {
            self.items = Some(self.load().clone());
        }
        (false, self.items.clone().unwrap())
    }

    fn get_sub_elements(&mut self, _: &MenuItem<i32>) -> (bool, Option<Vec<MenuItem<i32>>>) {
        (false, None)
    }
}

#[derive(Clone)]
struct RunProvider {
    items: Option<Vec<MenuItem<i32>>>,
    cache_path: Option<PathBuf>,
    cache: HashMap<String, i64>,
    sort_order: SortOrder,
}

impl RunProvider {
    fn new(sort_order: SortOrder) -> Self {
        let (cache_path, d_run_cache) = load_run_cache();
        RunProvider {
            items: None,
            cache_path,
            cache: d_run_cache,
            sort_order,
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_precision_loss)]
    fn load(&self) -> Vec<MenuItem<i32>> {
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
        let mut entries: Vec<MenuItem<i32>> = entries
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


fn load_run_cache() -> (Option<PathBuf>, HashMap<String, i64>) {
    let cache_path = dirs::cache_dir().map(|x| x.join("worf-run"));
    load_cache(cache_path)
}

fn update_run_cache_and_run<T: Clone>(
    cache_path: Option<PathBuf>,
    cache: &mut HashMap<String, i64>,
    selection_result: MenuItem<T>,
) -> Result<(), Error> {
    if let Some(cache_path) = cache_path {
        *cache.entry(selection_result.label).or_insert(0) += 1;
        if let Err(e) = save_cache_file(&cache_path, cache) {
            log::warn!("cannot save run cache {e:?}");
        }
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
pub fn show(config: &Config) -> Result<(), Error> {
    let provider = RunProvider::new(config.sort_order());
    let cache_path = provider.cache_path.clone();
    let mut cache = provider.cache.clone();

    let selection_result = gui::show(config.clone(), provider, false, None, None);
    match selection_result {
        Ok(s) => update_run_cache_and_run(cache_path, &mut cache, s.menu)?,
        Err(_) => {
            log::error!("No item selected");
        }
    }

    Ok(())
}

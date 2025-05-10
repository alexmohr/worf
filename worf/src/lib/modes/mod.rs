use crate::desktop::{create_file_if_not_exists, load_cache_file};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod auto;
pub mod dmenu;
pub mod drun;
pub mod emoji;
pub mod file;
pub mod math;
pub mod run;
pub mod ssh;

pub(crate) fn load_cache(cache_path: Option<PathBuf>) -> (Option<PathBuf>, HashMap<String, i64>) {
    let cache = {
        if let Some(ref cache_path) = cache_path {
            if let Err(e) = create_file_if_not_exists(cache_path) {
                log::warn!("No drun cache file and cannot create: {e:?}");
            }
        }

        load_cache_file(cache_path.as_ref()).unwrap_or_default()
    };
    (cache_path, cache)
}

use std::{collections::HashMap, fs, path::PathBuf};

use crate::{
    Error,
    config::Config,
    desktop::{cache_file_path, create_file_if_not_exists, load_cache_file},
};

pub mod auto;
pub mod dmenu;
pub mod drun;
pub mod emoji;
pub mod file;
pub mod math;
pub mod run;
pub mod search;
pub mod ssh;

pub(crate) fn load_cache(
    name: &str,
    config: &Config,
) -> Result<(PathBuf, HashMap<String, i64>), Error> {
    let cache_path = cache_file_path(config, name)?;
    let cache = {
        if let Err(e) = create_file_if_not_exists(&cache_path) {
            log::warn!("No drun cache file and cannot create: {e:?}");
        }

        match load_cache_file(&cache_path) {
            Ok(cache) => cache,
            Err(e) => {
                // Keep the broken file, it would be overwritten with an empty cache otherwise.
                let mut backup = cache_path.clone().into_os_string();
                backup.push(".bak");
                log::error!(
                    "Cannot read cache {}: {e}, moving it to {}",
                    cache_path.display(),
                    backup.display()
                );
                if let Err(e) = fs::rename(&cache_path, &backup) {
                    log::error!("Cannot move broken cache: {e}");
                }
                HashMap::new()
            }
        }
    };
    Ok((cache_path, cache))
}

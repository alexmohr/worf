use anyhow::anyhow;
use std::env;
use std::path::PathBuf;

pub fn home_dir() -> Result<String, anyhow::Error> {
    env::var("HOME").map_err(|e| anyhow::anyhow!("$HOME not set: {e}"))
}

pub fn conf_home() -> Result<String, anyhow::Error> {
    env::var("XDG_CONF_HOME").map_err(|e| anyhow::anyhow!("XDG_CONF_HOME not set: {e}"))
}

pub fn config_path(config_path: Option<String>) -> Result<PathBuf, anyhow::Error> {
    config_path
        .map(PathBuf::from)
        .and_then(|p| p.canonicalize().ok().filter(|c| c.exists()))
        .or_else(|| {
            [
                conf_home().ok().map(PathBuf::from),
                home_dir()
                    .ok()
                    .map(PathBuf::from)
                    .map(|c| c.join(".config")),
            ]
            .into_iter()
            .flatten()
            .map(|base| base.join("worf").join("style.css"))
            .find_map(|p| p.canonicalize().ok())
        })
        .ok_or_else(|| anyhow!("Could not find a valid config file."))
}

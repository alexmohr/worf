use std::{
    fs,
    sync::{Arc, LazyLock, Mutex, RwLock},
};

use regex::Regex;

use crate::{
    Error,
    config::{Config, SortOrder},
    desktop::{parse_exec, start_forked},
    gui::{self, ExpandMode, ItemProvider, MenuItem, ProviderData},
};

#[derive(Clone)]
pub(crate) struct SshProvider<T: Clone> {
    items: Vec<MenuItem<T>>,
}

impl<T: Clone> SshProvider<T> {
    pub(crate) fn new(menu_item_data: T, order: &SortOrder) -> Self {
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*Host\s+(.+)$").unwrap());
        let re = &*RE;
        let mut items: Vec<_> = dirs::home_dir()
            .map(|home| home.join(".ssh").join("config"))
            .filter(|path| path.exists())
            .map(|path| fs::read_to_string(&path).unwrap_or_default())
            .into_iter()
            .flat_map(|content| {
                re.captures_iter(&content)
                    .flat_map(|cap| {
                        cap[1]
                            .split_whitespace()
                            .map(|host| {
                                log::debug!("found ssh host {host}");
                                MenuItem::new(
                                    host.to_owned(),
                                    Some("computer".to_owned()),
                                    Some(format!("ssh {host}")),
                                    vec![],
                                    None,
                                    0.0,
                                    Some(menu_item_data.clone()),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        gui::apply_sort(&mut items, order);
        Self { items }
    }
}

impl<T: Clone> ItemProvider<T> for SshProvider<T> {
    fn get_elements(&mut self, query: Option<&str>) -> ProviderData<T> {
        if query.is_some() {
            ProviderData { items: None }
        } else {
            ProviderData {
                items: Some(self.items.clone()),
            }
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> ProviderData<T> {
        ProviderData { items: None }
    }
}

pub(crate) fn launch<T: Clone>(menu_item: &MenuItem<T>, config: &Config) -> Result<(), Error> {
    let host = menu_item.label.trim();
    if host.is_empty() {
        return Err(Error::MissingAction);
    }
    let term = config
        .term()
        .ok_or_else(|| Error::RunFailed("no terminal found to run ssh in".to_owned()))?;

    let mut args = parse_exec(&term);
    if args.is_empty() {
        return Err(Error::RunFailed("terminal command is empty".to_owned()));
    }
    let exec = args.remove(0);
    // The host is passed as argument to bash, so it is never interpreted by the shell.
    args.extend(
        [
            "bash",
            "-c",
            "source ~/.bashrc; exec ssh -- \"$1\"",
            "bash",
            host,
        ]
        .map(ToOwned::to_owned),
    );
    start_forked(&exec, args)
}

/// Shows the ssh mode
/// # Errors
///
/// Will return `Err`
/// * if it was not able to spawn the process
/// * if it didn't find a terminal
/// # Panics
/// When failing to unwrap the arc lock
pub fn show(config: &Arc<RwLock<Config>>) -> Result<(), Error> {
    let provider = Arc::new(Mutex::new(SshProvider::new(
        0,
        &config.read().unwrap().sort_order(),
    )));
    let selection_result = gui::show(config, provider, None, None, ExpandMode::Verbatim, None);
    if let Ok(mi) = selection_result {
        launch(&mi.menu, &config.read().unwrap())?;
    } else {
        log::error!("No item selected");
    }
    Ok(())
}

use crate::config::{Config, SortOrder};
use crate::desktop::spawn_fork;
use crate::gui::{ItemProvider, MenuItem};
use crate::{Error, gui};
use regex::Regex;
use std::fs;

#[derive(Clone)]
pub(crate) struct SshProvider<T: Clone> {
    elements: Vec<MenuItem<T>>,
}

impl<T: Clone> SshProvider<T> {
    pub(crate) fn new(menu_item_data: T, order: &SortOrder) -> Self {
        let re = Regex::new(r"(?m)^\s*Host\s+(.+)$").unwrap();
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
        Self { elements: items }
    }
}

impl<T: Clone> ItemProvider<T> for SshProvider<T> {
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<T>>) {
        (false, self.elements.clone())
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> (bool, Option<Vec<MenuItem<T>>>) {
        (false, None)
    }
}

pub(crate) fn launch<T: Clone>(menu_item: &MenuItem<T>, config: &Config) -> Result<(), Error> {
    let ssh_cmd = if let Some(action) = &menu_item.action {
        action.clone()
    } else {
        let cmd = config
            .term()
            .map(|s| format!("{s} ssh {}", menu_item.label));
        if let Some(cmd) = cmd {
            cmd
        } else {
            return Err(Error::MissingAction);
        }
    };

    let cmd = format!(
        "{} bash -c \"source ~/.bashrc; {ssh_cmd}\"",
        config.term().unwrap_or_default()
    );
    spawn_fork(&cmd, menu_item.working_dir.as_ref())
}

/// Shows the ssh mode
/// # Errors
///
/// Will return `Err`
/// * if it was not able to spawn the process
/// * if it didn't find a terminal
pub fn show(config: &Config) -> Result<(), Error> {
    let provider = SshProvider::new(0, &config.sort_order());
    let selection_result = gui::show(config.clone(), provider, true, None, None);
    if let Ok(mi) = selection_result {
        launch(&mi.menu, config)?;
    } else {
        log::error!("No item selected");
    }
    Ok(())
}

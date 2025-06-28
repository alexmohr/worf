use std::sync::{Arc, Mutex, RwLock};
use urlencoding::encode;

use crate::{
    Error,
    config::Config,
    desktop::spawn_fork,
    gui::{self, ArcFactory, DefaultItemFactory, ExpandMode, ItemProvider, MenuItem, ProviderData},
};

#[derive(Clone)]
pub(crate) struct SearchProvider<T: Clone> {
    search_query: String,
    data: T,
}

impl<T: Clone> SearchProvider<T> {
    pub fn new(data: T, search_query: String) -> Self {
        Self {
            search_query,
            data: data.clone(),
        }
    }
}

impl<T: Clone> ItemProvider<T> for SearchProvider<T> {
    fn get_elements(&mut self, query: Option<&str>) -> ProviderData<T> {
        if let Some(query) = query {
            let url = format!("{}{}", self.search_query, encode(query));
            let run_search = MenuItem::new(
                format!("Search {query}"),
                None,
                Some(format!("xdg-open {url}")),
                vec![],
                None,
                0.0,
                Some(self.data.clone()),
            );

            ProviderData {
                items: Some(vec![run_search]),
            }
        } else {
            ProviderData { items: None }
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> ProviderData<T> {
        ProviderData { items: None }
    }
}

/// Shows the web search mode
/// # Errors
///
/// Forwards errors from the gui. See `gui::show` for details.
/// # Panics
/// When failing to unwrap the arc lock
pub fn show(config: &Arc<RwLock<Config>>) -> Result<(), Error> {
    let provider = Arc::new(Mutex::new(SearchProvider::new(
        (),
        config.read().unwrap().search_query(),
    )));
    let factory: ArcFactory<()> = Arc::new(Mutex::new(DefaultItemFactory::new()));
    let selection_result = gui::show(
        config,
        provider,
        Some(factory),
        None,
        ExpandMode::Verbatim,
        None,
    )?;
    match selection_result.menu.action {
        None => Err(Error::MissingAction),
        Some(action) => spawn_fork(&action, None),
    }
}

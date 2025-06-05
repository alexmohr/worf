use urlencoding::encode;

use crate::desktop::spawn_fork;
use crate::{
    Error,
    config::Config,
    gui::{self, ItemProvider, MenuItem},
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
    fn get_elements(&mut self, query: Option<&str>) -> (bool, Vec<MenuItem<T>>) {
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
            (true, vec![run_search])
        } else {
            (false, vec![])
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> (bool, Option<Vec<MenuItem<T>>>) {
        (false, None)
    }
}

/// Shows the emoji mode
/// # Errors
///
/// Forwards errors from the gui. See `gui::show` for details.
pub fn show(config: &Config) -> Result<(), Error> {
    let provider = SearchProvider::new(String::new(), config.search_query());
    let selection_result = gui::show(config.clone(), provider, true, None, None)?;
    match selection_result.menu.action {
        None => Err(Error::MissingAction),
        Some(action) => spawn_fork(&action, None),
    }
}

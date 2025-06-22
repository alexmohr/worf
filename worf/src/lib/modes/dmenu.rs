use std::{
    io::{self, Read},
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    Error,
    config::{Config, SortOrder},
    gui::{self, DefaultItemFactory, ExpandMode, ItemProvider, MenuItem, ProviderData},
};

#[derive(Clone)]
struct DMenuProvider {
    items: Vec<MenuItem<String>>,
}

impl DMenuProvider {
    fn new(sort_order: &SortOrder) -> DMenuProvider {
        log::debug!("parsing stdin");
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .expect("Failed to read from stdin");

        let mut items: Vec<MenuItem<String>> = input
            .lines()
            .rev()
            .map(|s| MenuItem::new(s.to_string(), None, None, vec![], None, 0.0, None))
            .collect();
        log::debug!("parsed stdin");
        gui::apply_sort(&mut items, sort_order);
        Self { items }
    }
}
impl ItemProvider<String> for DMenuProvider {
    fn get_elements(&mut self, query: Option<&str>) -> ProviderData<String> {
        if query.is_some() {
            ProviderData { items: None }
        } else {
            ProviderData {
                items: Some(self.items.clone()),
            }
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<String>) -> ProviderData<String> {
        ProviderData { items: None }
    }
}

/// Shows the dmenu mode
/// # Errors
///
/// Forwards errors from the gui. See `gui::show` for details.
pub fn show(config: Arc<RwLock<Config>>) -> Result<(), Error> {
    let provider = Arc::new(Mutex::new(DMenuProvider::new(
        &config.read().unwrap().sort_order(),
    )));

    let selection_result = gui::show(
        config,
        provider,
        Some(Arc::new(Mutex::new(DefaultItemFactory::new()))),
        None,
        ExpandMode::Verbatim,
        None,
    );
    match selection_result {
        Ok(s) => {
            println!("{}", s.menu.label);
            Ok(())
        }
        Err(_) => Err(Error::InvalidSelection),
    }
}

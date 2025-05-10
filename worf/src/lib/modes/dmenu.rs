use std::io;
use std::io::Read;
use crate::config::{Config, SortOrder};
use crate::{gui, Error};
use crate::gui::{ItemProvider, MenuItem};

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
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<String>>) {
        (false, self.items.clone())
    }

    fn get_sub_elements(&mut self, _: &MenuItem<String>) -> (bool, Option<Vec<MenuItem<String>>>) {
        (false, None)
    }
}


/// Shows the dmenu mode
/// # Errors
///
/// Forwards errors from the gui. See `gui::show` for details.
pub fn show(config: &Config) -> Result<(), Error> {
    let provider = DMenuProvider::new(&config.sort_order());

    let selection_result = gui::show(config.clone(), provider, true, None, None);
    match selection_result {
        Ok(s) => {
            println!("{}", s.menu.label);
            Ok(())
        }
        Err(_) => Err(Error::InvalidSelection),
    }
}
use crate::config::Config;
use crate::gui;
use crate::gui::{ItemProvider, MenuItem};

#[derive(Clone)]
pub(crate) struct MathProvider<T: Clone> {
    menu_item_data: T,
    pub(crate) elements: Vec<MenuItem<T>>,
}

impl<T: Clone> MathProvider<T> {
    pub(crate) fn new(menu_item_data: T) -> Self {
        Self {
            menu_item_data,
            elements: vec![],
        }
    }
    fn add_elements(&mut self, elements: &mut Vec<MenuItem<T>>) {
        self.elements.append(elements);
    }
}

impl<T: Clone> ItemProvider<T> for MathProvider<T> {
    fn get_elements(&mut self, search: Option<&str>) -> (bool, Vec<MenuItem<T>>) {
        if let Some(search_text) = search {
            let result = match meval::eval_str(search_text) {
                Ok(result) => result.to_string(),
                Err(e) => format!("failed to calculate {e:?}"),
            };

            let item = MenuItem::new(
                result,
                None,
                search.map(String::from),
                vec![],
                None,
                0.0,
                Some(self.menu_item_data.clone()),
            );
            let mut result = vec![item];
            result.append(&mut self.elements.clone());
            (true, result)
        } else {
            (false, self.elements.clone())
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> (bool, Option<Vec<MenuItem<T>>>) {
        (false, None)
    }
}

/// Shows the math mode
pub fn show(config: &Config) {
    let mut calc: Vec<MenuItem<String>> = vec![];
    loop {
        let mut provider = MathProvider::new(String::new());
        provider.add_elements(&mut calc.clone());
        let selection_result = gui::show(config.clone(), provider, true, None, None);
        if let Ok(mi) = selection_result {
            calc.push(mi.menu);
        } else {
            log::error!("No item selected");
            break;
        }
    }
}

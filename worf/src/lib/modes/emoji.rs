use crate::config::{Config, SortOrder};
use crate::desktop::copy_to_clipboard;
use crate::gui::{ItemProvider, MenuItem};
use crate::{Error, gui};

#[derive(Clone)]
pub(crate) struct EmojiProvider<T: Clone> {
    elements: Vec<MenuItem<T>>,
    #[allow(dead_code)] // needed for the detection of mode in 'auto'
    menu_item_data: T,
}

impl<T: Clone> EmojiProvider<T> {
    pub(crate) fn new(data: T, sort_order: &SortOrder, hide_label: bool) -> Self {
        let emoji = emoji::search::search_annotation_all("");
        let mut menus = emoji
            .into_iter()
            .map(|e| {
                MenuItem::new(
                    if hide_label {
                        e.glyph.to_string()
                    } else {
                        format!("{} — Category: {} — Name: {}", e.glyph, e.group, e.name)
                    },
                    None,
                    Some(format!(
                        "emoji {} — Category: {} — Name: {}",
                        e.glyph, e.group, e.name
                    )),
                    vec![],
                    None,
                    0.0,
                    Some(data.clone()),
                )
            })
            .collect::<Vec<_>>();
        gui::apply_sort(&mut menus, sort_order);

        Self {
            elements: menus,
            menu_item_data: data.clone(),
        }
    }
}

impl<T: Clone> ItemProvider<T> for EmojiProvider<T> {
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<T>>) {
        (false, self.elements.clone())
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
    let provider = EmojiProvider::new(0, &config.sort_order(), config.emoji_hide_label());
    let selection_result = gui::show(config.clone(), provider, true, None, None)?;
    match selection_result.menu.action {
        None => Err(Error::MissingAction),
        Some(action) => copy_to_clipboard(action, None),
    }
}

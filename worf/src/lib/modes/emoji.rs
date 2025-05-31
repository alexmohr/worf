use crate::{
    Error,
    config::{Config, SortOrder},
    desktop::copy_to_clipboard,
    gui::{self, ItemProvider, MenuItem},
};

#[derive(Clone)]
pub(crate) struct EmojiProvider {
    elements: Vec<MenuItem<String>>,
}

impl EmojiProvider {
    pub(crate) fn new(sort_order: &SortOrder, hide_label: bool) -> Self {
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
                    Some(e.glyph.to_string()),
                )
            })
            .collect::<Vec<_>>();
        gui::apply_sort(&mut menus, sort_order);

        Self { elements: menus }
    }
}

impl ItemProvider<String> for EmojiProvider {
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<String>>) {
        (false, self.elements.clone())
    }

    fn get_sub_elements(&mut self, _: &MenuItem<String>) -> (bool, Option<Vec<MenuItem<String>>>) {
        (false, None)
    }
}

/// Shows the emoji mode
/// # Errors
///
/// Forwards errors from the gui. See `gui::show` for details.
pub fn show(config: &Config) -> Result<(), Error> {
    let provider = EmojiProvider::new(&config.sort_order(), config.emoji_hide_label());
    let selection_result = gui::show(config.clone(), provider, true, None, None)?;
    match selection_result.menu.data {
        None => Err(Error::MissingAction),
        Some(action) => copy_to_clipboard(action, None),
    }
}

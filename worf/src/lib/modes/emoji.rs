use std::sync::{Arc, Mutex, RwLock};

use crate::{
    Error,
    config::{Config, SortOrder, TextOutputMode},
    desktop::copy_to_clipboard,
    gui::{self, ExpandMode, ItemProvider, MenuItem, ProviderData},
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
    fn get_elements(&mut self, query: Option<&str>) -> ProviderData<String> {
        if query.is_some() {
            ProviderData { items: None }
        } else {
            ProviderData {
                items: Some(self.elements.clone()),
            }
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<String>) -> ProviderData<String> {
        ProviderData { items: None }
    }
}

/// Shows the emoji mode
/// # Errors
///
/// Forwards errors from the gui. See `gui::show` for details.
/// # Panics
/// When failing to unwrap the arc lock
pub fn show(config: &Arc<RwLock<Config>>) -> Result<(), Error> {
    let cfg = config.read().unwrap();
    let provider = Arc::new(Mutex::new(EmojiProvider::new(
        &cfg.sort_order(),
        cfg.emoji_hide_label(),
    )));
    drop(cfg);

    let selection_result = gui::show(config, provider, None, None, ExpandMode::Verbatim, None)?;
    match selection_result.menu.data {
        None => Err(Error::MissingAction),
        Some(action) => match config.read().unwrap().text_output_mode() {
            TextOutputMode::Clipboard => {
                copy_to_clipboard(action, None)?;
                Ok(())
            }
            TextOutputMode::StandardOutput => {
                println!("{action}");
                Ok(())
            }
            TextOutputMode::None => Ok(()),
        },
    }
}

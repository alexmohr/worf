//! Keyboard handling: navigation, editing the search and custom keys.

use std::rc::Rc;

use gdk4::{
    glib::Propagation,
    prelude::{Cast, ObjectExt},
};
use gtk4::{
    EventControllerKey, Expander, FlowBoxChild, ListBox, ListBoxRow,
    prelude::{EditableExt, EventControllerExt, FlowBoxChildExt, ListBoxRowExt, WidgetExt},
};
use log;

use super::{
    ChildPosition, CustomKeys, ExpandMode, MetaData, UiElements, build_ui_from_menu_items,
    close_gui, find_visible_child, get_selected_item, handle_selected_item, has_command_modifier,
    is_key_match, modifiers_from_mask, select_visible_child, set_search_text,
    significant_modifiers, unshifted_key, update_view, update_view_from_provider,
};
use crate::{Error, config::KeyDetectionType, desktop};

pub(super) fn setup_key_event_handler<T: Clone + 'static + Send>(
    ui_elements: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    custom_keys: Option<&CustomKeys>,
) {
    fn connect_key_handler<
        T: ObjectExt + Clone + 'static + WidgetExt,
        Menu: Clone + 'static + Send,
    >(
        widget: &T,
        ui: &Rc<UiElements<Menu>>,
        meta: &Rc<MetaData<Menu>>,
        keys: Option<CustomKeys>,
    ) {
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let ui = Rc::clone(ui);
        let meta = Rc::clone(meta);
        controller.connect_key_pressed(move |_, key_value, key_code, modifier| {
            handle_key_press(&ui, &meta, key_value, key_code, modifier, keys.as_ref())
        });
        widget.add_controller(controller.clone());
    }

    // Setup window controller
    connect_key_handler(&ui_elements.window, ui_elements, meta, custom_keys.cloned());
}

pub(super) fn handle_key_press<T: Clone + 'static + Send>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    keyboard_key: gdk4::Key,
    key_code: u32,
    modifier_type: gdk4::ModifierType,
    custom_keys: Option<&CustomKeys>,
) -> Propagation {
    log::debug!("received key. code: {key_code}, key: {keyboard_key:?}");

    let propagate =
        handle_custom_keys(ui, meta, keyboard_key, key_code, modifier_type, custom_keys);

    if propagate == Propagation::Stop {
        return propagate;
    }

    match keyboard_key {
        gdk4::Key::BackSpace | gdk4::Key::Delete => {
            let mut query = {
                let search_text = ui.search_text.lock().unwrap();
                search_text.clone()
            };
            let pos = ui.search.position();
            // Backspace removes the char before the cursor, Delete the one after it.
            let del_pos = if keyboard_key == gdk4::Key::BackSpace {
                pos - 1
            } else {
                pos
            };
            if let Ok(del_idx) = usize::try_from(del_pos)
                && let Some((start, ch)) = query.char_indices().nth(del_idx)
            {
                query.replace_range(start..start + ch.len_utf8(), "");
                set_search_text(ui, meta, &query);
                ui.search.set_position(del_pos);
                update_view_from_provider(ui, meta, &query);
            }
        }
        gdk4::Key::Home => {
            ui.search.set_position(0);
        }
        gdk4::Key::End => {
            // -1 moves the cursor behind the last char
            ui.search.set_position(-1);
        }
        gdk4::Key::Up | gdk4::Key::Left => {
            return move_selection(ui, meta, &Direction::Up);
        }
        gdk4::Key::Down | gdk4::Key::Right => {
            return move_selection(ui, meta, &Direction::Down);
        }
        _ => {
            if let Some(c) = keyboard_key.to_unicode()
                && !c.is_control()
                && !has_command_modifier(modifier_type)
            {
                let mut query = {
                    let search_text = ui.search_text.lock().unwrap();
                    search_text.clone()
                };

                let pos = ui.search.position();
                // position never is negative here.
                #[allow(clippy::cast_sign_loss)]
                let byte_idx = query
                    .char_indices()
                    .nth(pos as usize)
                    .map_or_else(|| query.len(), |(i, _)| i);
                query.insert(byte_idx, c);
                set_search_text(ui, meta, &query);
                ui.search.set_position(pos + 1);
                update_view_from_provider(ui, meta, &query);
            }
        }
    }
    Propagation::Proceed
}

#[derive(PartialEq)]
pub(super) enum Direction {
    Up,
    Down,
}

pub(super) fn collapse_expanders<T: Clone>(ui: &UiElements<T>) {
    let children: Vec<FlowBoxChild> = ui.menu_rows.read().unwrap().keys().cloned().collect();
    for child in children {
        if let Some(c) = child.child()
            && let Ok(expander) = c.downcast::<Expander>()
        {
            expander.set_expanded(false);
        }
    }
}

pub(super) fn move_selection<T: Clone + Send + 'static>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    direction: &Direction,
) -> Propagation {
    let selected_children = ui.main_box.selected_children();
    let Some(selected) = selected_children.first() else {
        return Propagation::Proceed;
    };

    // If the selected FlowBoxChild contains an expanded Expander and one of its
    // ListBox rows is focused, handle edge navigation between sub-items and
    // the surrounding FlowBox children.
    // Read everything needed from the menu rows up front, so no lock is held while
    // widgets are changed below (which can trigger handlers that need the lock).
    let (total, visible_items_count, sub_element_count) = {
        let list_items = ui.menu_rows.read().unwrap();
        (
            list_items.len(),
            list_items.iter().filter(|(_, menu)| menu.visible).count(),
            list_items
                .get(selected)
                .map_or(0, |item| item.sub_elements.len()),
        )
    };
    if sub_element_count > 0
        && let Some(parent_widget) = selected.child()
        && let Ok(expander) = parent_widget.downcast::<Expander>()
        && expander.is_expanded()
        && let Some(list_box) = expander.child().and_then(|w| w.downcast::<ListBox>().ok())
        && let Some(selected_row) = list_box.selected_row()
    {
        let idx = selected_row.index();
        // Count children using the model data (MenuItem.sub_elements)
        let child_count = sub_element_count;

        // Moving down from the last sub-item -> select next FlowBox child
        #[allow(clippy::cast_sign_loss)]
        if *direction == Direction::Down && (idx as usize) == child_count.saturating_sub(1) {
            // find index of `selected` inside main_box using the number of menu_rows
            let mut sel_index: Option<usize> = None;
            for i in 0..total {
                if let Some(child) = ui.main_box.child_at_index(i.try_into().unwrap_or(0))
                    && child == *selected
                {
                    sel_index = Some(i);
                    expander.set_expanded(false);
                    break;
                }
            }

            if let Some(i) = sel_index {
                // pick next visible child after the expander
                for j in (i + 1)..total {
                    if let Some(candidate) = ui.main_box.child_at_index(j.try_into().unwrap_or(0))
                        && candidate.is_visible()
                    {
                        ui.main_box.select_child(&candidate);
                        candidate.grab_focus();
                        candidate.activate();
                        return Propagation::Stop;
                    }
                }
            }
        }

        // Moving up from the first sub-item -> focus parent expander
        if *direction == Direction::Up {
            return if idx == 0 {
                // make sure the FlowBoxChild is selected and focus the expander
                ui.main_box.select_child(selected);
                // Try to focus the expander itself so the user clearly moved to the parent
                let _ = expander.grab_focus();
                expander.set_expanded(false);
                Propagation::Stop
            } else {
                Propagation::Proceed
            };
        }
    } else {
        collapse_expanders(ui);
    }

    // Everything below wraps around at the start and end of the list.
    if !meta.config.read().unwrap().rollover() {
        return Propagation::Proceed;
    }

    let Some(first_child) = find_visible_child(
        &ui.menu_rows.read().unwrap(),
        &ui.main_box,
        &ChildPosition::Front,
    ) else {
        return Propagation::Proceed;
    };

    let Some(last_child) = find_visible_child(
        &ui.menu_rows.read().unwrap(),
        &ui.main_box,
        &ChildPosition::Back,
    ) else {
        return Propagation::Proceed;
    };

    if *direction == Direction::Up && first_child == *selected && visible_items_count > 1 {
        select_visible_child(
            &ui.menu_rows.read().unwrap(),
            &ui.main_box,
            &ui.scroll,
            &ChildPosition::Back,
        );
        Propagation::Stop
    } else if *direction == Direction::Down && last_child == *selected && visible_items_count > 1 {
        select_visible_child(
            &ui.menu_rows.read().unwrap(),
            &ui.main_box,
            &ui.scroll,
            &ChildPosition::Front,
        );
        Propagation::Stop
    } else {
        Propagation::Proceed
    }
}

pub(super) fn handle_custom_keys<T: Clone + 'static + Send>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    keyboard_key: gdk4::Key,
    key_code: u32,
    modifier_type: gdk4::ModifierType,
    custom_keys: Option<&CustomKeys>,
) -> Propagation {
    let detection_type = meta.config.read().unwrap().key_detection_type();
    if let Some(custom_keys) = custom_keys {
        let mods = significant_modifiers(&modifiers_from_mask(modifier_type));
        // With Shift held the key value is the shifted symbol (i.e. `!` for `1`),
        // so also compare the unshifted key of the same physical key.
        let unshifted_key = unshifted_key(key_code);
        let custom_key = custom_keys.bindings.iter().find(|custom_key| {
            let key_match = if detection_type == KeyDetectionType::Code {
                custom_key.key == key_code.into()
            } else {
                custom_key.key == keyboard_key.to_upper().into()
                    || unshifted_key.is_some_and(|k| custom_key.key == k.to_upper().into())
            };
            key_match && significant_modifiers(&custom_key.modifiers) == mods
        });

        if let Some(custom_key) = custom_key {
            log::debug!("custom key {custom_key:?} matched");
            let search_text = ui.search_text.lock().unwrap().clone();
            if let Err(e) =
                handle_selected_item(ui, meta, Some(&search_text), None, Some(custom_key))
            {
                log::error!("{e}");
            }
            // The key is handled, it must not end up in the search.
            return Propagation::Stop;
        }
    }

    // hide search
    if is_key_match(
        meta.config.read().unwrap().key_hide_search(),
        &detection_type,
        key_code,
        keyboard_key,
    ) {
        handle_key_hide_search(ui)
    // submit
    } else if is_key_match(
        Some(meta.config.read().unwrap().key_submit()),
        &detection_type,
        key_code,
        keyboard_key,
    ) {
        handle_key_submit(ui, meta)
    // exit
    } else if is_key_match(
        Some(meta.config.read().unwrap().key_exit()),
        &detection_type,
        key_code,
        keyboard_key,
    ) {
        handle_key_exit(ui, meta)
    // copy
    } else if is_key_match(
        meta.config.read().unwrap().key_copy(),
        &detection_type,
        key_code,
        keyboard_key,
    ) {
        handle_key_copy(ui, meta)
    // expand
    } else if is_key_match(
        Some(meta.config.read().unwrap().key_expand()),
        &detection_type,
        key_code,
        keyboard_key,
    ) {
        handle_key_expand(ui, meta)
    } else {
        Propagation::Proceed
    }
}

pub(super) fn handle_key_exit<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    if let Err(e) = meta.selected_sender.send(Err(Error::NoSelection)) {
        log::error!("failed to send message {e}");
    }
    close_gui(&ui.app);
    Propagation::Stop
}

pub(super) fn handle_key_expand<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    if let Some(fb) = ui.main_box.selected_children().first()
        && let Some(child) = fb.child()
    {
        let expander = child.downcast::<Expander>().ok();
        if let Some(expander) = expander {
            expander.set_expanded(true);

            if let Some(list_box) = expander.child().and_then(|w| w.downcast::<ListBox>().ok())
                && let Some(first_row) = list_box.first_child()
            {
                first_row.grab_focus();
                list_box.select_row(first_row.downcast_ref::<ListBoxRow>());
            }
        } else {
            let data = {
                let lock = ui.menu_rows.read().unwrap();
                let menu_item = lock.get(fb);
                menu_item.map(|menu_item| {
                    (
                        meta.item_provider
                            .lock()
                            .unwrap()
                            .get_sub_elements(menu_item),
                        menu_item.clone(),
                    )
                })
            };

            if let Some((provider_data, menu_item)) = data {
                if let Some(items) = provider_data.items {
                    build_ui_from_menu_items(ui, meta, items);
                    let query = match meta.expand_mode {
                        ExpandMode::Verbatim => menu_item.label.clone(),
                        ExpandMode::WithSpace => format!("{} ", menu_item.label.clone()),
                    };

                    set_search_text(ui, meta, &query);
                    if let Ok(new_pos) = i32::try_from(query.len() + 1) {
                        ui.search.set_position(new_pos);
                    }

                    update_view(ui, meta, &query);
                } else if let Err(e) = handle_selected_item(ui, meta, None, Some(menu_item), None) {
                    log::error!("{e}");
                }
            }
        }
    }
    Propagation::Stop
}

pub(super) fn handle_key_copy<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    if let Some(item) = get_selected_item(ui)
        && let Some(action) = item.action
        && let Err(e) = desktop::copy_to_clipboard(action, None)
    {
        log::error!("failed to copy to clipboard: {e}");
    }
    if let Err(e) = meta.selected_sender.send(Err(Error::NoSelection)) {
        log::error!("failed to send message {e}");
    }
    close_gui(&ui.app);
    Propagation::Stop
}

pub(super) fn handle_key_submit<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    let search_lock = ui.search_text.lock().unwrap();
    if let Err(e) = handle_selected_item(ui, meta, Some(&search_lock), None, None) {
        log::error!("{e}");
    }
    Propagation::Stop
}

pub(super) fn handle_key_hide_search<T>(ui: &Rc<UiElements<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    ui.search.set_visible(!ui.search.is_visible());
    Propagation::Stop
}

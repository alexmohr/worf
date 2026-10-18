//! Creating the rows of the menu and selecting them.

use std::{collections::HashMap, rc::Rc, sync::LazyLock, time::Instant};

use gdk4::{glib, prelude::Cast};
use gtk4::{
    Align, Expander, FlowBox, FlowBoxChild, GestureClick, Image, Label, ListBox, ListBoxRow,
    ScrolledWindow, Widget,
    glib::ControlFlow,
    prelude::{AdjustmentExt, BoxExt, FlowBoxChildExt, GestureSingleExt, ListBoxRowExt, WidgetExt},
};
use log;
use regex::Regex;

use super::{
    ArcMenuMap, MenuItem, MetaData, UiElements, handle_selected_item, parse_label,
    set_menu_visibility_for_search, sort_flow_box_childs, truncate_markup, update_view,
};
use crate::{
    config::{self, Config, SortOrder},
    desktop::known_image_extension_regex_pattern,
};

pub(super) fn build_ui_from_menu_items<T: Clone + 'static + Send>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    mut items: Vec<MenuItem<T>>,
) {
    if meta.config.read().unwrap().sort_order() != SortOrder::Default {
        items.reverse();
    }
    let start = Instant::now();
    {
        while let Some(b) = ui.main_box.child_at_index(0) {
            ui.main_box.remove(&b);
            drop(b);
        }
        ui.menu_rows.write().unwrap().clear();
        // Sorting while rows are added would re-sort the whole box for every batch,
        // so the sort function is only set once all rows exist.
        ui.main_box.unset_sort_func();

        let generation = ui.build_generation.get().wrapping_add(1);
        ui.build_generation.set(generation);

        let meta_clone = Rc::<MetaData<T>>::clone(meta);
        let ui_clone = Rc::<UiElements<T>>::clone(ui);

        glib::idle_add_local(move || {
            if ui_clone.build_generation.get() != generation {
                // A newer build replaced this one, drop the remaining items.
                return ControlFlow::Break;
            }

            let mut batch = HashMap::new();
            for _ in 0..25 {
                let Some(item) = items.pop() else {
                    break;
                };
                batch.insert(add_menu_item(&ui_clone, &meta_clone, &item), item);
            }
            let done = items.is_empty();

            // Only the new rows need their visibility computed.
            let search_text = ui_clone.search_text.lock().unwrap().clone();
            set_menu_visibility_for_search(
                &search_text,
                &mut batch,
                &meta_clone.config,
                meta_clone.search_ignored_words.as_ref(),
            );
            ui_clone.menu_rows.write().unwrap().extend(batch);

            if done {
                let items_sort = ArcMenuMap::clone(&ui_clone.menu_rows);
                ui_clone.main_box.set_sort_func(move |child1, child2| {
                    sort_flow_box_childs(child1, child2, &items_sort)
                });

                log::debug!(
                    "Created {} menu items in {:?}",
                    ui_clone.menu_rows.read().unwrap().len(),
                    start.elapsed()
                );
                update_view(&ui_clone, &meta_clone, &search_text);

                ControlFlow::Break
            } else {
                ControlFlow::Continue
            }
        });
    }
}

pub(super) fn add_menu_item<T: Clone + 'static + Send>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    element_to_add: &MenuItem<T>,
) -> FlowBoxChild {
    let parent: Widget = if element_to_add.sub_elements.is_empty() {
        create_menu_row(ui, meta, element_to_add).upcast()
    } else {
        let expander = Expander::new(None);
        expander.set_widget_name("expander-box");
        expander.set_hexpand(true);

        let menu_row = create_menu_row(ui, meta, element_to_add);
        expander.set_label_widget(Some(&menu_row));

        let list_box = ListBox::new();
        list_box.set_hexpand(true);
        list_box.set_halign(Align::Fill);

        for sub_item in &element_to_add.sub_elements {
            let sub_row = create_menu_row(ui, meta, sub_item);
            sub_row.set_hexpand(true);
            sub_row.set_halign(Align::Fill);
            sub_row.set_widget_name("entry");
            list_box.append(&sub_row);
        }

        expander.set_child(Some(&list_box));
        expander.upcast()
    };

    parent.set_halign(Align::Fill);
    parent.set_valign(Align::Start);
    parent.set_hexpand(true);

    let child = FlowBoxChild::new();
    child.set_widget_name("entry");
    child.set_child(Some(&parent));
    child.set_hexpand(true);
    child.set_vexpand(false);

    ui.main_box.append(&child);
    child
}

pub(super) fn create_menu_row<T: Clone + 'static + Send>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    element_to_add: &MenuItem<T>,
) -> Widget {
    let row = ListBoxRow::new();
    row.set_focusable(true);
    row.set_hexpand(true);
    row.set_halign(Align::Fill);
    row.set_widget_name("row");

    let config = meta.config.read().unwrap();
    let row_box = gtk4::Box::new(config.row_box_orientation().into(), 0);
    row_box.set_hexpand(true);
    row_box.set_vexpand(false);
    row_box.set_halign(Align::Fill);

    row.set_child(Some(&row_box));

    let (label_img, label_text) = parse_label(&element_to_add.label);

    if config.allow_images() {
        let img = lookup_icon(
            element_to_add.icon_path.as_ref().map(AsRef::as_ref),
            &config,
        )
        .or_else(|| lookup_icon(label_img.as_ref().map(AsRef::as_ref), &config));

        if let Some(image) = img {
            image.set_widget_name("img");
            row_box.append(&image);
        }
    }

    let label = Label::new(label_text.as_ref().map(AsRef::as_ref));
    label.set_use_markup(config.allow_markup());
    label.set_natural_wrap_mode(config.line_wrap().into());
    label.set_hexpand(true);
    label.set_widget_name("text");
    label.set_wrap(true);
    if let Some(max_width_chars) = config.line_max_width_chars() {
        label.set_max_width_chars(max_width_chars);
    }

    if let Some(max_len) = config.line_max_chars()
        && let Some(text) = label_text.as_ref()
    {
        if config.allow_markup() {
            if let Some(truncated) = truncate_markup(text, max_len) {
                label.set_markup(&truncated);
            }
        } else if text.chars().count() > max_len {
            let end = text
                .char_indices()
                .nth(max_len)
                .map_or(text.len(), |(idx, _)| idx);
            label.set_text(&format!("{}...", &text[..end]));
        }
    }

    row_box.append(&label);

    if matches!(
        config.content_halign(),
        config::Align::Start | config::Align::Fill
    ) {
        label.set_xalign(0.0);
    }

    let click_ui = Rc::clone(ui);
    let click_meta = Rc::clone(meta);
    let element_clone = element_to_add.clone();

    let click = GestureClick::new();
    click.set_button(gtk4::gdk::BUTTON_PRIMARY);

    let presses = if config.single_click() { 1 } else { 2 };
    drop(config);

    click.connect_pressed(move |_gesture, n_press, _x, _y| {
        if n_press == presses
            && let Err(e) = handle_selected_item(
                &click_ui,
                &click_meta,
                None,
                Some(element_clone.clone()),
                None,
            )
        {
            log::error!("{e}");
        }
    });
    row.add_controller(click);

    row.upcast()
}

pub(super) fn lookup_icon(icon_path: Option<&str>, config: &Config) -> Option<Image> {
    static IMG_REGEX: LazyLock<Regex> = LazyLock::new(known_image_extension_regex_pattern);
    if let Some(image_path) = icon_path {
        let image = if image_path.starts_with('/') {
            Image::from_file(image_path)
        } else if IMG_REGEX.is_match(image_path) {
            if let Some(img) = freedesktop_icons::lookup(image_path)
                .with_size(config.image_size())
                .with_scale(1)
                .find()
            {
                Image::from_file(img)
            } else {
                Image::from_icon_name(image_path)
            }
        } else {
            Image::from_icon_name(image_path)
        };

        image.set_pixel_size(i32::from(config.image_size()));
        Some(image)
    } else {
        None
    }
}

pub(super) enum ChildPosition {
    Front,
    Back,
}

pub(super) fn find_visible_child<T: Clone>(
    items: &HashMap<FlowBoxChild, MenuItem<T>>,
    flow_box: &FlowBox,
    direction: &ChildPosition,
) -> Option<FlowBoxChild> {
    let range: Box<dyn Iterator<Item = usize>> = match direction {
        ChildPosition::Front => Box::new(0..items.len()),
        ChildPosition::Back => Box::new((0..items.len()).rev()),
    };

    for i in range {
        let i_32 = i.try_into().unwrap_or(i32::MAX);
        if let Some(child) = flow_box.child_at_index(i_32)
            && child.is_visible()
        {
            return Some(child);
        }
    }

    None
}

pub(super) fn select_visible_child<T: Clone>(
    items: &HashMap<FlowBoxChild, MenuItem<T>>,
    flow_box: &FlowBox,
    scroll: &ScrolledWindow,
    direction: &ChildPosition,
) {
    if let Some(child) = find_visible_child(items, flow_box, direction) {
        flow_box.select_child(&child);
        child.grab_focus();
        child.activate();

        let vadj = scroll.vadjustment();
        let new_scroll = match direction {
            ChildPosition::Front => 0.0,
            ChildPosition::Back => vadj.upper() - vadj.page_size(),
        };
        vadj.set_value(new_scroll);
    }
}

//! Window setup, styling and sizing.

use std::{cell::RefCell, rc::Rc};

use gdk4::{
    Display, Rectangle,
    gio::File,
    prelude::{DisplayExt, MonitorExt, SurfaceExt},
};
use gtk4::{
    Align, ApplicationWindow, CssProvider, FlowBox, FlowBoxChild, Label, Orientation,
    prelude::{BoxExt, FlowBoxChildExt, GtkWindowExt, NativeExt, OrientableExt, WidgetExt},
};
use gtk4_layer_shell::{Edge, KeyboardMode, LayerShell};
use log;

use super::{ChildPosition, CustomKeys, UiElements, select_visible_child};
use crate::config::{self, Config, CustomKeyHintLocation};

thread_local! {
    /// The style sheet currently added to the display, if any.
    static CSS_PROVIDER: RefCell<Option<CssProvider>> = const { RefCell::new(None) };
}

/// Loads the given style sheet, replacing the one loaded by a previous call.
/// Modes that call `show` in a loop would otherwise stack up providers.
pub(super) fn load_css(css: Option<&str>) {
    let Some(display) = Display::default() else {
        return;
    };
    CSS_PROVIDER.with_borrow_mut(|current| {
        if let Some(old) = current.take() {
            gtk4::style_context_remove_provider_for_display(&display, &old);
        }
        if let Some(css) = css {
            log::debug!("loading css from {css}");
            let provider = CssProvider::new();
            provider.load_from_file(&File::for_path(css));
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            *current = Some(provider);
        }
    });
}

pub(super) fn create_background(config: &Config) -> Option<ApplicationWindow> {
    if config.blurred_background() {
        let background = ApplicationWindow::builder()
            .decorated(false)
            .resizable(false)
            .fullscreened(config.blurred_background_fullscreen())
            .default_width(100)
            .default_height(100)
            .build();
        if config.blurred_background_layer() {
            // A layer like the main window, so the compositor animates both
            // the same way; as a toplevel it gets the window animations
            // instead. Anchored to every edge and ignoring exclusive zones,
            // it covers the whole output, bars included.
            background.init_layer_shell();
            background.set_layer(config.layer().into());
            background.set_keyboard_mode(KeyboardMode::None);
            background.set_exclusive_zone(-1);
            for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
                background.set_anchor(edge, true);
            }
        } else if !config.normal_window() {
            background.set_layer(config.layer().into());
        }
        background.set_widget_name("background");
        background.set_namespace(Some("worf"));
        background.connect_is_active_notify(move |window| {
            let Some(geometry) = get_monitor_geometry(window.surface().as_ref()) else {
                return;
            };
            window.set_height_request(geometry.height());
            window.set_width_request(geometry.width());
        });

        Some(background)
    } else {
        None
    }
}

pub(super) fn build_main_box<T: Clone + 'static>(config: &Config, ui_elements: &Rc<UiElements<T>>) {
    ui_elements.main_box.set_widget_name("inner-box");
    ui_elements.main_box.set_css_classes(&["inner-box"]);
    ui_elements.main_box.set_hexpand(true);
    ui_elements.main_box.set_vexpand(config.content_vcenter());

    ui_elements
        .main_box
        .set_selection_mode(gtk4::SelectionMode::Browse);
    ui_elements
        .main_box
        .set_max_children_per_line(config.columns());
    ui_elements.main_box.set_activate_on_single_click(true);
    ui_elements.main_box.set_halign(config.halign().into());
    ui_elements.main_box.set_valign(config.valign().into());
    if config.orientation() == config::Orientation::Horizontal {
        ui_elements.main_box.set_valign(Align::Center);
        ui_elements.main_box.set_orientation(Orientation::Vertical);
    } else {
        ui_elements.main_box.set_valign(Align::Start);
    }
    let ui_clone = Rc::clone(ui_elements);
    ui_elements.main_box.connect_map(move |fb| {
        fb.grab_focus();
        fb.invalidate_sort();

        let lock = ui_clone.menu_rows.read().unwrap();
        select_visible_child(
            &*lock,
            &ui_clone.main_box,
            &ui_clone.scroll,
            &ChildPosition::Front,
        );
    });
}

pub(super) fn build_custom_key_view(
    custom_keys: &CustomKeys,
    outer_box: &gtk4::Box,
    inner_box: &gtk4::Box,
) {
    fn create_label(inner_box: &FlowBox, text: &str, label_css: &str, box_css: &str) {
        let label_box = FlowBoxChild::new();
        label_box.set_halign(Align::Fill);
        inner_box.set_valign(Align::Start);
        label_box.set_widget_name(box_css);
        inner_box.append(&label_box);
        inner_box.set_vexpand(false);
        inner_box.set_hexpand(false);
        let label = Label::new(Some(text));
        label.set_halign(Align::Fill);
        label.set_valign(Align::Start);
        label.set_use_markup(true);
        label.set_hexpand(true);
        label.set_vexpand(false);
        label.set_widget_name(label_css);
        label.set_wrap(false);
        label.set_xalign(0.0);
        label_box.set_child(Some(&label));
    }

    inner_box.set_halign(Align::Fill);

    let hint_box = FlowBox::new();
    hint_box.set_halign(Align::Fill);
    hint_box.set_widget_name("custom-key-box");

    let custom_key_box = FlowBox::new();
    custom_key_box.set_halign(Align::Fill);
    custom_key_box.set_widget_name("custom-key-box");
    inner_box.append(&custom_key_box);

    let make_key_labels = || {
        for key in custom_keys.bindings.iter().filter(|key| key.visible) {
            create_label(
                &custom_key_box,
                key.label.as_ref(),
                "custom-key-label-text",
                "custom-key-label-box",
            );
        }
    };

    if let Some(hint) = custom_keys.hint.as_ref() {
        match hint.location {
            CustomKeyHintLocation::Top => {
                inner_box.append(&hint_box);
                create_label(
                    &hint_box,
                    &hint.label,
                    "custom-key-hint-text",
                    "custom-key-hint-box",
                );
                make_key_labels();
            } // todo this surely can be done better
            CustomKeyHintLocation::Bottom => {
                make_key_labels();
                create_label(
                    &hint_box,
                    &hint.label,
                    "custom-key-hint-text",
                    "custom-key-hint-box",
                );
                inner_box.append(&hint_box);
            }
        }
    }

    outer_box.append(inner_box);
}

pub(super) fn window_show_resize<T: Clone + 'static>(config: &Config, ui: &Rc<UiElements<T>>) {
    let Some(geometry) = get_monitor_geometry(ui.window.surface().as_ref()) else {
        return;
    };

    if !config.blurred_background_fullscreen()
        && let Some(background) = &ui.background
    {
        background.set_height_request(geometry.height());
        background.set_width_request(geometry.width());
    }

    // Calculate target width from config, return early if not set
    let Some(target_width) = percent_or_absolute(&config.width(), geometry.width()) else {
        log::error!("width is not set");
        return;
    };

    let target_height = if let Some(lines) = config.lines() {
        Some(calculate_row_height(ui, lines, config))
    } else if config.dynamic_lines() {
        Some(calculate_dynamic_lines_window_height(config, ui, geometry))
    } else if let Some(height) = percent_or_absolute(&config.height(), geometry.height()) {
        Some(height)
    } else {
        Some(0)
    };

    // Apply the calculated size or log an error if height missing
    if let Some(target_height) = target_height {
        log::debug!("Setting width {target_width}, height {target_height}");
        ui.window.set_height_request(target_height);
        ui.window.set_width_request(target_width);
    } else {
        log::error!("height is not set");
    }
}

pub(super) fn calculate_dynamic_lines_window_height<T: Clone + 'static>(
    config: &Config,
    ui: &UiElements<T>,
    geometry: Rectangle,
) -> i32 {
    if config.dynamic_lines_limit() {
        calculate_row_height(ui, visible_row_count(ui), config)
            .min(percent_or_absolute(&config.height(), geometry.height()).unwrap_or(0))
    } else {
        calculate_row_height(ui, visible_row_count(ui), config)
    }
}

pub(super) fn get_monitor_geometry(surface: Option<&gdk4::Surface>) -> Option<Rectangle> {
    surface
        .and_then(|surface| {
            let display = surface.display();
            display.monitor_at_surface(surface)
        })
        .map(|monitor| monitor.geometry())
}

pub(super) fn calculate_row_height<T: Clone + 'static>(
    ui: &UiElements<T>,
    lines: i32,
    config: &Config,
) -> i32 {
    const MEAS_SIZE: i32 = 10_000;
    let (_, _, _, height_search) = ui.search.measure(Orientation::Vertical, MEAS_SIZE);
    let (height_box, _, _, _) = ui.custom_key_box.measure(Orientation::Vertical, MEAS_SIZE);
    let (_, scroll_height, _, _) = ui.scroll.measure(Orientation::Vertical, MEAS_SIZE);
    let (_, window_height, _, _) = ui.window.measure(Orientation::Vertical, MEAS_SIZE);

    let height = {
        let lock = ui.menu_rows.read().unwrap();
        lock.iter()
            .find_map(|(fb, _)| {
                let (_, _, _, baseline) = fb.measure(Orientation::Vertical, MEAS_SIZE);
                if baseline > 0 {
                    let factor = config.lines_size_factor();

                    if config.allow_images() && baseline < i32::from(config.image_size()) {
                        // not relevant for height
                        #[allow(clippy::cast_possible_truncation)]
                        Some((f64::from(i32::from(config.image_size())) * factor) as i32)
                    } else {
                        // not relevant for height
                        #[allow(clippy::cast_possible_truncation)]
                        Some((f64::from(baseline) * factor) as i32)
                    }
                } else {
                    None
                }
            })
            .or_else(|| {
                lock.iter().find_map(|(fb, _)| {
                    let (_, nat, _, _) = fb.measure(Orientation::Vertical, MEAS_SIZE);
                    if nat > 0 { Some(nat) } else { None }
                })
            })
    };

    log::debug!(
        "heights: scroll {scroll_height}, window {window_height}, keys {height_box}, height \
         {height:?}, lines {lines:?}"
    );

    height_box
        + scroll_height
        + height_search
        + height.map_or(0, |h| h * lines)
        + config.lines_additional_space()
}

pub(super) fn visible_row_count<T: Clone + 'static>(ui: &UiElements<T>) -> i32 {
    i32::try_from(
        ui.menu_rows
            .read()
            .unwrap()
            .iter()
            .filter(|(_, menu)| menu.visible)
            .count(),
    )
    .unwrap_or(i32::MAX)
}

// allowed because truncating is fine, we do no need the precision
pub(super) fn percent_or_absolute(value: &str, base_value: i32) -> Option<i32> {
    if value.contains('%') {
        let value = value.replace('%', "").trim().to_string();
        match value.parse::<i32>() {
            // okay to truncate the value for positioning.
            #[allow(clippy::cast_possible_truncation)]
            Ok(n) => Some(((f64::from(n) / 100.0) * f64::from(base_value)) as i32),
            Err(_) => None,
        }
    } else {
        value.parse::<i32>().ok()
    }
}

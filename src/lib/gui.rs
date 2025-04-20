use std::collections::HashMap;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::anyhow;
use crossbeam::channel;
use crossbeam::channel::Sender;
use gdk4::gio::File;
use gdk4::glib::{Propagation, timeout_add_local};
use gdk4::prelude::{Cast, DisplayExt, MonitorExt};
use gdk4::{Display, Key};
use gtk4::glib::ControlFlow;
use gtk4::prelude::{
    ApplicationExt, ApplicationExtManual, BoxExt, EditableExt, FlowBoxChildExt, GestureSingleExt,
    GtkWindowExt, ListBoxRowExt, NativeExt, WidgetExt,
};
use gtk4::{
    Align, EventControllerKey, Expander, FlowBox, FlowBoxChild, GestureClick, Image, Label,
    ListBox, ListBoxRow, Ordering, PolicyType, ScrolledWindow, SearchEntry, Widget, gdk,
};
use gtk4::{Application, ApplicationWindow, CssProvider, Orientation};
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use log;

use crate::config;
use crate::config::{Animation, Config, MatchMethod};

type ArcMenuMap<T> = Arc<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>;
type MenuItemSender<T> = Sender<Result<MenuItem<T>, anyhow::Error>>;

impl From<config::Orientation> for Orientation {
    fn from(orientation: config::Orientation) -> Self {
        match orientation {
            config::Orientation::Vertical => Orientation::Vertical,
            config::Orientation::Horizontal => Orientation::Horizontal,
        }
    }
}

impl From<config::Align> for Align {
    fn from(align: config::Align) -> Self {
        match align {
            config::Align::Fill => Align::Fill,
            config::Align::Start => Align::Start,
            config::Align::Center => Align::Center,
        }
    }
}

#[derive(Clone)]
pub struct MenuItem<T> {
    pub label: String, // todo support empty label?
    pub icon_path: Option<String>,
    pub action: Option<String>,
    pub sub_elements: Vec<MenuItem<T>>,
    pub working_dir: Option<String>,
    pub initial_sort_score: i64,
    pub search_sort_score: f64,

    /// Allows to store arbitrary additional information
    pub data: Option<T>,
}

/// # Errors
///
/// Will return Err when the channel between the UI and this is broken
pub fn show<T>(config: Config, elements: Vec<MenuItem<T>>) -> Result<MenuItem<T>, anyhow::Error>
where
    T: Clone + 'static,
{
    if let Some(ref css) = config.style {
        let provider = CssProvider::new();
        let css_file_path = File::for_path(css);
        provider.load_from_file(&css_file_path);
        if let Some(display) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    let app = Application::builder().application_id("worf").build();
    let (sender, receiver) = channel::bounded(1);

    app.connect_activate(move |app| {
        build_ui(&config, &elements, &sender, app);
    });

    let gtk_args: [&str; 0] = [];
    app.run_with_args(&gtk_args);
    receiver.recv()?
}

fn build_ui<T>(
    config: &Config,
    elements: &Vec<MenuItem<T>>,
    sender: &Sender<Result<MenuItem<T>, anyhow::Error>>,
    app: &Application,
) where
    T: Clone + 'static,
{
    let window = ApplicationWindow::builder()
        .application(app)
        .decorated(false)
        .resizable(false)
        .default_width(0)
        .default_height(0)
        .build();

    window.set_widget_name("window");

    if !config.normal_window {
        // Initialize the window as a layer
        window.init_layer_shell();
        window.set_layer(gtk4_layer_shell::Layer::Overlay);
        window.set_keyboard_mode(KeyboardMode::Exclusive);
        window.set_namespace(Some("worf"));
    }

    let outer_box = gtk4::Box::new(config.orientation.unwrap().into(), 0);
    outer_box.set_widget_name("outer-box");

    let entry = SearchEntry::new();
    entry.set_widget_name("input");
    entry.set_css_classes(&["input"]);
    entry.set_placeholder_text(config.prompt.as_deref());
    entry.set_sensitive(false);
    outer_box.append(&entry);

    let scroll = ScrolledWindow::new();
    scroll.set_widget_name("scroll");
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);

    if config.hide_scroll.is_some_and(|hs| hs) {
        scroll.set_policy(PolicyType::External, PolicyType::External);
    }

    outer_box.append(&scroll);

    let inner_box = FlowBox::new();
    inner_box.set_widget_name("inner-box");
    inner_box.set_css_classes(&["inner-box"]);
    inner_box.set_hexpand(true);
    inner_box.set_vexpand(false);
    if let Some(halign) = config.halign {
        inner_box.set_halign(halign.into());
    }

    if let Some(valign) = config.valign {
        inner_box.set_valign(valign.into());
    } else if config.orientation.unwrap() == config::Orientation::Horizontal {
        inner_box.set_valign(Align::Center);
    } else {
        inner_box.set_valign(Align::Start);
    }

    inner_box.set_selection_mode(gtk4::SelectionMode::Browse);
    inner_box.set_max_children_per_line(config.columns.unwrap());
    inner_box.set_activate_on_single_click(true);

    let list_items: ArcMenuMap<T> = Arc::new(Mutex::new(HashMap::new()));
    for entry in elements {
        list_items
            .lock()
            .unwrap() // panic here ok? deadlock?
            .insert(
                add_menu_item(&inner_box, entry, config, sender, &list_items, app, &window),
                entry.clone(),
            );
    }

    let items_sort = Arc::<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>::clone(&list_items);
    inner_box.set_sort_func(move |child1, child2| sort_menu_items(child1, child2, &items_sort));

    let items_focus = Arc::<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>::clone(&list_items);
    inner_box.connect_map(move |fb| {
        fb.grab_focus();
        fb.invalidate_sort();

        let mut item_lock = items_focus.lock().unwrap();
        select_first_visible_child(item_lock.deref_mut(), fb)
    });

    let wrapper_box = gtk4::Box::new(Orientation::Vertical, 0);
    wrapper_box.append(&inner_box);
    scroll.set_child(Some(&wrapper_box));

    setup_key_event_handler(
        &window,
        entry.clone(),
        inner_box,
        app.clone(),
        sender.clone(),
        Arc::<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>::clone(&list_items),
        config.clone(),
    );

    window.set_child(Widget::NONE);
    window.show();
    animate_window_show(config.clone(), window.clone(), outer_box);
}

fn setup_key_event_handler<T: Clone + 'static>(
    window: &ApplicationWindow,
    entry_clone: SearchEntry,
    inner_box: FlowBox,
    app: Application,
    sender: MenuItemSender<T>,
    list_items: Arc<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>,
    config: Config,
) {
    let key_controller = EventControllerKey::new();

    let window_clone = window.clone();
    key_controller.connect_key_pressed(move |_, key_value, _, _| {
        match key_value {
            Key::Escape => {
                if let Err(e) = sender.send(Err(anyhow!("No item selected"))) {
                    log::error!("failed to send message {e}");
                }
                close_gui(app.clone(), window_clone.clone(), &config);
            }
            Key::Return => {
                if let Err(e) = handle_selected_item(
                    &sender,
                    app.clone(),
                    window_clone.clone(),
                    &config,
                    &inner_box,
                    &list_items,
                ) {
                    log::error!("{e}");
                }
            }
            Key::BackSpace => {
                let mut items = list_items.lock().unwrap();
                let mut query = entry_clone.text().to_string();
                query.pop();

                entry_clone.set_text(&query);
                filter_widgets(&query, &mut items, &config, &inner_box);
            }
            _ => {
                let mut items = list_items.lock().unwrap();
                if let Some(c) = key_value.to_unicode() {
                    let current = entry_clone.text().to_string();
                    let query = format!("{current}{c}");
                    entry_clone.set_text(&query);
                    filter_widgets(&query, &mut items, &config, &inner_box);
                }
            }
        }

        Propagation::Proceed
    });
    window.add_controller(key_controller);
}

fn sort_menu_items<T>(
    child1: &FlowBoxChild,
    child2: &FlowBoxChild,
    items_lock: &Mutex<HashMap<FlowBoxChild, MenuItem<T>>>,
) -> Ordering {
    let lock = items_lock.lock().unwrap();
    let m1 = lock.get(child1);
    let m2 = lock.get(child2);

    if !child1.is_visible() {
        return Ordering::Smaller;
    }
    if !child2.is_visible() {
        return Ordering::Larger;
    }

    match (m1, m2) {
        (Some(menu1), Some(menu2)) => {
            if menu1.search_sort_score != 0.0 || menu2.search_sort_score != 0.0 {
                if menu1.search_sort_score < menu2.search_sort_score {
                    Ordering::Smaller
                } else {
                    Ordering::Larger
                }
            } else if menu1.initial_sort_score < menu2.initial_sort_score {
                Ordering::Smaller
            } else {
                Ordering::Larger
            }
        }
        (Some(_), None) => Ordering::Larger,
        (None, Some(_)) => Ordering::Smaller,
        (None, None) => Ordering::Equal,
    }
}

fn animate_window_show(config: Config, window: ApplicationWindow, outer_box: gtk4::Box) {
    let display = window.display();
    if let Some(surface) = window.surface() {
        // todo this does not work for multi monitor systems
        let monitor = display.monitor_at_surface(&surface);
        if let Some(monitor) = monitor {
            let geometry = monitor.geometry();
            let Some(target_width) = percent_or_absolute(&config.width.unwrap(), geometry.width())
            else {
                return;
            };

            let Some(target_height) =
                percent_or_absolute(&config.height.unwrap(), geometry.height())
            else {
                return;
            };

            animate_window(
                window.clone(),
                config.show_animation.unwrap(),
                config.show_animation_time.unwrap(),
                target_height,
                target_width,
                move || {
                    window.set_child(Some(&outer_box));
                },
            );
        }
    }
}
fn animate_window_close<Func>(config: &Config, window: ApplicationWindow, on_done_func: Func)
where
    Func: Fn() + 'static,
{
    // todo the target size might not work for higher dpi displays or bigger resolutions
    window.set_child(Widget::NONE);

    let (target_h, target_w) = {
        if let Some(animation) = config.hide_animation {
            let allocation = window.allocation();
            match animation {
                Animation::None | Animation::Expand => (10, 10),
                Animation::ExpandVertical => (allocation.height(), 0),
                Animation::ExpandHorizontal => (0, allocation.width()),
            }
        } else {
            (0, 0)
        }
    };

    animate_window(
        window,
        config.hide_animation.unwrap(),
        config.hide_animation_time.unwrap(),
        target_h,
        target_w,
        on_done_func,
    );
}

// both warnings are disabled because
// we can deal with truncation and precission loss
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_precision_loss)]
fn animate_window<Func>(
    window: ApplicationWindow,
    animation_type: Animation,
    animation_time: u64,
    target_height: i32,
    target_width: i32,
    on_done_func: Func,
) where
    Func: Fn() + 'static,
{
    let allocation = window.allocation();

    let animation_step_length = Duration::from_millis(10); // ~60 FPS
    let animation_speed = Duration::from_millis(animation_time);

    let animation_steps =
        ((animation_speed.as_millis() / animation_step_length.as_millis()) as f32).max(1.0);

    let width = allocation.width();
    let height = allocation.height();

    // Calculate signed steps (can be negative)
    let mut width_step = ((target_width as f32 - width as f32) / animation_steps).round() as i32;
    let mut height_step = ((target_height as f32 - height as f32) / animation_steps).round() as i32;

    // Ensure we move at least 1 pixel per step in the correct direction
    if width_step == 0 && target_width != width {
        width_step = if target_width < width { -1 } else { 1 };
    }
    if height_step == 0 && target_height != height {
        height_step = if target_height < height { -1 } else { 1 };
    }

    timeout_add_local(animation_step_length, move || {
        let result = match animation_type {
            Animation::None => animation_none(&window, target_width, target_height),
            Animation::Expand => animation_expand(
                &window,
                target_width,
                target_height,
                width_step,
                height_step,
            ),
            Animation::ExpandVertical => {
                animation_expand_vertical(&window, target_width, target_height, width_step)
            }
            Animation::ExpandHorizontal => {
                animation_expand_horizontal(&window, target_width, target_height, height_step)
            }
        };

        window.queue_draw();

        if result == ControlFlow::Break {
            on_done_func();
        }
        result
    });
}

fn animation_none(
    window: &ApplicationWindow,
    target_width: i32,
    target_height: i32,
) -> ControlFlow {
    window.set_height_request(target_height);
    window.set_width_request(target_width);
    ControlFlow::Break
}

fn animation_expand(
    window: &ApplicationWindow,
    target_width: i32,
    target_height: i32,
    width_step: i32,
    height_step: i32,
) -> ControlFlow {
    let allocation = window.allocation();
    let mut done = true;
    let height = allocation.height();
    let width = allocation.width();

    if resize_height_needed(window, target_height, height_step, height) {
        window.set_height_request(height + height_step);
        done = false;
    }

    if resize_width_needed(window, target_width, width_step, width) {
        window.set_width_request(width + width_step);
        done = false;
    }

    if done {
        ControlFlow::Break
    } else {
        ControlFlow::Continue
    }
}

fn animation_expand_horizontal(
    window: &ApplicationWindow,
    target_width: i32,
    target_height: i32,
    height_step: i32,
) -> ControlFlow {
    let allocation = window.allocation();
    let height = allocation.height();
    window.set_width_request(target_width);

    if resize_height_needed(window, target_height, height_step, height) {
        window.set_height_request(height + height_step);
        ControlFlow::Continue
    } else {
        ControlFlow::Break
    }
}

fn animation_expand_vertical(
    window: &ApplicationWindow,
    target_width: i32,
    target_height: i32,
    width_step: i32,
) -> ControlFlow {
    let allocation = window.allocation();
    let width = allocation.width();
    window.set_height_request(target_height);

    if resize_width_needed(window, target_width, width_step, width) {
        window.set_width_request(allocation.width() + width_step);
        ControlFlow::Continue
    } else {
        ControlFlow::Break
    }
}

fn resize_height_needed(
    window: &ApplicationWindow,
    target_height: i32,
    height_step: i32,
    current_height: i32,
) -> bool {
    (height_step > 0 && window.height() < target_height)
        || (height_step < 0 && window.height() > target_height && current_height + height_step > 0)
}

fn resize_width_needed(
    window: &ApplicationWindow,
    target_width: i32,
    width_step: i32,
    current_width: i32,
) -> bool {
    (width_step > 0 && window.width() < target_width)
        || (width_step < 0 && window.width() > target_width && current_width + width_step > 0)
}

fn close_gui(app: Application, window: ApplicationWindow, config: &Config) {
    animate_window_close(config, window, move || app.quit());
}

fn handle_selected_item<T>(
    sender: &MenuItemSender<T>,
    app: Application,
    window: ApplicationWindow,
    config: &Config,
    inner_box: &FlowBox,
    lock_arc: &ArcMenuMap<T>,
) -> Result<(), String>
where
    T: Clone,
{
    if let Some(s) = inner_box.selected_children().into_iter().next() {
        let list_items = lock_arc.lock().unwrap();
        let item = list_items.get(&s);
        if let Some(item) = item {
            if let Err(e) = sender.send(Ok(item.clone())) {
                log::error!("failed to send message {e}");
            }
        }
        close_gui(app, window, config);
        return Ok(());
    }
    Err("selected item cannot be resolved".to_owned())
}

fn add_menu_item<T: Clone + 'static>(
    inner_box: &FlowBox,
    entry_element: &MenuItem<T>,
    config: &Config,
    sender: &MenuItemSender<T>,
    lock_arc: &ArcMenuMap<T>,
    app: &Application,
    window: &ApplicationWindow,
) -> FlowBoxChild {
    let parent: Widget = if entry_element.sub_elements.is_empty() {
        create_menu_row(
            entry_element,
            config,
            Arc::<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>::clone(lock_arc),
            sender.clone(),
            app.clone(),
            window.clone(),
            inner_box.clone(),
        )
        .upcast()
    } else {
        let expander = Expander::new(None);
        expander.set_widget_name("expander-box");
        expander.set_hexpand(true);

        // todo deduplicate this snippet
        let menu_row = create_menu_row(
            entry_element,
            config,
            Arc::<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>::clone(lock_arc),
            sender.clone(),
            app.clone(),
            window.clone(),
            inner_box.clone(),
        );
        expander.set_label_widget(Some(&menu_row));

        let list_box = ListBox::new();
        list_box.set_hexpand(true);
        list_box.set_halign(Align::Fill);

        for sub_item in &entry_element.sub_elements {
            let sub_row = create_menu_row(
                sub_item,
                config,
                Arc::<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>::clone(lock_arc),
                sender.clone(),
                app.clone(),
                window.clone(),
                inner_box.clone(),
            );
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

    inner_box.append(&child);
    child
}

fn create_menu_row<T: Clone + 'static>(
    menu_item: &MenuItem<T>,
    config: &Config,
    lock_arc: ArcMenuMap<T>,
    sender: MenuItemSender<T>,
    app: Application,
    window: ApplicationWindow,
    inner_box: FlowBox,
) -> Widget {
    let row = ListBoxRow::new();
    row.set_hexpand(true);
    row.set_halign(Align::Fill);
    row.set_widget_name("row");

    let click = GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    let config_clone = config.clone();
    click.connect_pressed(move |_gesture, n_press, _x, _y| {
        if n_press == 2 {
            if let Err(e) = handle_selected_item(
                &sender,
                app.clone(),
                window.clone(),
                &config_clone,
                &inner_box,
                &lock_arc,
            ) {
                log::error!("{e}");
            }
        }
    });

    row.add_controller(click);

    let row_box = gtk4::Box::new(config.row_bow_orientation.unwrap().into(), 0);
    row_box.set_hexpand(true);
    row_box.set_vexpand(false);
    row_box.set_halign(Align::Fill);

    row.set_child(Some(&row_box));

    if let Some(image_path) = &menu_item.icon_path {
        let image = Image::from_icon_name(image_path);
        image.set_pixel_size(
            config
                .image_size
                .unwrap_or(config::default_image_size().unwrap()),
        );
        image.set_widget_name("img");
        row_box.append(&image);
    }

    // todo make max length configurable
    let text = if config.text_wrap.is_some_and(|x| x) {
        &wrap_text(&menu_item.label, config.text_wrap_length)
    } else {
        menu_item.label.as_str()
    };

    let label = Label::new(Some(text));
    label.set_hexpand(true);
    label.set_widget_name("label");
    label.set_wrap(true);
    row_box.append(&label);

    if config.content_halign.unwrap() == config::Align::Start
        || config.content_halign.unwrap() == config::Align::Fill
    {
        label.set_xalign(0.0);
    }
    row.upcast()
}

fn filter_widgets<T>(
    query: &str,
    items: &mut HashMap<FlowBoxChild, MenuItem<T>>,
    config: &Config,
    inner_box: &FlowBox,
) {
    if items.is_empty() {
        for (child, _) in items.iter() {
            child.set_visible(true);
        }

        if let Some(child) = inner_box.first_child() {
            child.grab_focus();
            let fb = child.downcast::<FlowBoxChild>();
            if let Ok(fb) = fb {
                inner_box.select_child(&fb);
            }
        }
        return;
    }

    let query = query.to_owned().to_lowercase();
    for (flowbox_child, menu_item) in items.iter_mut() {
        let menu_item_search = format!(
            "{} {}",
            menu_item
                .action
                .as_ref()
                .map(|a| a.to_lowercase())
                .unwrap_or_default(),
            &menu_item.label.to_lowercase()
        );

        let matching = if let Some(matching) = &config.matching {
            matching
        } else {
            &config::default_match_method().unwrap()
        };

        let (search_sort_score, visible) = match matching {
            MatchMethod::Fuzzy => {
                let score = strsim::normalized_levenshtein(&query, &menu_item_search);
                (score, score > config.fuzzy_min_score.unwrap())
            }
            MatchMethod::Contains => {
                if menu_item_search.contains(&query) {
                    (1.0, true)
                } else {
                    (0.0, false)
                }
            }
            MatchMethod::MultiContains => {
                let score = query
                    .split(' ')
                    .filter(|i| menu_item_search.contains(i))
                    .map(|_| 1.0)
                    .sum();
                (score, score > 0.0)
            }
        };

        menu_item.search_sort_score = search_sort_score;
        flowbox_child.set_visible(visible);
    }

    select_first_visible_child(items, inner_box);
}

fn select_first_visible_child<T>(
    items: &mut HashMap<FlowBoxChild, MenuItem<T>>,
    inner_box: &FlowBox,
) {
    for i in 0..items.len() {
        if let Some(child) = inner_box.child_at_index(i as i32) {
            if child.is_visible() {
                inner_box.select_child(&child);
                break;
            }
        }
    }
}

// allowed because truncating is fine, we do no need the precision
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_precision_loss)]
fn percent_or_absolute(value: &str, base_value: i32) -> Option<i32> {
    if value.contains('%') {
        let value = value.replace('%', "").trim().to_string();
        match value.parse::<i32>() {
            Ok(n) => Some(((n as f32 / 100.0) * base_value as f32) as i32),
            Err(_) => None,
        }
    } else {
        value.parse::<i32>().ok()
    }
}

// highly unlikely that we are dealing with > i64 items
#[allow(clippy::cast_possible_wrap)]
pub fn initialize_sort_scores<T>(items: &mut [MenuItem<T>]) {
    let mut regular_score = items.len() as i64;
    items.sort_by(|l, r| l.label.cmp(&r.label));

    for item in items.iter_mut() {
        if item.initial_sort_score == 0 {
            item.initial_sort_score = regular_score;
            regular_score += 1;
        }
    }
}

fn wrap_text(text: &str, line_length: Option<usize>) -> String {
    let mut result = String::new();
    let mut line = String::new();
    let len = line_length.unwrap_or(text.len());

    for word in text.split_whitespace() {
        if line.len() + word.len() + 1 > len && !line.is_empty() {
            result.push_str(line.trim_end());
            result.push('\n');
            line.clear();
        }
        line.push_str(word);
        line.push(' ');
    }

    if !line.is_empty() {
        result.push_str(line.trim_end());
    }

    result
}

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
    GtkWindowExt, ListBoxRowExt, NativeExt, OrientableExt, WidgetExt,
};
use gtk4::{
    Align, EventControllerKey, Expander, FlowBox, FlowBoxChild, GestureClick, Image, Label,
    ListBox, ListBoxRow, NaturalWrapMode, Ordering, PolicyType, ScrolledWindow, SearchEntry,
    Widget, gdk, glib,
};
use gtk4::{Application, ApplicationWindow, CssProvider, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, LayerShell};
use log;
use regex::Regex;

use crate::config::{Anchor, Config, MatchMethod, WrapMode};
use crate::desktop::known_image_extension_regex_pattern;
use crate::{config, desktop};

type ArcMenuMap<T> = Arc<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>;
type ArcProvider<T> = Arc<Mutex<dyn ItemProvider<T> + Send>>;
type MenuItemSender<T> = Sender<Result<MenuItem<T>, anyhow::Error>>;

pub trait ItemProvider<T: Clone> {
    fn get_elements(&mut self, search: Option<&str>) -> (bool, Vec<MenuItem<T>>);
    fn get_sub_elements(&mut self, item: &MenuItem<T>) -> (bool, Option<Vec<MenuItem<T>>>);
}

impl From<&Anchor> for Edge {
    fn from(value: &Anchor) -> Self {
        match value {
            Anchor::Top => Edge::Top,
            Anchor::Left => Edge::Left,
            Anchor::Bottom => Edge::Bottom,
            Anchor::Right => Edge::Right,
        }
    }
}

impl From<config::Orientation> for Orientation {
    fn from(orientation: config::Orientation) -> Self {
        match orientation {
            config::Orientation::Vertical => Orientation::Vertical,
            config::Orientation::Horizontal => Orientation::Horizontal,
        }
    }
}

impl From<WrapMode> for NaturalWrapMode {
    fn from(value: WrapMode) -> Self {
        match value {
            WrapMode::None => NaturalWrapMode::None,
            WrapMode::Word => NaturalWrapMode::Word,
            WrapMode::Inherit => NaturalWrapMode::Inherit,
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

/// An entry in the list of selectable items in the UI.
/// Supports nested items but these cannot nested again (only nesting with depth == 1 is supported)
#[derive(Clone, PartialEq)]
pub struct MenuItem<T: Clone> {
    /// text to show in the UI
    pub label: String,
    /// optional icon, will use fallback icon if None is given
    pub icon_path: Option<String>,
    /// the action to run when this is selected.
    pub action: Option<String>,
    /// Sub elements of this entry. If this already has a parent entry, nesting is not supported
    pub sub_elements: Vec<MenuItem<T>>,
    /// Working directory to run the action in.
    pub working_dir: Option<String>,
    /// Initial sort score to display favourites at the top
    pub initial_sort_score: f64,

    /// Allows to store arbitrary additional information
    pub data: Option<T>,

    /// Score the item got in the current search
    search_sort_score: f64,
    /// True if the item is visible
    visible: bool,
}

impl<T: Clone> MenuItem<T> {
    #[must_use]
    pub fn new(
        label: String,
        icon_path: Option<String>,
        action: Option<String>,
        sub_elements: Vec<MenuItem<T>>,
        working_dir: Option<String>,
        initial_sort_score: f64,
        data: Option<T>,
    ) -> Self {
        MenuItem {
            label,
            icon_path,
            action,
            sub_elements,
            working_dir,
            initial_sort_score,
            data,
            search_sort_score: 0.0,
            visible: true,
        }
    }
}

impl<T: Clone> AsRef<MenuItem<T>> for MenuItem<T> {
    fn as_ref(&self) -> &MenuItem<T> {
        self
    }
}

struct MetaData<T: Clone> {
    item_provider: ArcProvider<T>,
    selected_sender: MenuItemSender<T>,
    config: Rc<Config>,
    new_on_empty: bool,
}

struct UiElements<T: Clone> {
    app: Application,
    window: ApplicationWindow,
    search: SearchEntry,
    main_box: FlowBox,
    menu_rows: ArcMenuMap<T>,
}

/// Shows the user interface and **blocks** until the user selected an entry
/// # Errors
///
/// Will return Err when the channel between the UI and this is broken
pub fn show<T, P>(
    config: Config,
    item_provider: P,
    new_on_empty: bool,
) -> Result<MenuItem<T>, anyhow::Error>
where
    T: Clone + 'static + Send,
    P: ItemProvider<T> + 'static + Clone + Send,
{
    gtk4::init()?;
    log::debug!("Starting GUI");
    if let Some(ref css) = config.style() {
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
        build_ui(
            &config,
            item_provider.clone(),
            sender.clone(),
            app.clone(),
            new_on_empty,
        );
    });

    let gtk_args: [&str; 0] = [];
    app.run_with_args(&gtk_args);
    receiver.recv()?
}

fn build_ui<T, P>(
    config: &Config,
    item_provider: P,
    sender: Sender<Result<MenuItem<T>, anyhow::Error>>,
    app: Application,
    new_on_empty: bool,
) where
    T: Clone + 'static + Send,
    P: ItemProvider<T> + 'static + Send,
{
    let start = Instant::now();

    let meta = Rc::new(MetaData {
        item_provider: Arc::new(Mutex::new(item_provider)),
        selected_sender: sender,
        config: Rc::new(config.clone()),
        new_on_empty,
    });

    let provider_clone = Arc::clone(&meta.item_provider);
    let get_provider_elements = thread::spawn(move || {
        log::debug!("getting items");
        provider_clone.lock().unwrap().get_elements(None)
    });

    let window = ApplicationWindow::builder()
        .application(&app)
        .decorated(false)
        .resizable(false)
        .default_width(100)
        .default_height(100)
        .build();

    let ui_elements = Rc::new(UiElements {
        app,
        window,
        search: SearchEntry::new(),
        main_box: FlowBox::new(),
        menu_rows: Arc::new(Mutex::new(HashMap::new())),
    });

    // handle keys as soon as possible
    setup_key_event_handler(&ui_elements, &meta);

    log::debug!("keyboard ready after {:?}", start.elapsed());

    ui_elements.window.set_widget_name("window");

    if !config.normal_window() {
        // Initialize the window as a layer
        ui_elements.window.init_layer_shell();
        ui_elements
            .window
            .set_layer(gtk4_layer_shell::Layer::Overlay);
        ui_elements
            .window
            .set_keyboard_mode(KeyboardMode::Exclusive);
        ui_elements.window.set_namespace(Some("worf"));
    }

    if let Some(location) = config.location() {
        for anchor in location {
            ui_elements.window.set_anchor(anchor.into(), true);
        }
    }

    let outer_box = gtk4::Box::new(config.orientation().into(), 0);
    outer_box.set_widget_name("outer-box");
    outer_box.append(&ui_elements.search);
    ui_elements.window.set_child(Some(&outer_box));

    let scroll = ScrolledWindow::new();
    scroll.set_widget_name("scroll");
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);

    if config.hide_scroll() {
        scroll.set_policy(PolicyType::External, PolicyType::External);
    }
    outer_box.append(&scroll);

    build_main_box(config, &ui_elements);
    build_search_entry(config, &ui_elements);

    let wrapper_box = gtk4::Box::new(Orientation::Vertical, 0);
    wrapper_box.append(&ui_elements.main_box);
    scroll.set_child(Some(&wrapper_box));

    let wait_for_items = Instant::now();
    let (_changed, provider_elements) = get_provider_elements.join().unwrap();
    log::debug!("got items after {:?}", wait_for_items.elapsed());
    build_ui_from_menu_items(&ui_elements, &meta, provider_elements);

    let window_start = Instant::now();
    ui_elements.window.show();
    log::debug!("window show took {:?}", window_start.elapsed());

    animate_window_show(config, ui_elements.window.clone());
    log::debug!("Building UI took {:?}", start.elapsed(),);
}

fn build_main_box<T: Clone + 'static>(config: &Config, ui_elements: &Rc<UiElements<T>>) {
    ui_elements.main_box.set_widget_name("inner-box");
    ui_elements.main_box.set_css_classes(&["inner-box"]);
    ui_elements.main_box.set_hexpand(true);
    ui_elements.main_box.set_vexpand(false);

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

        let lock = ui_clone.menu_rows.lock().unwrap();
        select_first_visible_child(&*lock, &ui_clone.main_box);
    });
}

fn build_search_entry<T: Clone>(config: &Config, ui_elements: &UiElements<T>) {
    ui_elements.search.set_widget_name("input");
    ui_elements.search.set_css_classes(&["input"]);
    ui_elements
        .search
        .set_placeholder_text(Some(config.prompt().as_ref()));
    ui_elements.search.set_can_focus(false);
    if config.hide_search() {
        ui_elements.search.set_visible(false);
    }
}

fn build_ui_from_menu_items<T: Clone + 'static>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    mut items: Vec<MenuItem<T>>,
) {
    let start = Instant::now();
    {
        while let Some(b) = ui.main_box.child_at_index(0) {
            ui.main_box.remove(&b);
            drop(b);
        }
        ui.menu_rows.lock().unwrap().clear();

        let meta_clone = Rc::<MetaData<T>>::clone(meta);
        let ui_clone = Rc::<UiElements<T>>::clone(ui);

        glib::idle_add_local(move || {
            ui_clone.main_box.unset_sort_func();
            let mut done = false;
            {
                let mut lock = ui_clone.menu_rows.lock().unwrap();

                for _ in 0..100 {
                    if let Some(item) = items.pop() {
                        lock.insert(add_menu_item(&ui_clone, &meta_clone, &item), item);
                    } else {
                        done = true;
                    }
                }

                let query = ui_clone.search.text();
                let menus = &mut *lock;
                set_menu_visibility_for_search(&query, menus, &meta_clone.config);
            }

            let items_sort = ArcMenuMap::clone(&ui_clone.menu_rows);
            ui_clone.main_box.set_sort_func(move |child1, child2| {
                sort_menu_items_by_score(child1, child2, &items_sort)
            });

            if done {
                let mut lock = ui_clone.menu_rows.lock().unwrap();
                let menus = &mut *lock;
                select_first_visible_child(menus, &ui_clone.main_box);

                log::debug!("Created menu items in {:?}", start.elapsed());
                ControlFlow::Break
            } else {
                ControlFlow::Continue
            }
        });
    }
}

fn setup_key_event_handler<T: Clone + 'static + Send>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
) {
    let key_controller = EventControllerKey::new();

    let ui_clone = Rc::clone(ui);
    let meta_clone = Rc::clone(meta);
    key_controller.connect_key_pressed(move |_, key_value, _, _| {
        handle_key_press(&ui_clone, &meta_clone, key_value)
    });

    ui.window.add_controller(key_controller);
}

#[allow(clippy::too_many_arguments)] // todo refactor this?
fn handle_key_press<T: Clone + 'static>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    keyboard_key: Key,
) -> Propagation {
    let update_view = |query: &String| {
        let mut lock = ui.menu_rows.lock().unwrap();
        let menus = &mut *lock;
        set_menu_visibility_for_search(query, menus, &meta.config);
        select_first_visible_child(&*lock, &ui.main_box);
    };

    let update_view_from_provider = |query: &String| {
        let (changed, filtered_list) = meta.item_provider.lock().unwrap().get_elements(Some(query));
        if changed {
            build_ui_from_menu_items(ui, meta, filtered_list);
        }

        update_view(query);
    };

    match keyboard_key {
        Key::Escape => {
            if let Err(e) = meta.selected_sender.send(Err(anyhow!("No item selected"))) {
                log::error!("failed to send message {e}");
            }
            close_gui(ui.app.clone(), ui.window.clone(), &meta.config);
        }
        Key::Return => {
            let query = ui.search.text().to_string();
            if let Err(e) = handle_selected_item(ui, meta, Some(&query), None, meta.new_on_empty) {
                log::error!("{e}");
            }
        }
        Key::BackSpace => {
            let mut query = ui.search.text().to_string();
            if !query.is_empty() {
                query.pop();

                ui.search.set_text(&query);
                update_view_from_provider(&query);
            }
        }
        Key::Tab => {
            if let Some(fb) = ui.main_box.selected_children().first() {
                if let Some(child) = fb.child() {
                    let expander = child.downcast::<Expander>().ok();
                    if let Some(expander) = expander {
                        expander.set_expanded(true);
                    } else {
                        let opt_changed = {
                            let lock = ui.menu_rows.lock().unwrap();
                            let menu_item = lock.get(fb);
                            menu_item.map(|menu_item| {
                                (
                                    meta.item_provider
                                        .lock()
                                        .unwrap()
                                        .get_sub_elements(menu_item),
                                    menu_item.label.clone(),
                                )
                            })
                        };

                        if let Some(changed) = opt_changed {
                            let items = changed.0.1.unwrap_or_default();
                            if changed.0.0 {
                                build_ui_from_menu_items(ui, meta, items);
                            }

                            let query = changed.1;
                            ui.search.set_text(&query);
                            update_view(&query);
                        }
                    }
                }
            }
            return Propagation::Stop;
        }
        _ => {
            if let Some(c) = keyboard_key.to_unicode() {
                let current = ui.search.text().to_string();
                let query = format!("{current}{c}");
                ui.search.set_text(&query);
                update_view_from_provider(&query);
            }
        }
    }

    Propagation::Proceed
}

fn sort_menu_items_by_score<T: Clone>(
    child1: &FlowBoxChild,
    child2: &FlowBoxChild,
    items_lock: &ArcMenuMap<T>,
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
            fn compare(a: f64, b: f64) -> Ordering {
                if a > b {
                    Ordering::Smaller
                } else if a < b {
                    Ordering::Larger
                } else {
                    Ordering::Equal
                }
            }

            if menu1.search_sort_score > 0.0 || menu2.search_sort_score > 0.0 {
                compare(menu1.search_sort_score, menu2.search_sort_score)
            } else {
                compare(menu1.initial_sort_score, menu2.initial_sort_score)
            }
        }
        (Some(_), None) => Ordering::Larger,
        (None, Some(_)) => Ordering::Smaller,
        (None, None) => Ordering::Equal,
    }
}

fn animate_window_show(config: &Config, window: ApplicationWindow) {
    if let Some(surface) = window.surface() {
        let display = window.display();

        // todo this does not work for multi monitor systems
        let monitor = display.monitor_at_surface(&surface);
        if let Some(monitor) = monitor {
            let geometry = monitor.geometry();
            let Some(target_width) = percent_or_absolute(&config.width(), geometry.width()) else {
                return;
            };

            let Some(target_height) = percent_or_absolute(&config.height(), geometry.height())
            else {
                return;
            };

            log::debug!(
                "monitor geometry: {geometry:?}, target_height {target_height}, target_width {target_width}"
            );

            let animation_start = Instant::now();
            animate_window(
                window,
                config.show_animation_time(),
                target_height,
                target_width,
                move || {
                    log::debug!("animation done after {:?}", animation_start.elapsed());
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

    animate_window(window, config.hide_animation_time(), 10, 10, on_done_func);
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.7 {
        10.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3)
    }
}

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_precision_loss)]
fn animate_window<Func>(
    window: ApplicationWindow,
    animation_time: u64,
    target_height: i32,
    target_width: i32,
    on_done_func: Func,
) where
    Func: Fn() + 'static,
{
    if animation_time == 0 {
        window.set_width_request(target_width);
        window.set_height_request(target_height);
        on_done_func();
        return;
    }

    let allocation = window.allocation();

    // Define animation parameters
    let animation_step_length = Duration::from_millis(20);

    // Start positions (initial window dimensions)
    let start_width = allocation.width() as f32;
    let start_height = allocation.height() as f32;

    // Calculate the change in width and height
    let delta_width = target_width as f32 - start_width;
    let delta_height = target_height as f32 - start_height;

    // Animation time starts when the timeout is ran for the first time
    let mut start_time: Option<Instant> = None;

    let mut last_t = 0.0;

    let before_animation = Instant::now();
    timeout_add_local(animation_step_length, move || {
        if !window.is_visible() {
            return ControlFlow::Continue;
        }
        let start_time = start_time.unwrap_or_else(|| {
            let now = Instant::now();
            start_time = Some(now);
            log::debug!("animation started after {:?}", before_animation.elapsed());
            now
        });

        let elapsed_us = start_time.elapsed().as_micros() as f32;
        let t = (elapsed_us / (animation_time * 1000) as f32).min(1.0);

        // Skip if there's no meaningful change in progress
        if (t - last_t).abs() < 0.001 && t < 1.0 {
            return ControlFlow::Continue;
        }
        last_t = t;

        let eased_t = ease_in_out_cubic(t);

        let current_width = start_width + delta_width * eased_t;
        let current_height = start_height + delta_height * eased_t;

        let rounded_width = current_width.round() as i32;
        let rounded_height = current_height.round() as i32;

        if t >= 1.0 || rounded_height > target_height || rounded_width > target_width {
            window.set_width_request(target_width);
            window.set_height_request(target_height);
            on_done_func();
            ControlFlow::Break
        } else {
            window.set_width_request(rounded_width);
            window.set_height_request(rounded_height);
            ControlFlow::Continue
        }
    });
}

fn close_gui(app: Application, window: ApplicationWindow, config: &Config) {
    animate_window_close(config, window, move || app.quit());
}

fn handle_selected_item<T>(
    ui: &UiElements<T>,
    meta: &MetaData<T>,
    query: Option<&str>,
    item: Option<MenuItem<T>>,
    new_on_empty: bool,
) -> Result<(), String>
where
    T: Clone,
{
    if let Some(selected_item) = item {
        if let Err(e) = meta.selected_sender.send(Ok(selected_item.clone())) {
            log::error!("failed to send message {e}");
        }

        close_gui(ui.app.clone(), ui.window.clone(), &meta.config);
        return Ok(());
    } else if let Some(s) = ui.main_box.selected_children().into_iter().next() {
        let list_items = ui.menu_rows.lock().unwrap();
        let item = list_items.get(&s);
        if let Some(item) = item {
            if let Err(e) = meta.selected_sender.send(Ok(item.clone())) {
                log::error!("failed to send message {e}");
            }
            close_gui(ui.app.clone(), ui.window.clone(), &meta.config);
            return Ok(());
        }
    }

    if new_on_empty {
        let item = MenuItem {
            label: query.unwrap_or("").to_owned(),
            icon_path: None,
            action: None,
            sub_elements: Vec::new(),
            working_dir: None,
            initial_sort_score: 0.0,
            search_sort_score: 0.0,
            data: None,
            visible: true,
        };

        if let Err(e) = meta.selected_sender.send(Ok(item.clone())) {
            log::error!("failed to send message {e}");
        }
        close_gui(ui.app.clone(), ui.window.clone(), &meta.config);
        Ok(())
    } else {
        Err("selected item cannot be resolved".to_owned())
    }
}

fn add_menu_item<T: Clone + 'static>(
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

fn create_menu_row<T: Clone + 'static>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    element_to_add: &MenuItem<T>,
) -> Widget {
    let row = ListBoxRow::new();
    row.set_focusable(true);
    row.set_hexpand(true);
    row.set_halign(Align::Fill);
    row.set_widget_name("row");

    let row_box = gtk4::Box::new(meta.config.row_bow_orientation().into(), 0);
    row_box.set_hexpand(true);
    row_box.set_vexpand(false);
    row_box.set_halign(Align::Fill);

    row.set_child(Some(&row_box));
    if meta.config.allow_images() {
        if let Some(image) = lookup_icon(element_to_add, &meta.config) {
            image.set_widget_name("img");
            row_box.append(&image);
        }
    }

    let label = Label::new(Some(element_to_add.label.as_str()));

    label.set_natural_wrap_mode(meta.config.line_wrap().into());
    label.set_hexpand(true);
    label.set_widget_name("text");
    label.set_wrap(true);
    row_box.append(&label);

    if meta.config.content_halign().eq(&config::Align::Start)
        || meta.config.content_halign().eq(&config::Align::Fill)
    {
        label.set_xalign(0.0);
    }

    let click_ui = Rc::clone(ui);
    let click_meta = Rc::clone(meta);
    let element_clone = element_to_add.clone();

    let click = GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    click.connect_pressed(move |_gesture, n_press, _x, _y| {
        if n_press == 2 {
            if let Err(e) = handle_selected_item(
                click_ui.as_ref(),
                click_meta.as_ref(),
                None,
                Some(element_clone.clone()),
                false,
            ) {
                log::error!("{e}");
            }
        }
    });
    row.add_controller(click);

    row.upcast()
}

fn lookup_icon<T: Clone>(menu_item: &MenuItem<T>, config: &Config) -> Option<Image> {
    if let Some(image_path) = &menu_item.icon_path {
        let img_regex = Regex::new(&format!(
            r"((?i).*{})",
            known_image_extension_regex_pattern()
        ));
        let image = if image_path.starts_with('/') {
            Image::from_file(image_path)
        } else if img_regex.unwrap().is_match(image_path) {
            if let Ok(img) = desktop::fetch_icon_from_common_dirs(image_path) {
                Image::from_file(img)
            } else {
                Image::from_icon_name(image_path)
            }
        } else {
            Image::from_icon_name(image_path)
        };

        image.set_pixel_size(config.image_size());
        Some(image)
    } else {
        None
    }
}

fn set_menu_visibility_for_search<T: Clone>(
    query: &str,
    items: &mut HashMap<FlowBoxChild, MenuItem<T>>,
    config: &Config,
) {
    {
        if query.is_empty() {
            for (fb, menu_item) in items.iter_mut() {
                menu_item.search_sort_score = 0.0;
                menu_item.visible = true;
                fb.set_visible(menu_item.visible);
            }
        } else {
            let query = if config.insensitive() {
                query.to_owned().to_lowercase()
            } else {
                query.to_owned()
            };
            for (fb, menu_item) in items.iter_mut() {
                let menu_item_search = format!(
                    "{} {}",
                    menu_item
                        .action
                        .as_ref()
                        .map(|a| a.to_lowercase())
                        .unwrap_or_default(),
                    &menu_item.label.to_lowercase()
                );

                let (search_sort_score, visible) = match config.match_method() {
                    MatchMethod::Fuzzy => {
                        let mut score = strsim::jaro_winkler(&query, &menu_item_search);
                        if score == 0.0 {
                            score = -1.0;
                        }

                        (score, score > config.fuzzy_min_score() && score > 0.0)
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

                menu_item.search_sort_score = search_sort_score + menu_item.initial_sort_score;
                menu_item.visible = visible;
                fb.set_visible(menu_item.visible);
            }
        }
    }
}

fn select_first_visible_child<T: Clone>(
    items: &HashMap<FlowBoxChild, MenuItem<T>>,
    flow_box: &FlowBox,
) {
    for i in 0..items.len() {
        let i_32 = i.try_into().unwrap_or(i32::MAX);
        if let Some(child) = flow_box.child_at_index(i_32) {
            if child.is_visible() {
                flow_box.select_child(&child);
                child.grab_focus();
                child.activate();
                return;
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

/// Sorts menu items in alphabetical order, while maintaining the initial score
// highly unlikely that we are dealing with > i64 items
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_precision_loss)]
pub fn sort_menu_items_alphabetically_honor_initial_score<T: Clone>(items: &mut [MenuItem<T>]) {
    let special_score = items.len() as f64;
    let mut regular_score = 0.0;
    items.sort_by(|l, r| r.label.cmp(&l.label));

    for item in items.iter_mut() {
        if item.initial_sort_score == 0.0 {
            item.initial_sort_score += regular_score;
            regular_score += 1.0;
        } else {
            item.initial_sort_score += special_score;
        }
    }
}

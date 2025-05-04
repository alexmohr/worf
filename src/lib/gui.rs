use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crossbeam::channel;
use crossbeam::channel::Sender;
use gdk4::gio::File;
use gdk4::glib::{Propagation, timeout_add_local};
use gdk4::prelude::{Cast, DisplayExt, MonitorExt, SurfaceExt};
use gdk4::{Display, Key, ModifierType};
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

use crate::config::{Anchor, Config, MatchMethod, SortOrder, WrapMode};
use crate::desktop::known_image_extension_regex_pattern;
use crate::{Error, config, desktop};

type ArcMenuMap<T> = Arc<Mutex<HashMap<FlowBoxChild, MenuItem<T>>>>;
type ArcProvider<T> = Arc<Mutex<dyn ItemProvider<T> + Send>>;

pub struct Selection<T: Clone + Send> {
    pub menu: MenuItem<T>,
    pub custom_key: Option<CustomKey>,
}
type SelectionSender<T> = Sender<Result<Selection<T>, Error>>;

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

#[derive(Clone, PartialEq)]
pub struct CustomKey {
    pub key: Key,
    pub modifiers: ModifierType, // acts as a mask, so multiple things can be set.
    pub label: String,
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

struct MetaData<T: Clone + Send> {
    item_provider: ArcProvider<T>,
    selected_sender: SelectionSender<T>,
    config: Rc<Config>,
    new_on_empty: bool,
    search_ignored_words: Option<Vec<Regex>>,
}

struct UiElements<T: Clone> {
    app: Application,
    window: ApplicationWindow,
    search: SearchEntry,
    main_box: FlowBox,
    menu_rows: ArcMenuMap<T>,
    search_text: Arc<Mutex<String>>,
}

/// Shows the user interface and **blocks** until the user selected an entry
/// # Errors
///
/// Will return Err when the channel between the UI and this is broken
pub fn show<T, P>(
    config: Config,
    item_provider: P,
    new_on_empty: bool,
    search_ignored_words: Option<Vec<Regex>>,
    custom_keys: Option<Vec<CustomKey>>,
) -> Result<Selection<T>, Error>
where
    T: Clone + 'static + Send,
    P: ItemProvider<T> + 'static + Clone + Send,
{
    gtk4::init().map_err(|e| Error::Graphics(e.to_string()))?;
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
            search_ignored_words.clone(),
            custom_keys.clone(),
        );
    });

    let gtk_args: [&str; 0] = [];
    app.run_with_args(&gtk_args);
    receiver.recv().map_err(|e| Error::Io(e.to_string()))?
}

fn build_ui<T, P>(
    config: &Config,
    item_provider: P,
    sender: Sender<Result<Selection<T>, Error>>,
    app: Application,
    new_on_empty: bool,
    search_ignored_words: Option<Vec<Regex>>,
    custom_keys: Option<Vec<CustomKey>>,
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
        search_ignored_words,
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
        search_text: Arc::new(Mutex::new(String::new())),
    });

    // handle keys as soon as possible
    setup_key_event_handler(&ui_elements, &meta, &custom_keys);

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
    build_custom_key_view(config, &ui_elements, &custom_keys, &outer_box);

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
    build_search_entry(config, &ui_elements, &meta);

    let wrapper_box = gtk4::Box::new(Orientation::Vertical, 0);
    wrapper_box.append(&ui_elements.main_box);
    scroll.set_child(Some(&wrapper_box));

    let wait_for_items = Instant::now();
    let (_changed, provider_elements) = get_provider_elements.join().unwrap();
    log::debug!("got items after {:?}", wait_for_items.elapsed());
    build_ui_from_menu_items(&ui_elements, &meta, provider_elements);

    let animate_cfg = config.clone();
    let animate_window = ui_elements.window.clone();
    timeout_add_local(Duration::from_millis(5), move || {
        if !animate_window.is_active() {
            return ControlFlow::Continue;
        }
        animate_window.set_opacity(1.0);
        window_show_resize(&animate_cfg.clone(), &animate_window);
        ControlFlow::Break
    });

    // hide the fact that we are starting with a small window
    ui_elements.window.set_opacity(0.01);
    let window_start = Instant::now();
    ui_elements.window.present();
    log::debug!("window show took {:?}", window_start.elapsed());

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

fn build_search_entry<T: Clone + Send>(
    config: &Config,
    ui_elements: &UiElements<T>,
    meta: &MetaData<T>,
) {
    ui_elements.search.set_widget_name("input");
    ui_elements.search.set_css_classes(&["input"]);
    ui_elements
        .search
        .set_placeholder_text(Some(config.prompt().as_ref()));
    ui_elements.search.set_can_focus(false);
    if config.hide_search() {
        ui_elements.search.set_visible(false);
    }
    if let Some(search) = config.search() {
        set_search_text(&search, ui_elements, meta);
    }
}

fn build_custom_key_view<T>(
    config: &Config,
    ui: &Rc<UiElements<T>>,
    custom_keys: &Option<Vec<CustomKey>>,
    outer_box: &gtk4::Box,
) where
    T: 'static + Clone + Send,
{
    let inner_box = gtk4::Box::new(Orientation::Horizontal, 0);
    inner_box.set_halign(Align::Start);
    inner_box.set_widget_name("custom-key-box");
    if let Some(custom_keys) = custom_keys {
        for key in custom_keys {
            let label_box = gtk4::Box::new(Orientation::Horizontal, 0);
            label_box.set_halign(Align::Start);
            label_box.set_widget_name("custom-key-label-box");
            inner_box.append(&label_box);
            let label = Label::new(Some(&key.label));
            label.set_use_markup(true);
            label.set_hexpand(true);
            label.set_widget_name("custom-key-label-text");
            label.set_wrap(true);
            label_box.append(&label);
        }
    }
    outer_box.append(&inner_box);
}

fn set_search_text<T: Clone + Send>(text: &str, ui: &UiElements<T>, meta: &MetaData<T>) {
    let mut lock = ui.search_text.lock().unwrap();
    text.clone_into(&mut lock);
    if let Some(pw) = meta.config.password() {
        let mut ui_text = String::new();
        for _ in 0..text.len() {
            ui_text += &pw;
        }
        ui.search.set_text(&ui_text);
    } else {
        ui.search.set_text(text);
    }
}

fn build_ui_from_menu_items<T: Clone + 'static + Send>(
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

                let search_lock = ui_clone.search_text.lock().unwrap();
                let menus = &mut *lock;
                set_menu_visibility_for_search(
                    &search_lock,
                    menus,
                    &meta_clone.config,
                    meta_clone.search_ignored_words.as_ref(),
                );
            }

            let items_sort = ArcMenuMap::clone(&ui_clone.menu_rows);
            ui_clone.main_box.set_sort_func(move |child1, child2| {
                sort_menu_items_by_score(child1, child2, &items_sort)
            });

            if done {
                let mut lock = ui_clone.menu_rows.lock().unwrap();
                let menus = &mut *lock;
                select_first_visible_child(menus, &ui_clone.main_box);

                log::debug!(
                    "Created {} menu items in {:?}",
                    menus.len(),
                    start.elapsed()
                );
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
    custom_keys: &Option<Vec<CustomKey>>,
) {
    let key_controller = EventControllerKey::new();

    let ui_clone = Rc::clone(ui);
    let meta_clone = Rc::clone(meta);
    let keys_clone = custom_keys.clone();
    key_controller.connect_key_pressed(move |_, key_value, _, modifier| {
        handle_key_press(
            &ui_clone,
            &meta_clone,
            key_value,
            modifier,
            keys_clone.as_ref(),
        )
    });

    ui.window.add_controller(key_controller);
}
fn handle_key_press<T: Clone + 'static + Send>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    keyboard_key: Key,
    modifier_type: ModifierType,
    custom_keys: Option<&Vec<CustomKey>>,
) -> Propagation {
    let update_view = |query: &String| {
        let mut lock = ui.menu_rows.lock().unwrap();
        let menus = &mut *lock;
        set_menu_visibility_for_search(
            query,
            menus,
            &meta.config,
            meta.search_ignored_words.as_ref(),
        );
        select_first_visible_child(&*lock, &ui.main_box);
    };

    let update_view_from_provider = |query: &String| {
        let (changed, filtered_list) = meta.item_provider.lock().unwrap().get_elements(Some(query));
        if changed {
            build_ui_from_menu_items(ui, meta, filtered_list);
        }

        update_view(query);
    };

    if let Some(custom_keys) = custom_keys {
        for custom_key in custom_keys {
            if custom_key.key == keyboard_key && custom_key.modifiers == modifier_type {
                let search_lock = ui.search_text.lock().unwrap();
                if let Err(e) = handle_selected_item(
                    ui,
                    meta,
                    Some(&search_lock),
                    None,
                    meta.new_on_empty,
                    Some(&custom_key),
                ) {
                    log::error!("{e}");
                }
            }
        }
    }

    match keyboard_key {
        Key::Escape => {
            if let Err(e) = meta.selected_sender.send(Err(Error::NoSelection)) {
                log::error!("failed to send message {e}");
            }
            close_gui(&ui.app);
        }
        Key::Return => {
            let search_lock = ui.search_text.lock().unwrap();
            if let Err(e) =
                handle_selected_item(ui, meta, Some(&search_lock), None, meta.new_on_empty, None)
            {
                log::error!("{e}");
            }
        }
        Key::BackSpace => {
            let mut query = ui.search_text.lock().unwrap().to_string();
            if !query.is_empty() {
                query.pop();

                set_search_text(&query, ui, meta);
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
                            set_search_text(&query, ui, meta);
                            update_view(&query);
                        }
                    }
                }
            }
            return Propagation::Stop;
        }
        _ => {
            if let Some(c) = keyboard_key.to_unicode() {
                let query = format!("{}{c}", ui.search_text.lock().unwrap());
                set_search_text(&query, ui, meta);
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

fn window_show_resize(config: &Config, window: &ApplicationWindow) {
    if let Some(surface) = window.surface() {
        let display = surface.display();
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

            window.set_width_request(target_width);
            window.set_height_request(target_height);
        }
    }
}

fn close_gui(app: &Application) {
    app.quit();
}

fn handle_selected_item<T>(
    ui: &UiElements<T>,
    meta: &MetaData<T>,
    query: Option<&str>,
    item: Option<MenuItem<T>>,
    new_on_empty: bool,
    custom_key: Option<&CustomKey>,
) -> Result<(), String>
where
    T: Clone + Send,
{
    if let Some(selected_item) = item {
        close_gui(ui.app.clone(), ui.window.clone(), &meta.config);
        if let Err(e) = meta.selected_sender.send(Ok(Selection {
            menu: selected_item.clone(),
            custom_key: custom_key.map(|k| k.clone()),
        })) {
            log::error!("failed to send message {e}");
        }

        close_gui(&ui.app);
        return Ok(());
    } else if let Some(s) = ui.main_box.selected_children().into_iter().next() {
        let list_items = ui.menu_rows.lock().unwrap();
        let item = list_items.get(&s);
        if let Some(item) = item {
            if let Err(e) = meta.selected_sender.send(Ok(item.clone())) {
                log::error!("failed to send message {e}");
            }
            close_gui(&ui.app);
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

        if let Err(e) = meta.selected_sender.send(Ok(Selection {
            menu: item.clone(),
            custom_key: custom_key.map(|k| k.clone()),
        })) {
            log::error!("failed to send message {e}");
        }
        close_gui(&ui.app);
        Ok(())
    } else {
        Err("selected item cannot be resolved".to_owned())
    }
}

fn add_menu_item<T: Clone + 'static + Send>(
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

fn create_menu_row<T: Clone + 'static + Send>(
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

    let (label_img, label_text) = parse_label(&element_to_add.label);

    if meta.config.allow_images() {
        let img = lookup_icon(
            element_to_add.icon_path.as_ref().map(AsRef::as_ref),
            &meta.config,
        )
        .or(lookup_icon(
            label_img.as_ref().map(AsRef::as_ref),
            &meta.config,
        ));

        if let Some(image) = img {
            image.set_widget_name("img");
            row_box.append(&image);
        }
    }

    let label = Label::new(label_text.as_ref().map(AsRef::as_ref));
    label.set_use_markup(meta.config.allow_markup());
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
                None,
            ) {
                log::error!("{e}");
            }
        }
    });
    row.add_controller(click);

    row.upcast()
}
fn parse_label(label: &str) -> (Option<String>, Option<String>) {
    let mut img = None;
    let mut text = None;

    let parts: Vec<&str> = label.split(':').collect();
    let mut i = 0;

    while i < parts.len() {
        match parts.get(i) {
            Some(&"img") => {
                if i + 1 < parts.len() {
                    img = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            Some(&"text") => {
                i += 1;
                let mut text_parts = Vec::new();
                while i < parts.len() && parts[i] != "img" && parts[i] != "text" {
                    text_parts.push(parts[i]);
                    i += 1;
                }
                text = Some(text_parts.join(":").trim().to_string());
            }
            other => {
                // Treat as fallback text if no text tag is present
                if text.is_none() {
                    text = Some((*other.unwrap_or(&"")).to_string());
                } else {
                    text = Some(text.unwrap() + ":" + (*other.unwrap_or(&"")));
                }
                i += 1;
            }
        }
    }

    (img, text)
}

fn lookup_icon(icon_path: Option<&str>, config: &Config) -> Option<Image> {
    if let Some(image_path) = icon_path {
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
    search_ignored_words: Option<&Vec<Regex>>,
) {
    {
        if query.is_empty() {
            for (fb, menu_item) in items.iter_mut() {
                menu_item.search_sort_score = 0.0;
                menu_item.visible = true;
                fb.set_visible(menu_item.visible);
            }
            return;
        }

        let mut query = if config.insensitive() {
            query.to_owned().to_lowercase()
        } else {
            query.to_owned()
        };

        if let Some(s) = search_ignored_words.as_ref() {
            s.iter().for_each(|rgx| {
                query = rgx.replace_all(&query, "").to_string();
            });
        }

        for (fb, menu_item) in items.iter_mut() {
            let menu_item_search = format!(
                "{} {}",
                menu_item
                    .action
                    .as_ref()
                    .map(|a| {
                        if config.insensitive() {
                            a.to_lowercase()
                        } else {
                            a.clone()
                        }
                    })
                    .unwrap_or_default(),
                if config.insensitive() {
                    menu_item.label.to_lowercase()
                } else {
                    menu_item.label.clone()
                }
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
pub fn apply_sort<T: Clone>(items: &mut [MenuItem<T>], order: &SortOrder) {
    match order {
        SortOrder::Default => {}
        SortOrder::Alphabetical => {
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
    }
}

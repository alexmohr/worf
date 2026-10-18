use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering as AtomicOrdering},
    },
    thread,
    time::Instant,
};

use crossbeam::channel::{self, Sender};
use gdk4::{
    gio::{self},
    glib::{MainContext, SignalHandlerId},
    prelude::{Cast, ObjectExt},
};
use gtk4::{
    Align, Application, ApplicationWindow, CssProvider, Expander, FlowBox, FlowBoxChild, ListBox,
    NaturalWrapMode, Orientation, PolicyType, ScrolledWindow, SearchEntry,
    prelude::{
        ApplicationExt, ApplicationExtManual, BoxExt, EditableExt, FlowBoxChildExt, GtkWindowExt,
        ListBoxRowExt, NativeExt, WidgetExt,
    },
};
use gtk4_layer_shell::{Edge, KeyboardMode, LayerShell};
use log;
use regex::Regex;

use crate::{
    Error,
    config::{self, Anchor, Config, CustomKeyHintLocation, Key, WrapMode},
};

mod input;
mod keys;
mod label;
mod rows;
mod search;
mod window;

use input::setup_key_event_handler;
pub use keys::Modifier;
use keys::{
    has_command_modifier, is_key_match, modifiers_from_mask, significant_modifiers, unshifted_key,
};
use label::{parse_label, truncate_markup};
use rows::{ChildPosition, build_ui_from_menu_items, find_visible_child, select_visible_child};
pub use search::{apply_sort, filtered_query};
use search::{set_menu_visibility_for_search, sort_flow_box_childs};
use window::{
    build_custom_key_view, build_main_box, calculate_dynamic_lines_window_height,
    create_background, get_monitor_geometry, load_css, window_show_resize,
};

pub type ArcMenuMap<T> = Arc<RwLock<HashMap<FlowBoxChild, MenuItem<T>>>>;

pub type ArcProvider<T> = Arc<Mutex<dyn ItemProvider<T> + Send>>;

pub type ArcFactory<T> = Arc<Mutex<dyn ItemFactory<T> + Send>>;

pub struct Selection<T: Clone + Send> {
    pub menu: MenuItem<T>,
    pub custom_key: Option<KeyBinding>,
}

type SelectionSender<T> = Sender<Result<Selection<T>, Error>>;

pub struct ProviderData<T: Clone> {
    pub items: Option<Vec<MenuItem<T>>>,
}

pub trait ItemProvider<T: Clone> {
    fn get_elements(&mut self, search: Option<&str>) -> ProviderData<T>;

    /// Get elements below the given menu entry.
    /// Will be called for completion
    /// If (true, None) is returned and submit-accept is set in the config, this
    /// will be handled the name way as pressing enter (or the configured submit key).
    fn get_sub_elements(&mut self, item: &MenuItem<T>) -> ProviderData<T>;
}

pub trait ItemFactory<T: Clone> {
    fn new_menu_item(&self, label: String) -> Option<MenuItem<T>>;
}

/// Default generic item factory that creates an almost empty menu item
/// Without data, no icon, and sort score of 0.
pub struct DefaultItemFactory<T: Clone> {
    _marker: PhantomData<T>,
}

impl<T: Clone> DefaultItemFactory<T> {
    #[must_use]
    pub fn new() -> DefaultItemFactory<T> {
        DefaultItemFactory::<T> {
            _marker: PhantomData,
        }
    }
}

impl<T: Clone> Default for DefaultItemFactory<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> ItemFactory<T> for DefaultItemFactory<T> {
    fn new_menu_item(&self, label: String) -> Option<MenuItem<T>> {
        Some(MenuItem::new(label, None, None, vec![], None, 0.0, None))
    }
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

#[derive(PartialEq)]
pub enum ExpandMode {
    Verbatim,
    WithSpace,
}

impl From<config::Layer> for gtk4_layer_shell::Layer {
    fn from(value: config::Layer) -> Self {
        match value {
            config::Layer::Background => gtk4_layer_shell::Layer::Background,
            config::Layer::Bottom => gtk4_layer_shell::Layer::Bottom,
            config::Layer::Top => gtk4_layer_shell::Layer::Top,
            config::Layer::Overlay => gtk4_layer_shell::Layer::Overlay,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct KeyBinding {
    pub key: Key,
    pub modifiers: HashSet<Modifier>,
    pub label: String,
    pub visible: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct CustomKeyHint {
    pub label: String,
    pub location: CustomKeyHintLocation,
}

#[derive(Clone, PartialEq, Debug)]
pub struct CustomKeys {
    pub bindings: Vec<KeyBinding>,
    pub hint: Option<CustomKeyHint>,
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
        //allow_submit: bool,
    ) -> Self {
        MenuItem {
            label,
            icon_path,
            action,
            sub_elements,
            working_dir,
            initial_sort_score,
            data,
            //allow_submit,
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
    item_factory: Option<ArcFactory<T>>,
    selected_sender: SelectionSender<T>,
    config: Arc<RwLock<Config>>,
    search_ignored_words: Option<Vec<Regex>>,
    expand_mode: ExpandMode,
}

struct UiElements<T: Clone> {
    app: Application,
    window: ApplicationWindow,
    background: Option<ApplicationWindow>,
    search: SearchEntry,
    main_box: FlowBox,
    menu_rows: ArcMenuMap<T>,
    search_text: Arc<Mutex<String>>,
    search_delete_event: Arc<Mutex<Option<SignalHandlerId>>>,
    outer_box: gtk4::Box,
    scroll: ScrolledWindow,
    custom_key_box: gtk4::Box,
    /// Incremented whenever the menu is rebuilt, so pending incremental builds can stop.
    build_generation: Cell<u64>,
    /// Id of the latest search sent to the item provider, older results are dropped.
    provider_request: Arc<AtomicU64>,
}

thread_local! {
    /// The style sheet currently added to the display, if any.
    static CSS_PROVIDER: RefCell<Option<CssProvider>> = const { RefCell::new(None) };
}

/// Shows the user interface and **blocks** until the user selected an entry
/// # Errors
///
/// Will return Err when the channel between the UI and this is broken
/// # Panics
/// When failing to unwrap the arc lock
pub fn show<T>(
    config: &Arc<RwLock<Config>>,
    item_provider: ArcProvider<T>,
    item_factory: Option<ArcFactory<T>>,
    search_ignored_words: Option<Vec<Regex>>,
    expand_mode: ExpandMode,
    custom_keys: Option<CustomKeys>,
) -> Result<Selection<T>, Error>
where
    T: Clone + 'static + Send,
{
    gtk4::init().map_err(|e| Error::Graphics(e.to_string()))?;
    log::debug!("Starting GUI");
    load_css(config.read().unwrap().style().as_deref());

    // No application id: "worf" is not a valid id (it needs a dot) and every worf process
    // must run on its own instead of handing over to a running instance.
    // Without an id the wayland app_id is the program name, "worf".
    let app = Application::builder()
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    // Unbounded, so a second send on the main thread can never block it.
    let (sender, receiver) = channel::unbounded();

    let meta = Rc::new(MetaData {
        item_provider,
        item_factory,
        selected_sender: sender,
        config: Arc::clone(config),
        search_ignored_words,
        expand_mode,
    });

    let connect_cfg = Arc::clone(config);
    app.connect_activate(move |app| {
        build_ui::<T>(&connect_cfg, &meta, app.clone(), custom_keys.as_ref());
    });

    let gtk_args: [&str; 0] = [];
    app.run_with_args(&gtk_args);

    // The selection is sent from the main loop before it quits. If the window was closed
    // in another way (i.e. by the compositor) nothing was sent.
    receiver.try_recv().unwrap_or(Err(Error::NoSelection))
}

fn free_ui_on_shutdown<T: Clone + 'static>(ui: &Rc<UiElements<T>>) {
    // Signal handlers on the widgets hold the UI, so it is never freed on its own.
    // Destroying the windows on shutdown drops the handlers and breaks those cycles,
    // which matters for modes that call `show` in a loop.
    let shutdown_ui = Rc::downgrade(ui);
    ui.app.connect_shutdown(move |_| {
        if let Some(ui) = shutdown_ui.upgrade() {
            // Stops a pending incremental build.
            ui.build_generation
                .set(ui.build_generation.get().wrapping_add(1));
            ui.menu_rows.write().unwrap().clear();
            ui.window.destroy();
            if let Some(background) = &ui.background {
                background.destroy();
            }
        }
    });
}

fn build_ui<T>(
    config: &Arc<RwLock<Config>>,
    meta: &Rc<MetaData<T>>,
    app: Application,
    custom_keys: Option<&CustomKeys>,
) where
    T: Clone + 'static + Send,
{
    let start = Instant::now();

    let provider_clone = Arc::clone(&meta.item_provider);
    let get_provider_elements = thread::spawn(move || {
        log::debug!("getting items");
        provider_clone.lock().unwrap().get_elements(None)
    });

    let window = ApplicationWindow::builder()
        .application(&app)
        .decorated(false)
        .resizable(false)
        .default_width(1)
        .default_height(1)
        .build();

    let background = create_background(&config.read().unwrap());

    let search_entry = SearchEntry::new();
    search_entry.set_can_focus(false);
    let main_window = window.clone();
    main_window.set_can_focus(true);
    let ui_elements = Rc::new(UiElements {
        app,
        window: main_window,
        background,
        search: search_entry,
        main_box: FlowBox::new(),
        menu_rows: Arc::new(RwLock::new(HashMap::new())),
        search_text: Arc::new(Mutex::new(String::new())),
        search_delete_event: Arc::new(Mutex::new(None)),
        outer_box: gtk4::Box::new(config.read().unwrap().orientation().into(), 0),
        scroll: ScrolledWindow::new(),
        custom_key_box: gtk4::Box::new(Orientation::Vertical, 0),
        build_generation: Cell::new(0),
        provider_request: Arc::new(AtomicU64::new(0)),
    });
    free_ui_on_shutdown(&ui_elements);

    // handle keys as soon as possible
    setup_key_event_handler(&ui_elements, meta, custom_keys);

    log::debug!("keyboard ready after {:?}", start.elapsed());

    if !config.read().unwrap().normal_window() {
        // Initialize the window as a layer
        ui_elements.window.init_layer_shell();
        ui_elements
            .window
            .set_layer(config.read().unwrap().layer().into());
        ui_elements
            .window
            .set_keyboard_mode(KeyboardMode::Exclusive);
    }

    ui_elements.window.set_widget_name("window");
    ui_elements.window.set_namespace(Some("worf"));

    if let Some(location) = config.read().unwrap().location() {
        for anchor in location {
            ui_elements.window.set_anchor(anchor.into(), true);
        }
    }

    ui_elements.outer_box.set_widget_name("outer-box");
    ui_elements.outer_box.append(&ui_elements.search);
    if let Some(custom_keys) = custom_keys {
        build_custom_key_view(
            custom_keys,
            &ui_elements.outer_box,
            &ui_elements.custom_key_box,
        );
    }

    ui_elements.window.set_child(Some(&ui_elements.outer_box));
    // Set initial focus to the search entry
    ui_elements.search.grab_focus();

    ui_elements.scroll.set_widget_name("scroll");
    ui_elements.scroll.set_hexpand(true);
    ui_elements.scroll.set_vexpand(true);

    if config.read().unwrap().hide_scroll() {
        ui_elements
            .scroll
            .set_policy(PolicyType::External, PolicyType::External);
    }
    ui_elements.outer_box.append(&ui_elements.scroll);

    build_main_box(&config.read().unwrap(), &ui_elements);
    build_search_entry(&config.read().unwrap(), &ui_elements, meta);

    let wrapper_box = gtk4::Box::new(Orientation::Vertical, 0);
    wrapper_box.append(&ui_elements.main_box);
    ui_elements.scroll.set_child(Some(&wrapper_box));

    let wait_for_items = Instant::now();
    let provider_elements = get_provider_elements.join().unwrap();
    log::debug!("got items after {:?}", wait_for_items.elapsed());

    let cfg = Arc::clone(config);
    let ui = Rc::clone(&ui_elements);
    ui_elements.window.connect_is_active_notify(move |_| {
        window_show_resize(&cfg.read().unwrap(), &ui);
    });

    if let Some(elements) = provider_elements.items {
        build_ui_from_menu_items(&ui_elements, meta, elements);
    }

    let window_start = Instant::now();
    ui_elements.window.present();
    if let Some(background) = &ui_elements.background {
        background.present();
    }

    log::debug!("window show took {:?}", window_start.elapsed());

    log::debug!("Building UI took {:?}", start.elapsed());
}

fn build_search_entry<T: Clone + Send + 'static>(
    config: &Config,
    ui_elements: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
) {
    ui_elements.search.set_widget_name("input");
    ui_elements.search.set_css_classes(&["input"]);
    ui_elements
        .search
        .set_placeholder_text(Some(&config.prompt().unwrap_or("Search...".to_owned())));
    ui_elements.search.set_can_focus(false);
    search_start_listen_delete_event(ui_elements, meta);

    if config.hide_search() {
        ui_elements.search.set_visible(false);
    }
    if let Some(search) = config.search() {
        set_search_text(ui_elements, meta, &search);
    }
}

fn search_start_listen_delete_event<T: Clone + Send + 'static>(
    ui_elements: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
) {
    let ui_clone = Rc::clone(ui_elements);
    let meta_clone = Rc::clone(meta);
    *ui_elements.search_delete_event.lock().unwrap() =
        Some(ui_elements.search.connect_text_notify(move |se| {
            if se.text().is_empty() {
                ui_clone.search_text.lock().unwrap().clear();
                update_view_from_provider(&ui_clone, &meta_clone, "");
            }
        }));
}

fn search_stop_listen_delete_event<T: Clone + Send + 'static>(ui_elements: &UiElements<T>) {
    let mut lock = ui_elements.search_delete_event.lock().unwrap();
    if let Some(id) = lock.take() {
        ui_elements.search.disconnect(id);
    }
}

fn set_search_text<T: Clone + Send + 'static>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    query: &str,
) {
    search_stop_listen_delete_event(ui);
    let mut lock = ui.search_text.lock().unwrap();
    query.clone_into(&mut lock);
    if let Some(pw) = meta.config.read().unwrap().password() {
        // One mask per char (not byte), so the cursor position matches the query.
        ui.search.set_text(&pw.repeat(query.chars().count()));
    } else {
        ui.search.set_text(query);
    }
    search_start_listen_delete_event(ui, meta);
}

fn update_view_from_provider<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>, query: &str)
where
    T: Clone + Send + 'static,
{
    // Providers can be slow (i.e. file mode reads directories), so they run in the background
    // to keep the UI responsive while typing. Only the result of the latest search is used.
    let request = ui.provider_request.fetch_add(1, AtomicOrdering::SeqCst) + 1;
    let latest = Arc::clone(&ui.provider_request);
    let provider = Arc::clone(&meta.item_provider);
    let provider_query = query.to_owned();
    let query = query.to_owned();
    let ui = Rc::clone(ui);
    let meta = Rc::clone(meta);

    MainContext::default().spawn_local(async move {
        let result = gio::spawn_blocking(move || {
            // Skip the work if another search was started in the meantime.
            (latest.load(AtomicOrdering::SeqCst) == request)
                .then(|| provider.lock().unwrap().get_elements(Some(&provider_query)))
        })
        .await;

        // A newer search replaced this one, or the window was closed in the meantime.
        // The provider still ran for the latest search, modes may rely on its state.
        if ui.provider_request.load(AtomicOrdering::SeqCst) != request || !ui.window.is_visible() {
            return;
        }
        match result {
            Ok(Some(ProviderData {
                items: Some(filtered_list),
            })) => {
                // The view is updated once the new rows exist.
                build_ui_from_menu_items(&ui, &meta, filtered_list);
            }
            Ok(_) => update_view(&ui, &meta, &query),
            Err(_) => log::error!("item provider panicked while searching for {query}"),
        }
    });
}

fn update_view<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>, query: &str)
where
    T: Clone + Send + 'static,
{
    set_menu_visibility_for_search(
        query,
        &mut ui.menu_rows.write().unwrap(),
        &meta.config,
        meta.search_ignored_words.as_ref(),
    );
    // Scores changed, so sort again. The sort function needs the lock, so it must not be held.
    ui.main_box.invalidate_sort();

    let menu_rows = ui.menu_rows.read().unwrap();
    select_visible_child(&*menu_rows, &ui.main_box, &ui.scroll, &ChildPosition::Front);

    if !query.is_empty() && meta.config.read().unwrap().auto_select_on_search() {
        let visible_items: Vec<_> = menu_rows.iter().filter(|(_, menu)| menu.visible).collect();

        let item = if visible_items.len() == 1 {
            Some(visible_items[0].1.clone())
        } else {
            None
        };

        drop(menu_rows);

        if let Some(item) = item
            && let Err(e) = handle_selected_item(ui, meta, None, Some(item), None)
        {
            log::error!("failed to handle selected item {e}");
        }
    } else {
        drop(menu_rows);
    }

    if meta.config.read().unwrap().dynamic_lines()
        && let Some(geometry) = get_monitor_geometry(ui.window.surface().as_ref())
    {
        let height =
            calculate_dynamic_lines_window_height(&meta.config.read().unwrap(), ui, geometry);
        ui.window.set_height_request(height);
    }
}

fn close_gui(app: &Application) {
    app.quit();
}

fn get_selected_item<T>(ui: &UiElements<T>) -> Option<MenuItem<T>>
where
    T: Clone + Send + 'static,
{
    if let Some(s) = ui.main_box.selected_children().into_iter().next() {
        let list_items = ui.menu_rows.read().unwrap();
        let item = list_items.get(&s);
        if let Some(selected_item) = item
            && selected_item.visible
        {
            // Check if item is an expander (has sub_elements)
            if !selected_item.sub_elements.is_empty() {
                // Try to get the Expander widget from the FlowBoxChild
                if let Some(expander) = s.child().and_then(|w| w.downcast::<Expander>().ok())
                    && expander.is_expanded()
                    && let Some(list_box) =
                        expander.child().and_then(|w| w.downcast::<ListBox>().ok())
                    && let Some(selected_row) = list_box.selected_row()
                {
                    let idx = selected_row.index();
                    #[allow(clippy::cast_sign_loss)]
                    if let Some(sub_item) = selected_item.sub_elements.get(idx as usize) {
                        return Some(sub_item.clone());
                    }
                }
            }
            // Not an expander or not expanded, return top-level item
            return Some(selected_item.clone());
        }
    }

    None
}

fn handle_selected_item<T>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    query: Option<&str>,
    item: Option<MenuItem<T>>,
    custom_key: Option<&KeyBinding>,
) -> Result<(), String>
where
    T: Clone + Send + 'static,
{
    if let Some(selected_item) = item {
        send_selected_item(ui, meta, custom_key.cloned(), selected_item);
        return Ok(());
    } else if let Some(item) = get_selected_item(ui) {
        send_selected_item(ui, meta, custom_key.cloned(), item);
        return Ok(());
    }

    if let Some(factory) = meta.item_factory.as_ref() {
        let factory = factory.lock().unwrap();
        let label = filtered_query(meta.search_ignored_words.as_ref(), query.unwrap_or(""));
        let item = factory.new_menu_item(label);
        if let Some(item) = item {
            send_selected_item(ui, meta, custom_key.cloned(), item);
            return Ok(());
        }
    }

    Err("selected item cannot be resolved".to_owned())
}

fn send_selected_item<T>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    custom_key: Option<KeyBinding>,
    selected_item: MenuItem<T>,
) where
    T: Clone + Send + 'static,
{
    let ui_clone = Rc::clone(ui);
    let meta_clone = Rc::clone(meta);
    ui.window.connect_hide(move |_| {
        if let Err(e) = meta_clone.selected_sender.send(Ok(Selection {
            menu: selected_item.clone(),
            custom_key: custom_key.clone(),
        })) {
            log::error!("failed to send message {e}");
        }
    });
    if let Some(background) = &ui.background {
        background.hide();
    }
    ui.window.hide();
    close_gui(&ui_clone.app);
}

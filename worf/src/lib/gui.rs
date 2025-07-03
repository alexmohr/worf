use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Instant,
};

use crossbeam::channel::{self, Sender};
use gdk4::glib::SignalHandlerId;
use gdk4::prelude::ObjectExt;
use gdk4::{
    Display, Rectangle,
    gio::File,
    glib::{self, MainContext, Propagation},
    prelude::{Cast, DisplayExt, MonitorExt, SurfaceExt},
};
use gtk4::prelude::{AdjustmentExt, EventControllerExt};
use gtk4::{
    Align, Application, ApplicationWindow, CssProvider, EventControllerKey, Expander, FlowBox,
    FlowBoxChild, GestureClick, Image, Label, ListBox, ListBoxRow, NaturalWrapMode, Ordering,
    Orientation, PolicyType, ScrolledWindow, SearchEntry, Widget,
    glib::ControlFlow,
    prelude::{
        ApplicationExt, ApplicationExtManual, BoxExt, EditableExt, FlowBoxChildExt,
        GestureSingleExt, GtkWindowExt, ListBoxRowExt, NativeExt, OrientableExt, WidgetExt,
    },
};
use gtk4_layer_shell::{Edge, KeyboardMode, LayerShell};
use log;
use regex::Regex;

use crate::{
    Error,
    config::{
        self, Anchor, Config, CustomKeyHintLocation, Key, KeyDetectionType, MatchMethod, SortOrder,
        WrapMode,
    },
    desktop,
    desktop::known_image_extension_regex_pattern,
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

fn into_core_order(gtk_order: Ordering) -> core::cmp::Ordering {
    match gtk_order {
        Ordering::Smaller => core::cmp::Ordering::Less,
        Ordering::Larger => core::cmp::Ordering::Greater,
        _ => core::cmp::Ordering::Equal,
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

    // /// If set to true, the item is _not_ an intermediate thing
    // /// and is acceptable, i.e. will close the UI
    // pub allow_submit: bool,
    // todo
    /// Score the item got in the current search
    search_sort_score: f64,
    /// True if the item is visible
    visible: bool,
}

impl From<gtk4::gdk::Key> for Key {
    fn from(value: gdk4::Key) -> Self {
        match value {
            // Letters
            gdk4::Key::A => Key::A,
            gdk4::Key::B => Key::B,
            gdk4::Key::C => Key::C,
            gdk4::Key::D => Key::D,
            gdk4::Key::E => Key::E,
            gdk4::Key::F => Key::F,
            gdk4::Key::G => Key::G,
            gdk4::Key::H => Key::H,
            gdk4::Key::I => Key::I,
            gdk4::Key::J => Key::J,
            gdk4::Key::K => Key::K,
            gdk4::Key::L => Key::L,
            gdk4::Key::M => Key::M,
            gdk4::Key::N => Key::N,
            gdk4::Key::O => Key::O,
            gdk4::Key::P => Key::P,
            gdk4::Key::Q => Key::Q,
            gdk4::Key::R => Key::R,
            gdk4::Key::S => Key::S,
            gdk4::Key::T => Key::T,
            gdk4::Key::U => Key::U,
            gdk4::Key::V => Key::V,
            gdk4::Key::W => Key::W,
            gdk4::Key::X => Key::X,
            gdk4::Key::Y => Key::Y,
            gdk4::Key::Z => Key::Z,

            // Numbers
            gdk4::Key::_0 => Key::Num0,
            gdk4::Key::_1 => Key::Num1,
            gdk4::Key::_2 => Key::Num2,
            gdk4::Key::_3 => Key::Num3,
            gdk4::Key::_4 => Key::Num4,
            gdk4::Key::_5 => Key::Num5,
            gdk4::Key::_6 => Key::Num6,
            gdk4::Key::_7 => Key::Num7,
            gdk4::Key::_8 => Key::Num8,
            gdk4::Key::_9 => Key::Num9,

            // Function Keys
            gdk4::Key::F1 => Key::F1,
            gdk4::Key::F2 => Key::F2,
            gdk4::Key::F3 => Key::F3,
            gdk4::Key::F4 => Key::F4,
            gdk4::Key::F5 => Key::F5,
            gdk4::Key::F6 => Key::F6,
            gdk4::Key::F7 => Key::F7,
            gdk4::Key::F8 => Key::F8,
            gdk4::Key::F9 => Key::F9,
            gdk4::Key::F10 => Key::F10,
            gdk4::Key::F11 => Key::F11,
            gdk4::Key::F12 => Key::F12,

            // Navigation / Editing
            gdk4::Key::Escape => Key::Escape,
            gdk4::Key::Return => Key::Enter,
            gdk4::Key::space => Key::Space,
            gdk4::Key::Tab => Key::Tab,
            gdk4::Key::BackSpace => Key::Backspace,
            gdk4::Key::Insert => Key::Insert,
            gdk4::Key::Delete => Key::Delete,
            gdk4::Key::Home => Key::Home,
            gdk4::Key::End => Key::End,
            gdk4::Key::Page_Up => Key::PageUp,
            gdk4::Key::Page_Down => Key::PageDown,
            gdk4::Key::Left => Key::Left,
            gdk4::Key::Right => Key::Right,
            gdk4::Key::Up => Key::Up,
            gdk4::Key::Down => Key::Down,

            // Special characters
            gdk4::Key::exclam => Key::Exclamation,
            gdk4::Key::at => Key::At,
            gdk4::Key::numbersign => Key::Hash,
            gdk4::Key::dollar => Key::Dollar,
            gdk4::Key::percent => Key::Percent,
            gdk4::Key::asciicircum => Key::Caret,
            gdk4::Key::ampersand => Key::Ampersand,
            gdk4::Key::asterisk => Key::Asterisk,
            gdk4::Key::parenleft => Key::LeftParen,
            gdk4::Key::parenright => Key::RightParen,
            gdk4::Key::minus => Key::Minus,
            gdk4::Key::underscore => Key::Underscore,
            gdk4::Key::equal => Key::Equal,
            gdk4::Key::plus => Key::Plus,
            gdk4::Key::bracketleft => Key::LeftBracket,
            gdk4::Key::bracketright => Key::RightBracket,
            gdk4::Key::braceleft => Key::LeftBrace,
            gdk4::Key::braceright => Key::RightBrace,
            gdk4::Key::backslash => Key::Backslash,
            gdk4::Key::bar => Key::Pipe,
            gdk4::Key::semicolon => Key::Semicolon,
            gdk4::Key::colon => Key::Colon,
            gdk4::Key::apostrophe => Key::Apostrophe,
            gdk4::Key::quotedbl => Key::Quote,
            gdk4::Key::comma => Key::Comma,
            gdk4::Key::period => Key::Period,
            gdk4::Key::slash => Key::Slash,
            gdk4::Key::question => Key::Question,
            gdk4::Key::grave => Key::Grave,
            gdk4::Key::asciitilde => Key::Tilde,
            _ => Key::None,
        }
    }
}

impl From<u32> for Key {
    fn from(value: u32) -> Self {
        match value {
            // Letters
            38 => Key::A,
            56 => Key::B,
            54 => Key::C,
            40 => Key::D,
            26 => Key::E,
            41 => Key::F,
            42 => Key::G,
            43 => Key::H,
            31 => Key::I,
            44 => Key::J,
            45 => Key::K,
            46 => Key::L,
            58 => Key::M,
            57 => Key::N,
            32 => Key::O,
            33 => Key::P,
            24 => Key::Q,
            27 => Key::R,
            39 => Key::S,
            28 => Key::T,
            30 => Key::U,
            55 => Key::V,
            25 => Key::W,
            53 => Key::X,
            29 => Key::Y,
            52 => Key::Z,

            // Numbers
            10 => Key::Num1,
            11 => Key::Num2,
            12 => Key::Num3,
            13 => Key::Num4,
            14 => Key::Num5,
            15 => Key::Num6,
            16 => Key::Num7,
            17 => Key::Num8,
            18 => Key::Num9,
            19 => Key::Num0,

            // Function Keys
            67 => Key::F1,
            68 => Key::F2,
            69 => Key::F3,
            70 => Key::F4,
            71 => Key::F5,
            72 => Key::F6,
            73 => Key::F7,
            74 => Key::F8,
            75 => Key::F9,
            76 => Key::F10,
            77 => Key::F11,
            78 => Key::F12,

            // Navigation / Editing
            9 => Key::Escape,
            36 => Key::Enter,
            65 => Key::Space,
            23 => Key::Tab,
            22 => Key::Backspace,
            118 => Key::Insert,
            119 => Key::Delete,
            110 => Key::Home,
            115 => Key::End,
            112 => Key::PageUp,
            117 => Key::PageDown,
            113 => Key::Left,
            114 => Key::Right,
            111 => Key::Up,
            116 => Key::Down,

            // Special characters
            20 => Key::Exclamation,
            63 => Key::At,
            3 => Key::Hash,
            4 => Key::Dollar,
            5 => Key::Percent,
            6 => Key::Caret,
            7 => Key::Ampersand,
            8 => Key::Asterisk,
            34 => Key::LeftParen,
            35 => Key::RightParen,
            48 => Key::Minus,
            47 => Key::Underscore,
            21 => Key::Equal,
            49 => Key::Plus,
            51 => Key::Backslash,
            94 => Key::Pipe,
            50 => Key::Quote,
            59 => Key::Comma,
            60 => Key::Period,
            61 => Key::Slash,
            62 => Key::Question,
            96 => Key::Grave,
            97 => Key::Tilde,
            _ => Key::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Shift,
    Control,
    Alt,
    Super,
    Meta,
    CapsLock,
    None,
}

#[derive(PartialEq)]
pub enum ExpandMode {
    Verbatim,
    WithSpace,
}

fn modifiers_from_mask(mask: gdk4::ModifierType) -> HashSet<Modifier> {
    let mut modifiers = HashSet::new();

    if mask.contains(gdk4::ModifierType::SHIFT_MASK) {
        modifiers.insert(Modifier::Shift);
    }
    if mask.contains(gdk4::ModifierType::CONTROL_MASK) {
        modifiers.insert(Modifier::Control);
    }
    if mask.contains(gdk4::ModifierType::ALT_MASK) {
        modifiers.insert(Modifier::Alt);
    }
    if mask.contains(gdk4::ModifierType::SUPER_MASK) {
        modifiers.insert(Modifier::Super);
    }
    if mask.contains(gdk4::ModifierType::META_MASK) {
        modifiers.insert(Modifier::Meta);
    }
    if mask.contains(gdk4::ModifierType::LOCK_MASK) {
        modifiers.insert(Modifier::CapsLock);
    }

    if modifiers.is_empty() {
        modifiers.insert(Modifier::None);
    }

    modifiers
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
    if let Some(ref css) = config.read().unwrap().style() {
        log::debug!("loading css from {css}");
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
    // Use glib's MainContext to handle the receiver asynchronously
    let main_context = MainContext::default();
    let receiver_result = main_context.block_on(async {
        MainContext::default()
            .spawn_local(async move { receiver.recv().map_err(|e| Error::Io(e.to_string())) })
            .await
            .unwrap_or_else(|e| Err(Error::Io(e.to_string())))
    });

    receiver_result?
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
    search_entry.set_can_focus(true);
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
    });

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

    log::debug!("Building UI took {:?}", start.elapsed(),);
}

fn create_background(config: &Config) -> Option<ApplicationWindow> {
    if config.blurred_background() {
        let background = ApplicationWindow::builder()
            .decorated(false)
            .resizable(false)
            .fullscreened(config.blurred_background_fullscreen())
            .default_width(100)
            .default_height(100)
            .build();
        if !config.normal_window() {
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

fn build_main_box<T: Clone + 'static>(config: &Config, ui_elements: &Rc<UiElements<T>>) {
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

fn build_custom_key_view(custom_keys: &CustomKeys, outer_box: &gtk4::Box, inner_box: &gtk4::Box) {
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

fn set_search_text<T: Clone + Send + 'static>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    query: &str,
) {
    search_stop_listen_delete_event(ui);
    let mut lock = ui.search_text.lock().unwrap();
    query.clone_into(&mut lock);
    if let Some(pw) = meta.config.read().unwrap().password() {
        let mut ui_text = String::new();
        for _ in 0..query.len() {
            ui_text += &pw;
        }
        ui.search.set_text(&ui_text);
    } else {
        ui.search.set_text(query);
    }
    search_start_listen_delete_event(ui, meta);
}

fn build_ui_from_menu_items<T: Clone + 'static + Send>(
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

        let meta_clone = Rc::<MetaData<T>>::clone(meta);
        let ui_clone = Rc::<UiElements<T>>::clone(ui);

        glib::idle_add_local(move || {
            let mut done = false;
            {
                ui_clone.main_box.unset_sort_func();
                let mut lock = ui_clone.menu_rows.write().unwrap();

                for _ in 0..25 {
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
                sort_flow_box_childs(child1, child2, &items_sort)
            });

            if done {
                let lock = ui_clone.menu_rows.read().unwrap();

                select_visible_child(
                    &*lock,
                    &ui_clone.main_box,
                    &ui_clone.scroll,
                    &ChildPosition::Front,
                );

                log::debug!(
                    "Created {} menu items in {:?}",
                    &lock.len(),
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
    ui_elements: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    custom_keys: Option<&CustomKeys>,
) {
    // handle keys as soon as possible
    // Remove old handler, use only one for both window and search
    let key_controller_window = EventControllerKey::new();
    key_controller_window.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let ui_clone = Rc::clone(ui_elements);
    let meta_clone = Rc::clone(meta);
    let keys_clone = custom_keys.cloned();
    key_controller_window.connect_key_pressed(move |_, key_value, key_code, modifier| {
        handle_key_press(
            &ui_clone,
            &meta_clone,
            key_value,
            key_code,
            modifier,
            keys_clone.as_ref(),
        )
    });

    ui_elements.window.add_controller(key_controller_window);

    let key_controller_search = EventControllerKey::new();
    key_controller_search.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let ui_clone2 = Rc::clone(ui_elements);
    let meta_clone2 = Rc::clone(meta);
    let keys_clone2 = custom_keys.cloned();
    key_controller_search.connect_key_pressed(move |_, key_value, key_code, modifier| {
        handle_key_press(
            &ui_clone2,
            &meta_clone2,
            key_value,
            key_code,
            modifier,
            keys_clone2.as_ref(),
        )
    });
    ui_elements.search.add_controller(key_controller_search);
}

fn is_key_match(
    key_opt: Option<Key>,
    key_detection_type: &KeyDetectionType,
    key_code: u32,
    gdk_key: gdk4::Key,
) -> bool {
    if let Some(key) = key_opt {
        if key_detection_type == &KeyDetectionType::Code {
            key == key_code.into()
        } else {
            key == gdk_key.to_upper().into()
        }
    } else {
        false
    }
}

#[allow(clippy::cast_sign_loss)] // ok because we only need positive values
fn handle_key_press<T: Clone + 'static + Send>(
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
            if !query.is_empty() {
                let pos = ui.search.position();
                let del_pos = if keyboard_key == gdk4::Key::BackSpace {
                    pos - 1
                } else {
                    pos
                };
                if let Some((start, ch)) = query.char_indices().nth(del_pos as usize) {
                    let end = start + ch.len_utf8();
                    query.replace_range(start..end, "");
                }
                set_search_text(ui, meta, &query);
                ui.search.set_position(pos - 1);
                update_view_from_provider(ui, meta, &query);
            }
        }
        gdk4::Key::Home => {
            ui.search.set_position(0);
        }
        gdk4::Key::Left => {
            ui.search.set_position(ui.search.position() - 1);
        }
        gdk4::Key::Right => {
            ui.search.set_position(ui.search.position() + 1);
        }
        gdk4::Key::End => {
            if let Ok(i) = i32::try_from(ui.search_text.lock().unwrap().len() + 1) {
                ui.search.set_position(i);
            }
        }
        gdk4::Key::Up => {
            return move_selection(ui, true);
        }
        gdk4::Key::Down => {
            return move_selection(ui, false);
        }
        _ => {
            if let Some(c) = keyboard_key.to_unicode() {
                let mut query = {
                    let search_text = ui.search_text.lock().unwrap();
                    search_text.clone()
                };
                let pos = ui.search.position();
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

fn move_selection<T: Clone + Send + 'static>(ui: &Rc<UiElements<T>>, up: bool) -> Propagation {
    let selected_children = ui.main_box.selected_children();
    let Some(selected) = selected_children.first() else {
        return Propagation::Proceed;
    };

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

    if up && first_child == *selected {
        select_visible_child(
            &ui.menu_rows.read().unwrap(),
            &ui.main_box,
            &ui.scroll,
            &ChildPosition::Back,
        );
        Propagation::Stop
    } else if !up && last_child == *selected {
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

fn handle_custom_keys<T: Clone + 'static + Send>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    keyboard_key: gdk4::Key,
    key_code: u32,
    modifier_type: gdk4::ModifierType,
    custom_keys: Option<&CustomKeys>,
) -> Propagation {
    let detection_type = meta.config.read().unwrap().key_detection_type();
    if let Some(custom_keys) = custom_keys {
        let mods = modifiers_from_mask(modifier_type);
        for custom_key in &custom_keys.bindings {
            let custom_key_match = if detection_type == KeyDetectionType::Code {
                custom_key.key == key_code.into()
            } else {
                custom_key.key == keyboard_key.to_upper().into()
            } && mods.is_subset(&custom_key.modifiers);

            log::debug!("custom key {custom_key:?}, match {custom_key_match}");

            if custom_key_match {
                let search_lock = ui.search_text.lock().unwrap();
                if let Err(e) =
                    handle_selected_item(ui, meta, Some(&search_lock), None, Some(custom_key))
                {
                    log::error!("{e}");
                }
            }
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
    }
    // exit
    else if is_key_match(
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

fn update_view_from_provider<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>, query: &str)
where
    T: Clone + Send + 'static,
{
    let data = meta.item_provider.lock().unwrap().get_elements(Some(query));
    if let Some(filtered_list) = data.items {
        build_ui_from_menu_items(ui, meta, filtered_list);
    }
    update_view(ui, meta, query);
}

fn update_view<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>, query: &str)
where
    T: Clone + Send + 'static,
{
    let mut menu_rows = ui.menu_rows.write().unwrap();
    set_menu_visibility_for_search(
        query,
        &mut menu_rows,
        &meta.config,
        meta.search_ignored_words.as_ref(),
    );

    select_visible_child(&*menu_rows, &ui.main_box, &ui.scroll, &ChildPosition::Front);

    if meta.config.read().unwrap().auto_select_on_search() {
        let visible_items = menu_rows
            .iter()
            .filter(|(_, menu)| menu.visible)
            .collect::<Vec<_>>();
        if visible_items.len() == 1 {
            if let Err(e) =
                handle_selected_item(ui, meta, None, Some(visible_items[0].1.clone()), None)
            {
                log::error!("failed to handle selected item {e}");
            }
        }
    }

    drop(menu_rows);
    if meta.config.read().unwrap().dynamic_lines() {
        if let Some(geometry) = get_monitor_geometry(ui.window.surface().as_ref()) {
            let height =
                calculate_dynamic_lines_window_height(&meta.config.read().unwrap(), ui, geometry);
            ui.window.set_height_request(height);
        }
    }
}

fn handle_key_exit<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    if let Err(e) = meta.selected_sender.send(Err(Error::NoSelection)) {
        log::error!("failed to send message {e}");
    }
    close_gui(&ui.app);
    Propagation::Stop
}

fn handle_key_expand<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    if let Some(fb) = ui.main_box.selected_children().first() {
        if let Some(child) = fb.child() {
            let expander = child.downcast::<Expander>().ok();
            if let Some(expander) = expander {
                expander.set_expanded(true);
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
                    } else if let Err(e) =
                        handle_selected_item(ui, meta, None, Some(menu_item), None)
                    {
                        log::error!("{e}");
                    }
                }
            }
        }
    }
    Propagation::Stop
}

fn handle_key_copy<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    if let Some(item) = get_selected_item(ui) {
        if let Some(action) = item.action {
            if let Err(e) = desktop::copy_to_clipboard(action, None) {
                log::error!("failed to copy to clipboard: {e}");
            }
        }
    }
    if let Err(e) = meta.selected_sender.send(Err(Error::NoSelection)) {
        log::error!("failed to send message {e}");
    }
    close_gui(&ui.app);
    Propagation::Stop
}

fn handle_key_submit<T>(ui: &Rc<UiElements<T>>, meta: &Rc<MetaData<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    let search_lock = ui.search_text.lock().unwrap();
    if let Err(e) = handle_selected_item(ui, meta, Some(&search_lock), None, None) {
        log::error!("{e}");
    }
    Propagation::Stop
}

fn handle_key_hide_search<T>(ui: &Rc<UiElements<T>>) -> Propagation
where
    T: Clone + Send + 'static,
{
    ui.search.set_visible(!ui.search.is_visible());
    Propagation::Stop
}

fn sort_flow_box_childs<T: Clone>(
    child1: &FlowBoxChild,
    child2: &FlowBoxChild,
    items_lock: &ArcMenuMap<T>,
) -> Ordering {
    let lock = items_lock.read().unwrap();
    let m1 = lock.get(child1);
    let m2 = lock.get(child2);

    if !child1.is_visible() {
        return Ordering::Smaller;
    }
    if !child2.is_visible() {
        return Ordering::Larger;
    }

    sort_menu_items_by_score(m1, m2)
}

fn sort_menu_items_by_score<T: Clone>(
    m1: Option<&MenuItem<T>>,
    m2: Option<&MenuItem<T>>,
) -> Ordering {
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

fn window_show_resize<T: Clone + 'static>(config: &Config, ui: &Rc<UiElements<T>>) {
    let Some(geometry) = get_monitor_geometry(ui.window.surface().as_ref()) else {
        return;
    };

    if !config.blurred_background_fullscreen() {
        if let Some(background) = &ui.background {
            background.set_height_request(geometry.height());
            background.set_width_request(geometry.width());
        }
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

fn calculate_dynamic_lines_window_height<T: Clone + 'static>(
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

fn get_monitor_geometry(surface: Option<&gdk4::Surface>) -> Option<Rectangle> {
    surface
        .and_then(|surface| {
            let display = surface.display();
            display.monitor_at_surface(surface)
        })
        .map(|monitor| monitor.geometry())
}

#[allow(clippy::cast_possible_truncation)] // does not matter for calculating height
fn calculate_row_height<T: Clone + 'static>(
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
                        Some((f64::from(i32::from(config.image_size())) * factor) as i32)
                    } else {
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
        "heights: scroll {scroll_height}, window {window_height}, keys {height_box}, height {height:?}, lines {lines:?}"
    );

    height_box
        + scroll_height
        + height_search
        + height.map_or(0, |h| h * lines)
        + config.lines_additional_space()
}

fn close_gui(app: &Application) {
    app.quit();
}

fn visible_row_count<T: Clone + 'static>(ui: &UiElements<T>) -> i32 {
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

fn get_selected_item<T>(ui: &UiElements<T>) -> Option<MenuItem<T>>
where
    T: Clone + Send + 'static,
{
    if let Some(s) = ui.main_box.selected_children().into_iter().next() {
        let list_items = ui.menu_rows.read().unwrap();
        let item = list_items.get(&s);
        if let Some(selected_item) = item {
            if selected_item.visible {
                return Some(selected_item.clone());
            }
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

    let row_box = gtk4::Box::new(meta.config.read().unwrap().row_box_orientation().into(), 0);
    row_box.set_hexpand(true);
    row_box.set_vexpand(false);
    row_box.set_halign(Align::Fill);

    row.set_child(Some(&row_box));

    let (label_img, label_text) = parse_label(&element_to_add.label);

    let config = meta.config.read().unwrap();
    if meta.config.read().unwrap().allow_images() {
        let img = lookup_icon(
            element_to_add.icon_path.as_ref().map(AsRef::as_ref),
            &config,
        )
        .or(lookup_icon(label_img.as_ref().map(AsRef::as_ref), &config));

        if let Some(image) = img {
            image.set_widget_name("img");
            row_box.append(&image);
        }
    }

    let label = Label::new(label_text.as_ref().map(AsRef::as_ref));
    label.set_use_markup(meta.config.read().unwrap().allow_markup());
    label.set_natural_wrap_mode(meta.config.read().unwrap().line_wrap().into());
    label.set_hexpand(true);
    label.set_widget_name("text");
    label.set_wrap(true);
    if let Some(max_width_chars) = meta.config.read().unwrap().line_max_width_chars() {
        label.set_max_width_chars(max_width_chars);
    }

    if let Some(max_len) = meta.config.read().unwrap().line_max_chars() {
        if let Some(text) = label_text.as_ref() {
            if text.chars().count() > max_len {
                let end = text
                    .char_indices()
                    .nth(max_len)
                    .map_or(text.len(), |(idx, _)| idx);
                label.set_text(&format!("{}...", &text[..end]));
            }
        }
    }

    row_box.append(&label);

    if meta
        .config
        .read()
        .unwrap()
        .content_halign()
        .eq(&config::Align::Start)
        || meta
            .config
            .read()
            .unwrap()
            .content_halign()
            .eq(&config::Align::Fill)
    {
        label.set_xalign(0.0);
    }

    let click_ui = Rc::clone(ui);
    let click_meta = Rc::clone(meta);
    let element_clone = element_to_add.clone();

    let click = GestureClick::new();
    click.set_button(gtk4::gdk::BUTTON_PRIMARY);

    let presses = if meta.config.read().unwrap().single_click() {
        1
    } else {
        2
    };

    click.connect_pressed(move |_gesture, n_press, _x, _y| {
        if n_press == presses {
            if let Err(e) = handle_selected_item(
                &click_ui,
                &click_meta,
                None,
                Some(element_clone.clone()),
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

fn set_menu_visibility_for_search<T: Clone>(
    query: &str,
    items: &mut HashMap<FlowBoxChild, MenuItem<T>>,
    config: &Arc<RwLock<Config>>,
    search_ignored_words: Option<&Vec<Regex>>,
) {
    if query.is_empty() {
        for (fb, menu_item) in items.iter_mut() {
            menu_item.search_sort_score = 0.0;
            menu_item.visible = true;
            fb.set_visible(menu_item.visible);
        }
    }

    let mut query = if config.read().unwrap().insensitive() {
        query.to_owned().to_lowercase()
    } else {
        query.to_owned()
    };

    query = filtered_query(search_ignored_words, &query);

    for (fb, menu_item) in items.iter_mut() {
        let menu_item_search = format!(
            "{} {}",
            menu_item
                .action
                .as_ref()
                .map(|a| {
                    if config.read().unwrap().insensitive() {
                        a.to_lowercase()
                    } else {
                        a.clone()
                    }
                })
                .unwrap_or_default(),
            if config.read().unwrap().insensitive() {
                menu_item.label.to_lowercase()
            } else {
                menu_item.label.clone()
            }
        );

        let (search_sort_score, visible) = match config.read().unwrap().match_method() {
            MatchMethod::Fuzzy => {
                let mut score = strsim::jaro_winkler(&query, &menu_item_search);
                if score == 0.0 {
                    score = -1.0;
                }

                (
                    score,
                    score > config.read().unwrap().fuzzy_min_score() && score > 0.0,
                )
            }
            MatchMethod::Contains => {
                if menu_item_search.contains(&query) {
                    (1.0, true)
                } else {
                    (0.0, false)
                }
            }
            MatchMethod::MultiContains => {
                let contains = query.split(' ').all(|x| menu_item_search.contains(x));
                (if contains { 1.0 } else { 0.0 }, contains)
            }
            MatchMethod::None => {
                (1.0, true) // items are always shown
            }
        };

        menu_item.search_sort_score = search_sort_score + menu_item.initial_sort_score;
        menu_item.visible = visible;
        fb.set_visible(menu_item.visible);
    }
}

#[must_use]
pub fn filtered_query(search_ignored_words: Option<&Vec<Regex>>, query: &str) -> String {
    let mut query = query.to_owned();
    if let Some(s) = search_ignored_words.as_ref() {
        s.iter().for_each(|rgx| {
            query = rgx.replace_all(&query, "").to_string();
        });
    }
    query
}
enum ChildPosition {
    Front,
    Back,
}

fn find_visible_child<T: Clone>(
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
        if let Some(child) = flow_box.child_at_index(i_32) {
            if child.is_visible() {
                return Some(child);
            }
        }
    }

    None
}

fn select_visible_child<T: Clone>(
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

            items.sort_by(|l, r| into_core_order(sort_menu_items_by_score(Some(l), Some(r))));
        }
    }
}

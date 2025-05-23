use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crossbeam::channel;
use crossbeam::channel::Sender;
use gdk4::Display;
use gdk4::gio::File;
use gdk4::glib::{MainContext, Propagation};
use gdk4::prelude::{Cast, DisplayExt, MonitorExt, SurfaceExt};
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
    pub custom_key: Option<KeyBinding>,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Key {
    None,

    // Letters
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // Numbers
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    // Function Keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Navigation / Editing
    Escape,
    Enter,
    Space,
    Tab,
    Backspace,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,

    // Special characters
    Exclamation,  // !
    At,           // @
    Hash,         // #
    Dollar,       // $
    Percent,      // %
    Caret,        // ^
    Ampersand,    // &
    Asterisk,     // *
    LeftParen,    // (
    RightParen,   // )
    Minus,        // -
    Underscore,   // _
    Equal,        // =
    Plus,         // +
    LeftBracket,  // [
    RightBracket, // ]
    LeftBrace,    // {
    RightBrace,   // }
    Backslash,    // \
    Pipe,         // |
    Semicolon,    // ;
    Colon,        // :
    Apostrophe,   // '
    Quote,        // "
    Comma,        // ,
    Period,       // .
    Slash,        // /
    Question,     // ?
    Grave,        // `
    Tilde,        // ~
}

impl From<gdk::Key> for Key {
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

impl From<gdk4::ModifierType> for Modifier {
    fn from(value: gdk4::ModifierType) -> Self {
        match value {
            gdk4::ModifierType::SHIFT_MASK => Modifier::Shift,
            gdk4::ModifierType::CONTROL_MASK => Modifier::Control,
            gdk4::ModifierType::ALT_MASK => Modifier::Alt,
            gdk4::ModifierType::SUPER_MASK => Modifier::Super,
            gdk4::ModifierType::META_MASK => Modifier::Meta,
            gdk4::ModifierType::LOCK_MASK => Modifier::CapsLock,
            _ => Modifier::None,
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct KeyBinding {
    pub key: Key,
    pub modifiers: Modifier, // todo support masks
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
    custom_keys: Option<Vec<KeyBinding>>,
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
            custom_keys.as_ref(),
        );
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

fn build_ui<T, P>(
    config: &Config,
    item_provider: P,
    sender: Sender<Result<Selection<T>, Error>>,
    app: Application,
    new_on_empty: bool,
    search_ignored_words: Option<Vec<Regex>>,
    custom_keys: Option<&Vec<KeyBinding>>,
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
    setup_key_event_handler(&ui_elements, &meta, custom_keys);

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
    build_custom_key_view(custom_keys, &outer_box);

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

    animate_window.connect_is_active_notify(move |w| {
        w.set_opacity(1.0);
        window_show_resize(&animate_cfg.clone(), w);
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

fn build_custom_key_view(custom_keys: Option<&Vec<KeyBinding>>, outer_box: &gtk4::Box) {
    let inner_box = FlowBox::new();
    inner_box.set_halign(Align::Fill);
    inner_box.set_widget_name("custom-key-box");
    if let Some(custom_keys) = custom_keys {
        for key in custom_keys {
            let label_box = FlowBoxChild::new();
            label_box.set_halign(Align::Fill);
            inner_box.set_valign(Align::Start);
            label_box.set_widget_name("custom-key-label-box");
            inner_box.append(&label_box);
            inner_box.set_vexpand(false);
            inner_box.set_hexpand(false);
            let label = Label::new(Some(&key.label));
            label.set_halign(Align::Fill);
            label.set_valign(Align::Start);
            label.set_use_markup(true);
            label.set_hexpand(true);
            label.set_vexpand(false);
            label.set_widget_name("custom-key-label-text");
            label.set_wrap(false);
            label.set_xalign(0.0);
            label_box.set_child(Some(&label));
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
    custom_keys: Option<&Vec<KeyBinding>>,
) {
    let key_controller = EventControllerKey::new();

    let ui_clone = Rc::clone(ui);
    let meta_clone = Rc::clone(meta);
    let keys_clone = custom_keys.cloned();
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

#[allow(clippy::too_many_lines)] // todo fix this.
fn handle_key_press<T: Clone + 'static + Send>(
    ui: &Rc<UiElements<T>>,
    meta: &Rc<MetaData<T>>,
    keyboard_key: gdk4::Key,
    modifier_type: gdk4::ModifierType,
    custom_keys: Option<&Vec<KeyBinding>>,
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
            if custom_key.key == keyboard_key.into() && custom_key.modifiers == modifier_type.into()
            {
                let search_lock = ui.search_text.lock().unwrap();
                if let Err(e) = handle_selected_item(
                    ui,
                    Rc::<MetaData<T>>::clone(meta),
                    Some(&search_lock),
                    None,
                    meta.new_on_empty,
                    Some(custom_key),
                ) {
                    log::error!("{e}");
                }
            }
        }
    }

    match keyboard_key {
        gdk4::Key::Escape => {
            if let Err(e) = meta.selected_sender.send(Err(Error::NoSelection)) {
                log::error!("failed to send message {e}");
            }
            close_gui(&ui.app);
        }
        gdk4::Key::Return => {
            let search_lock = ui.search_text.lock().unwrap();
            if let Err(e) = handle_selected_item(
                ui,
                Rc::<MetaData<T>>::clone(meta),
                Some(&search_lock),
                None,
                meta.new_on_empty,
                None,
            ) {
                log::error!("{e}");
            }
        }
        gdk4::Key::BackSpace => {
            let mut query = ui.search_text.lock().unwrap().to_string();
            if !query.is_empty() {
                query.pop();

                set_search_text(&query, ui, meta);
                update_view_from_provider(&query);
            }
        }
        gdk4::Key::Tab => {
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
    ui: &Rc<UiElements<T>>,
    meta: Rc<MetaData<T>>,
    query: Option<&str>,
    item: Option<MenuItem<T>>,
    new_on_empty: bool,
    custom_key: Option<&KeyBinding>,
) -> Result<(), String>
where
    T: Clone + Send + 'static,
{
    if let Some(selected_item) = item {
        send_selected_item(ui, meta, custom_key.cloned(), selected_item);
        return Ok(());
    } else if let Some(s) = ui.main_box.selected_children().into_iter().next() {
        let list_items = ui.menu_rows.lock().unwrap();
        let item = list_items.get(&s);
        if let Some(selected_item) = item {
            if selected_item.visible {
                send_selected_item(ui, meta, custom_key.cloned(), selected_item.clone());
                return Ok(());
            }
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

        send_selected_item(ui, meta, custom_key.cloned(), item);
        Ok(())
    } else {
        Err("selected item cannot be resolved".to_owned())
    }
}

fn send_selected_item<T>(
    ui: &Rc<UiElements<T>>,
    meta: Rc<MetaData<T>>,
    custom_key: Option<KeyBinding>,
    selected_item: MenuItem<T>,
) where
    T: Clone + Send + 'static,
{
    let ui_clone = Rc::clone(ui);
    ui.window.connect_hide(move |_| {
        if let Err(e) = meta.selected_sender.send(Ok(Selection {
            menu: selected_item.clone(),
            custom_key: custom_key.clone(),
        })) {
            log::error!("failed to send message {e}");
        }
    });
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
                &click_ui,
                Rc::<MetaData<T>>::clone(&click_meta),
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
    if config.sort_order() == SortOrder::Default {
        return;
    }

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
                    let contains = query.split(' ').all(|x| menu_item_search.contains(x));
                    (if contains { 1.0 } else { 0.0 }, contains)
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

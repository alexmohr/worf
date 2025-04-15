use crate::lib::config;
use crate::lib::config::{Config, MatchMethod};
use anyhow::{Context, anyhow};
use crossbeam::channel;
use crossbeam::channel::Sender;
use gdk4::gio::{File, Menu};
use gdk4::glib::{GString, Propagation, Unichar};
use gdk4::prelude::{Cast, DisplayExt, ListModelExtManual, MonitorExt};
use gdk4::{Display, Key};
use gtk4::prelude::{
    ApplicationExt, ApplicationExtManual, BoxExt, ButtonExt, EditableExt, EntryExt, FileChooserExt,
    FlowBoxChildExt, GestureSingleExt, GtkWindowExt, ListBoxRowExt, NativeExt, OrientableExt,
    WidgetExt,
};
use gtk4::{
    Align, Entry, EventControllerKey, Expander, FlowBox, FlowBoxChild, GestureClick, Image, Label,
    ListBox, ListBoxRow, Ordering, PolicyType, Revealer, ScrolledWindow, SearchEntry, Widget, gdk,
};
use gtk4::{Application, ApplicationWindow, CssProvider, Orientation};
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use hyprland::ctl::output::create;
use hyprland::ctl::plugin::list;
use std::collections::HashMap;

use log::{debug, error, info};
use std::process::exit;
use std::sync::{Arc, Mutex, MutexGuard};

type ArcMenuMap = Arc<Mutex<HashMap<FlowBoxChild, MenuItem>>>;
type MenuItemSender = Sender<Result<MenuItem, anyhow::Error>>;

impl Into<Orientation> for config::Orientation {
    fn into(self) -> Orientation {
        match self {
            config::Orientation::Vertical => Orientation::Vertical,
            config::Orientation::Horizontal => Orientation::Horizontal,
        }
    }
}

impl Into<Align> for config::Align {
    fn into(self) -> Align {
        match self {
            config::Align::Fill => Align::Fill,
            config::Align::Start => Align::Start,
            config::Align::Center => Align::Center,
        }
    }
}

#[derive(Clone)]
pub struct MenuItem {
    pub label: String, // todo support empty label?
    pub icon_path: Option<String>,
    pub action: Option<String>,
    pub sub_elements: Vec<MenuItem>,
    pub working_dir: Option<String>,
    pub initial_sort_score: i64,
    pub search_sort_score: f64,
}

pub fn show(config: Config, elements: Vec<MenuItem>) -> Result<MenuItem, anyhow::Error> {
    if let Some(ref css) = config.style {
        let provider = CssProvider::new();
        let css_file_path = File::for_path(css);
        provider.load_from_file(&css_file_path);
        let display = Display::default().expect("Could not connect to a display");
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let app = Application::builder().application_id("worf").build();
    let (sender, receiver) = channel::bounded(1);

    app.connect_activate(move |app| {
        build_ui(&config, &elements, sender.clone(), app);
    });

    let gtk_args: [&str; 0] = [];
    app.run_with_args(&gtk_args);
    let selection = receiver.recv()?;
    selection
}

fn build_ui(
    config: &Config,
    elements: &Vec<MenuItem>,
    sender: Sender<Result<MenuItem, anyhow::Error>>,
    app: &Application,
) {
    // Create a toplevel undecorated window
    let window = ApplicationWindow::builder()
        .application(app)
        .decorated(false)
        .resizable(false)
        .default_width(20)
        .default_height(20)
        .build();

    window.set_widget_name("window");

    config.normal_window.map(|normal| {
        if !normal {
            // Initialize the window as a layer
            window.init_layer_shell();
            window.set_layer(gtk4_layer_shell::Layer::Overlay);
            window.set_keyboard_mode(KeyboardMode::Exclusive);
            window.set_namespace(Some("worf"));
        }
    });

    let outer_box = gtk4::Box::new(config.orientation.unwrap().into(), 0);
    outer_box.set_widget_name("outer-box");

    window.set_child(Some(&outer_box));

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

    let hide_scroll = false; // todo
    if hide_scroll {
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
    } else {
        inner_box.set_halign(Align::Fill);
    }

    inner_box.set_selection_mode(gtk4::SelectionMode::Browse);
    inner_box.set_max_children_per_line(config.columns.unwrap());
    inner_box.set_activate_on_single_click(true);

    let mut list_items: ArcMenuMap = Arc::new(Mutex::new(HashMap::new()));
    for entry in elements {
        list_items
            .lock()
            .unwrap() // panic here ok? deadlock?
            .insert(
                add_menu_item(
                    &inner_box,
                    &entry,
                    &config,
                    sender.clone(),
                    list_items.clone(),
                    app.clone(),
                ),
                entry.clone(),
            );
    }

    let items_clone = list_items.clone();
    inner_box.set_sort_func(move |child1, child2| sort_menu_items(child1, child2, &items_clone));

    // Set focus after everything is realized
    inner_box.connect_map(|fb| {
        fb.grab_focus();
        fb.invalidate_sort();
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
        list_items.clone(),
        config.clone(),
    );

    window.show();

    let display = window.display();
    window.surface().map(|surface| {
        // todo this does not work for multi monitor systems
        let monitor = display.monitor_at_surface(&surface);
        if let Some(monitor) = monitor {
            let geometry = monitor.geometry();
            config.width.as_ref().map(|width| {
                percent_or_absolute(&width, geometry.width()).map(|w| window.set_width_request(w))
            });
            config.height.as_ref().map(|height| {
                percent_or_absolute(&height, geometry.height())
                    .map(|h| window.set_height_request(h))
            });
        } else {
            log::error!("failed to get monitor to init window size");
        }
    });
}

fn setup_key_event_handler(
    window: &ApplicationWindow,
    entry_clone: SearchEntry,
    inner_box: FlowBox,
    app: Application,
    sender: MenuItemSender,
    list_items: Arc<Mutex<HashMap<FlowBoxChild, MenuItem>>>,
    config: Config,
) {
    let key_controller = EventControllerKey::new();

    key_controller.connect_key_pressed(move |_, key_value, _, _| {
        match key_value {
            Key::Escape => {
                if let Err(e) = sender.send(Err(anyhow!("No item selected"))) {
                    log::error!("failed to send message {e}");
                }
                app.quit();
            }
            Key::Return => {
                if let Err(e) = handle_selected_item(&sender, &app, &inner_box, &list_items) {
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

fn sort_menu_items(
    child1: &FlowBoxChild,
    child2: &FlowBoxChild,
    items_lock: &Mutex<HashMap<FlowBoxChild, MenuItem>>,
) -> Ordering {
    let lock = items_lock.lock().unwrap();
    let m1 = lock.get(child1);
    let m2 = lock.get(child2);

    match (m1, m2) {
        (Some(menu1), Some(menu2)) => {
            if menu1.search_sort_score != 0.0 || menu2.search_sort_score != 0.0 {
                if menu1.search_sort_score > menu2.search_sort_score {
                    Ordering::Smaller
                } else {
                    Ordering::Larger
                }
            } else {
                if menu1.initial_sort_score > menu2.initial_sort_score {
                    Ordering::Smaller
                } else {
                    Ordering::Larger
                }
            }
        }
        (Some(_), None) => Ordering::Larger,
        (None, Some(_)) => Ordering::Smaller,
        (None, None) => Ordering::Equal,
    }
}

fn handle_selected_item(
    sender: &MenuItemSender,
    app: &Application,
    inner_box: &FlowBox,
    lock_arc: &ArcMenuMap,
) -> Result<(), String> {
    for s in inner_box.selected_children() {
        let list_items = lock_arc.lock().unwrap();
        let item = list_items.get(&s);
        if let Some(item) = item {
            if let Err(e) = sender.send(Ok(item.clone())) {
                log::error!("failed to send message {e}");
            }
        }
        app.quit();
        return Ok(());
    }
    Err("selected item cannot be resolved".to_owned())
}

fn add_menu_item(
    inner_box: &FlowBox,
    entry_element: &MenuItem,
    config: &Config,
    sender: MenuItemSender,
    lock_arc: ArcMenuMap,
    app: Application,
) -> FlowBoxChild {
    let parent: Widget = if !entry_element.sub_elements.is_empty() {
        let expander = Expander::new(None);
        expander.set_widget_name("expander-box");
        expander.set_hexpand(true);

        let menu_row = create_menu_row(
            entry_element,
            config,
            lock_arc.clone(),
            sender.clone(),
            app.clone(),
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
                lock_arc.clone(),
                sender.clone(),
                app.clone(),
                inner_box.clone(),
            );
            sub_row.set_hexpand(true);
            sub_row.set_halign(Align::Fill);
            sub_row.set_widget_name("entry");
            list_box.append(&sub_row);
        }

        expander.set_child(Some(&list_box));
        expander.upcast()
    } else {
        create_menu_row(
            entry_element,
            config,
            lock_arc.clone(),
            sender.clone(),
            app.clone(),
            inner_box.clone(),
        )
        .upcast()
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

fn create_menu_row(
    menu_item: &MenuItem,
    config: &Config,
    lock_arc: ArcMenuMap,
    sender: MenuItemSender,
    app: Application,
    inner_box: FlowBox,
) -> Widget {
    let row = ListBoxRow::new();
    row.set_hexpand(true);
    row.set_halign(Align::Fill);
    row.set_widget_name("row");

    let click = GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    click.connect_pressed(move |_gesture, n_press, _x, _y| {
        if n_press == 2 {
            if let Err(e) = handle_selected_item(&sender, &app, &inner_box, &lock_arc) {
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

    let label = Label::new(Some(&menu_item.label));
    label.set_hexpand(true);
    row_box.append(&label);

    if config.content_halign.unwrap() == config::Align::Start
        || config.content_halign.unwrap() == config::Align::Fill
    {
        label.set_xalign(0.0);
    }
    row.upcast()
}

fn filter_widgets(
    query: &str,
    items: &mut HashMap<FlowBoxChild, MenuItem>,
    config: &Config,
    inner_box: &FlowBox,
) {
    if items.is_empty() {
        items.iter().for_each(|(child, _)| {
            child.set_visible(true);
        });
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
    let mut highest_score = -1.0;
    let mut fb: Option<&FlowBoxChild> = None;
    items.iter_mut().for_each(|(flowbox_child, mut menu_item)| {
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
        if visible {
            highest_score = search_sort_score;
            fb = Some(flowbox_child);
        }

        flowbox_child.set_visible(visible);
    });

    if let Some(top_item) = fb {
        inner_box.select_child(top_item);
        top_item.grab_focus();
    }
}

fn percent_or_absolute(value: &String, base_value: i32) -> Option<i32> {
    if value.contains("%") {
        let value = value.replace("%", "").trim().to_string();
        match value.parse::<i32>() {
            Ok(n) => Some(((n as f32 / 100.0) * base_value as f32) as i32),
            Err(_) => None,
        }
    } else {
        value.parse::<i32>().ok()
    }
}

pub fn initialize_sort_scores(items: &mut Vec<MenuItem>) {
    let mut regular_score = items.len() as i64;
    items.sort_by(|l, r| r.label.cmp(&l.label));

    for item in items.iter_mut() {
        if item.initial_sort_score == 0 {
            item.initial_sort_score = regular_score;
            regular_score += 1;
        }
    }
}

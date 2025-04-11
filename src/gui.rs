use crate::config::Config;
use anyhow::{Context, anyhow};
use crossbeam::channel;
use crossbeam::channel::Sender;
use gdk4::gio::File;
use gdk4::glib::Propagation;
use gdk4::prelude::{Cast, DisplayExt, MonitorExt};
use gdk4::{Display, Key};
use gtk4::prelude::{
    ApplicationExt, ApplicationExtManual, BoxExt, ButtonExt, EditableExt, EntryExt, FileChooserExt,
    FlowBoxChildExt, GtkWindowExt, ListBoxRowExt, NativeExt, WidgetExt,
};
use gtk4::{Align, EventControllerKey, Expander, FlowBox, FlowBoxChild, Image, Label, ListBox, ListBoxRow, PolicyType, ScrolledWindow, SearchEntry, Widget};
use gtk4::{Application, ApplicationWindow, CssProvider, Orientation};
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use log::{debug, error, info};
use std::process::exit;
use hyprland::ctl::output::create;
use hyprland::ctl::plugin::list;

#[derive(Clone)]
pub struct MenuItem {
    pub label: String, // todo support empty label?
    pub icon_path: Option<String>,
    pub action: Option<String>,
    pub sub_elements: Vec<MenuItem>,
}

pub fn show(config: Config, elements: Vec<MenuItem>) -> anyhow::Result<(i32)> {
    // Load CSS
    let provider = CssProvider::new();
    let css_file_path = File::for_path("/home/me/.config/wofi/style.css");

    provider.load_from_file(&css_file_path);
    // Apply CSS to the display
    let display = Display::default().expect("Could not connect to a display");
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let display = Display::default().expect("Could not connect to a display");
    // Apply CSS to the display
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // No need for application_id unless you want portal support
    let app = Application::builder().application_id("worf").build();
    let (sender, receiver) = channel::bounded(1);

    app.connect_activate(move |app| {
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
                window.set_layer(gtk4_layer_shell::Layer::Overlay);
                window.init_layer_shell();
                window.set_keyboard_mode(KeyboardMode::Exclusive);
                window.set_namespace(Some("worf"));
            }
        });

        let outer_box = gtk4::Box::new(Orientation::Vertical, 0);
        outer_box.set_widget_name("outer-box");
        window.set_child(Some(&outer_box));

        let entry = SearchEntry::new();
        entry.set_widget_name("input");
        entry.set_css_classes(&["input"]);
        entry.set_placeholder_text(config.prompt.as_deref());

        // Example `search` and `password_char` usage
        // let password_char = Some('*');
        // todo\
        // if let Some(c) = password_char {
        //     let entry_casted: Entry = entry.clone().upcast();
        //     entry_casted.set_visibility(false);
        //     entry_casted.set_invisible_char(c);
        // }

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

        inner_box.set_selection_mode(gtk4::SelectionMode::Browse);
        inner_box.set_max_children_per_line(1); // todo change to `columns` variable
        //inner_box.set_orientation(Orientation::Horizontal); // or Vertical
        inner_box.set_halign(Align::Fill);
        inner_box.set_valign(Align::Start);
        inner_box.set_activate_on_single_click(true);

        for entry in &elements {
            add_menu_item(&inner_box, &entry);
        }

        // Set focus after everything is realized
        inner_box.connect_map(|fb| {
            fb.grab_focus();
        });

        let wrapper_box = gtk4::Box::new(Orientation::Vertical, 0);
        wrapper_box.set_homogeneous(true);
        wrapper_box.append(&inner_box);
        scroll.set_child(Some(&wrapper_box));

        // todo implement search function
        // // Dummy filter and sort funcs – replace with actual logic
        // inner_box.set_filter_func(Some(Box::new(|_child| {
        //     true // filter logic here
        // })));
        // inner_box.set_sort_func(Some(Box::new(|child1, child2| {
        //     child1.widget_name().cmp(&child2.widget_name())
        // })));

        // Create key event controller
        let entry_clone = entry.clone();
        setup_key_event_handler(&window, entry_clone, inner_box, app.clone(), sender.clone());

        window.show();

        // Get the display where the window resides
        let display = window.display();

        // Get the monitor that the window is on (use window's coordinates to find this)
        window.surface().map(|surface| {
            let monitor = display.monitor_at_surface(&surface);
            if let Some(monitor) = monitor {
                let geometry = monitor.geometry();
                config.width.as_ref().map(|width| {
                    percent_or_absolute(&width, geometry.width())
                        .map(|w| window.set_width_request(w))
                });
                config.height.as_ref().map(|height| {
                    percent_or_absolute(&height, geometry.height())
                        .map(|h| window.set_height_request(h))
                });
            } else {
                error!("failed to get monitor to init window size");
            }
        });
    });

    let empty_array: [&str; 0] = [];

    app.run_with_args(&empty_array);
    let selected_index = receiver.recv()?;
    Ok(selected_index)
}

fn setup_key_event_handler(
    window: &ApplicationWindow,
    entry_clone: SearchEntry,
    inner_box: FlowBox,
    app: Application,
    sender: Sender<i32>,
) {
    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(move |_, key_value, _, _| {
        match key_value {
            Key::Escape => exit(1), // todo better way to do this?
            Key::Return => {
                for s in &inner_box.selected_children() {
                    // let element : &Option<&EntryElement> = &elements.get(s.index() as usize);
                    // if let Some(element) = *element {
                    //     debug!("Running action on element with name {}", element.label);
                    //     (element.action)();
                    // }
                    if let Err(e) = sender.send(s.index()) {
                        error!("failed to send selected child {e:?}")
                    }
                    app.quit();
                }
            }
            _ => {
                if let Some(c) = key_value.name() {
                    // Only proceed if it's a single alphanumeric character
                    if c.len() == 1 && c.chars().all(|ch| ch.is_alphanumeric()) {
                        let current = entry_clone.text().to_string();
                        entry_clone.set_text(&format!("{current}{c}"));
                    }
                }
            }
        }

        Propagation::Proceed
    });
    // Add the controller to the window
    window.add_controller(key_controller);
}

fn add_menu_item(inner_box: &FlowBox, entry_element: &MenuItem) {
    let parent: Widget = if !entry_element.sub_elements.is_empty() {
        let expander = Expander::new(None);
        expander.set_widget_name("expander-box");
        expander.set_halign(Align::Fill);

        let menu_row = create_menu_row(entry_element);
        expander.set_label_widget(Some(&menu_row));

        let list_box = ListBox::new();
        list_box.set_widget_name("entry");

        // todo multi nesting is not supported yet.
        for sub_item in entry_element.sub_elements.iter(){
            list_box.append(&create_menu_row(sub_item));
        }

        expander.set_child(Some(&list_box));
        expander.upcast()
    } else {
        create_menu_row(entry_element).upcast()
    };

    parent.set_halign(Align::Start);

    let child = FlowBoxChild::new();
    child.set_widget_name("entry");
    child.set_child(Some(&parent));

    inner_box.append(&child);
}

fn create_menu_row(menu_item: &MenuItem) -> Widget {
    let row = ListBoxRow::new();
    row.set_widget_name("entry");
    row.set_hexpand(true);
    row.set_halign(Align::Start);

    let row_box = gtk4::Box::new(Orientation::Horizontal, 0);
    row.set_child(Some(&row_box));

    if let Some(image_path) = &menu_item.icon_path {
        // todo check config too
        let image = Image::from_icon_name(image_path);
        image.set_pixel_size(24);
        image.set_widget_name("img");
        row_box.append(&image);
    }

    let label = Label::new(Some(&menu_item.label));

    label.set_widget_name("unselected");
    row_box.append(&label);


    row.upcast()
}

fn percent_or_absolute(value: &String, base_value: i32) -> Option<i32> {
    if value.contains("%") {
        let value = value.replace("%", "");
        let value = value.trim();
        match value.parse::<i32>() {
            Ok(n) => {
                let result = ((n as f32 / 100.0) * base_value as f32) as i32;
                Some(result)
            }
            Err(_) => None,
        }
    } else {
        value.parse::<i32>().ok()
    }
}

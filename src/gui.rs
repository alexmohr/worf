use crate::config::Config;
use anyhow::Context;
use gdk4::Display;
use gdk4::gio::File;
use gdk4::glib::Propagation;
use gdk4::prelude::{Cast, DisplayExt, MonitorExt};
use gtk4::prelude::{
    ApplicationExt, ApplicationExtManual, BoxExt, ButtonExt, EditableExt, EntryExt,
    FlowBoxChildExt, GtkWindowExt, ListBoxRowExt, NativeExt, WidgetExt,
};
use gtk4::{
    Align, EventControllerKey, Expander, FlowBox, FlowBoxChild, Label, ListBox, ListBoxRow,
    PolicyType, ScrolledWindow, SearchEntry, Widget,
};
use gtk4::{Application, ApplicationWindow, CssProvider, Orientation};
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use log::error;
use std::process::exit;

pub struct EntryElement {
    pub label: String, // todo support empty label?
    pub icon_path: Option<String>,
    pub action: Box<dyn Fn() + Send + 'static>,
    pub sub_elements: Option<Vec<EntryElement>>,
}

pub fn init(config: Config, elements: Vec<EntryElement>) -> anyhow::Result<()> {
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
    let app = Application::builder().application_id("ravi").build();

    app.connect_activate(move |app| {
        // Create a toplevel undecorated window
        let window = ApplicationWindow::builder()
            .application(app)
            .decorated(false)
            .resizable(false)
            .default_width(20)
            .default_height(20)
            .build();

        window.init_layer_shell();
        window.set_keyboard_mode(KeyboardMode::Exclusive);
        window.set_widget_name("window");
        window.set_layer(gtk4_layer_shell::Layer::Overlay);
        window.set_namespace(Some("ravi"));

        let outer_box = gtk4::Box::new(Orientation::Vertical, 0);
        outer_box.set_widget_name("outer-box");
        window.set_child(Some(&outer_box));

        let entry = SearchEntry::new();
        entry.set_widget_name("input");
        entry.set_css_classes(&["input"]);
        entry.set_placeholder_text(Some("Enter search..."));

        // Create key event controller
        let entry_clone = entry.clone();
        setup_key_event_handler(&window, entry_clone);

        // Example `search` and `password_char` usage
        let password_char = Some('*');

        entry.set_placeholder_text(Some("placeholder"));

        // todo
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
            add_entry_element(&inner_box, &entry);
        }

        // todo
        // Set focus after everything is realized
        inner_box.connect_map(|fb| {
            fb.grab_focus();
        });

        let wrapper_box = gtk4::Box::new(Orientation::Vertical, 0);
        wrapper_box.set_homogeneous(true);
        wrapper_box.append(&inner_box);
        scroll.set_child(Some(&wrapper_box));

        // todo
        // // Dummy filter and sort funcs – replace with actual logic
        // inner_box.set_filter_func(Some(Box::new(|_child| {
        //     true // filter logic here
        // })));

        // todo
        // inner_box.set_sort_func(Some(Box::new(|child1, child2| {
        //     child1.widget_name().cmp(&child2.widget_name())
        // })));

        window.show();

        // Get the display where the window resides
        let display = window.display();

        // Get the monitor that the window is on (use window's coordinates to find this)
        window.surface().map(|surface| {
            let monitor = display.monitor_at_surface(&surface);
            if let Some(monitor) = monitor {
                let geometry = monitor.geometry();
                if let Some(w) = percent_or_absolute(
                    &config.width.clone().unwrap_or("800".to_owned()),
                    geometry.width(),
                ) {
                    window.set_width_request(w);
                }
                if let Some(h) = percent_or_absolute(
                    &config.height.clone().unwrap_or("500".to_owned()),
                    geometry.height(),
                ) {
                    window.set_height_request(h);
                }
            } else {
                error!("failed to get monitor to init window size");
            }
        });
    });

    let empty_array: [&str; 0] = [];

    app.run_with_args(&empty_array);
    Ok(())
}

fn setup_key_event_handler(window: &ApplicationWindow, entry_clone: SearchEntry) {
    let key_controller = EventControllerKey::new();
    let x = key_controller.connect_key_pressed(move |_controller, key_value, code, mode| {
        if code == 9 {
            // todo find better way to handle escape
            exit(1);
        }

        if let Some(c) = key_value.name() {
            // Only proceed if it's a single alphanumeric character
            if c.len() == 1 && c.chars().all(|ch| ch.is_alphanumeric()) {
                let current = entry_clone.text().to_string();
                entry_clone.set_text(&format!("{current}{c}"));
            }
        }
        Propagation::Proceed
    });
    // Add the controller to the window
    window.add_controller(key_controller);
}

fn add_entry_element(inner_box: &gtk4::FlowBox, entry_element: &EntryElement) {
    let parent: Widget = if entry_element.sub_elements.is_some() {
        let expander = Expander::new(None);

        // Inline label as expander label
        let label = Label::new(Some(&entry_element.label));
        expander.set_label_widget(Some(&label));

        let list_box = ListBox::new();
        // todo subelements do not fill full space yet.
        // todo multi nesting is not supported yet.

        for x in entry_element.sub_elements.iter().flatten() {
            let row = ListBoxRow::new();
            row.set_widget_name("entry");

            let label = Label::new(Some(&x.label));
            row.set_child(Some(&label));
            list_box.append(&row);
        }

        expander.set_child(Some(&list_box));
        expander.upcast()
    } else {
        Label::new(Some(&entry_element.label)).upcast()
    };

    parent.set_halign(Align::Start);

    let child = FlowBoxChild::new();
    child.set_widget_name("entry");
    child.set_child(Some(&parent));

    inner_box.append(&child);
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

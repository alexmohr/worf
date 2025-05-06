use anyhow::anyhow;
use std::collections::{HashMap, HashSet};
use std::env;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use worf_lib::config::Config;
use worf_lib::desktop::spawn_fork;
use worf_lib::gui::{ItemProvider, Key, KeyBinding, MenuItem, Modifier};
use worf_lib::{config, gui};

#[derive(Clone)]
struct MenuItemMetaData {
    ids: Vec<String>,
}

#[derive(Clone)]
struct PasswordProvider {
    items: Vec<MenuItem<MenuItemMetaData>>,
}

fn split_at_tab(input: &str) -> Option<(&str, &str)> {
    let mut parts = input.splitn(2, '\t');
    Some((parts.next()?, parts.next()?))
}

impl PasswordProvider {
    fn new(config: &Config) -> Self {
        let output = Command::new("rbw")
            .arg("list")
            .arg("--fields")
            .arg("id,name")
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut items = stdout
            .lines()
            .filter_map(|s| split_at_tab(s))
            .fold(
                HashMap::new(),
                |mut acc: HashMap<String, Vec<String>>, (id, name)| {
                    acc.entry(name.to_owned()).or_default().push(id.to_owned());
                    acc
                },
            )
            .iter()
            .map(|(key, value)| {
                MenuItem::new(
                    key.clone(),
                    None,
                    None,
                    vec![],
                    None,
                    0.0,
                    Some(MenuItemMetaData {
                        ids: value.clone(),
                    }),
                )
            })
            .collect::<Vec<_>>();

        gui::apply_sort(&mut items, &config.sort_order());

        Self { items }
    }

    fn sub_provider(ids: Vec<String>) -> Self {
        Self {
            items: ids
                .iter()
                .map(|id| {
                    MenuItem::new(
                        rbw_get_user(id),
                        None,
                        None,
                        vec![],
                        None,
                        0.0,
                        Some(MenuItemMetaData {
                            ids: vec![id.clone()],
                        }),
                    )
                })
                .collect::<Vec<_>>(),
        }
    }
}

impl ItemProvider<MenuItemMetaData> for PasswordProvider {
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<MenuItemMetaData>>) {
        (false, self.items.clone())
    }

    fn get_sub_elements(
        &mut self,
        _: &MenuItem<MenuItemMetaData>,
    ) -> (bool, Option<Vec<MenuItem<MenuItemMetaData>>>) {
        (false, None)
    }
}

fn groups() -> String {
    let output = Command::new("groups")
        .output()
        .expect("Failed to get groups");
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

fn keyboard_type(text: &str) {
    Command::new("ydotool")
        .arg("type")
        .arg(text)
        .output()
        .expect("Failed to execute ydotool");
}

fn keyboard_tab() {
    Command::new("ydotool")
        .arg("key")
        .arg("-d")
        .arg("10")
        .arg("15:1")
        .arg("15:0")
        .output()
        .expect("Failed to execute ydotool");
}

fn rbw_get(id: &str, field: &str) -> String {
    let output = Command::new("rbw")
        .arg("get")
        .arg(id)
        .arg("--field")
        .arg(field)
        .output()
        .expect("Failed to execute command");

    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

fn rbw_get_user(id: &str) -> String {
    rbw_get(id, "user")
}

fn rbw_get_password(id: &str) -> String {
    rbw_get(id, "password")
}

fn rbw_get_totp(id: &str) -> String {
    rbw_get(id, "totp")
}

fn key_type_all() -> KeyBinding {
    KeyBinding {
        key: Key::Num1,
        modifiers: Modifier::Alt,
        label: "<b>Alt+1</b> Type User".to_string(),
    }
}

fn key_type_user() -> KeyBinding {
    KeyBinding {
        key: Key::Num2,
        modifiers: Modifier::Alt,
        label: "<b>Alt+2</b> Type User".to_string(),
    }
}

fn key_type_password() -> KeyBinding {
    KeyBinding {
        key: Key::Num3,
        modifiers: Modifier::Alt,
        label: "<b>Alt+3</b> Type Password".to_string(),
    }
}

fn key_type_totp() -> KeyBinding {
    KeyBinding {
        key: Key::Num4,
        modifiers: Modifier::Alt,
        label: "<b>Alt+3</b> Type Totp".to_string(),
    }
}

fn key_reload() -> KeyBinding {
    KeyBinding {
        key: Key::R,
        modifiers: Modifier::Alt,
        label: "<b>Alt+r</b> Sync".to_string(),
    }
}

fn key_urls() -> KeyBinding {
    KeyBinding {
        key: Key::U,
        modifiers: Modifier::Alt,
        label: "<b>Alt+u</b> Sync".to_string(),
    }
}

fn key_names() -> KeyBinding {
    KeyBinding {
        key: Key::N,
        modifiers: Modifier::Alt,
        label: "<b>Alt+n</b> Sync".to_string(),
    }
}

fn key_folders() -> KeyBinding {
    KeyBinding {
        key: Key::C,
        modifiers: Modifier::Alt,
        label: "<b>Alt+c</b> Sync".to_string(),
    }
}

fn key_totp() -> KeyBinding {
    KeyBinding {
        key: Key::T,
        modifiers: Modifier::Alt,
        label: "<b>Alt+t</b> Sync".to_string(),
    }
}

fn key_lock() -> KeyBinding {
    KeyBinding {
        key: Key::L,
        modifiers: Modifier::Alt,
        label: "<b>Alt+l</b> Sync".to_string(),
    }
}

fn show(config: Config, provider: PasswordProvider) -> anyhow::Result<()> {
    match gui::show(
        config.clone(),
        provider,
        false,
        None,
        Some(vec![
            key_type_all(),
            key_type_user(),
            key_type_password(),
            key_type_totp(),
            key_reload(),
            key_urls(),
            key_names(),
            key_folders(),
            key_totp(),
            key_lock(),
        ]),
    ) {
        Ok(selection) => {
            if let Some(meta) = selection.menu.data {
                if meta.ids.len() > 1 {
                    return show(config, PasswordProvider::sub_provider(meta.ids));
                }

                let id = meta.ids.iter().next().unwrap_or(&selection.menu.label);

                sleep(Duration::from_millis(250));
                if let Some(key) = selection.custom_key {
                    if key == key_type_all() {
                        keyboard_type(&rbw_get_user(&id));
                        keyboard_tab();
                        keyboard_type(&rbw_get_password(&id));
                    } else if key == key_type_user() {
                        keyboard_type(&rbw_get_user(&id));
                    } else if key == key_type_password() {
                        keyboard_type(&rbw_get_password(&id));
                    } else if key == key_type_totp() {
                        keyboard_type(&rbw_get_totp(&id));
                    }
                }
                Ok(())
            } else {
                Err(anyhow!("missing meta data"))
            }
        }
        Err(e) => return Err(anyhow::anyhow!(e)),
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let args = config::parse_args();
    let config = config::load_config(Some(&args)).unwrap_or(args);

    if !groups().contains("input") {
        eprintln!("User must be in input group. 'sudo usermod -aG input $USER', then login again");
        std::process::exit(1)
    }

    // will exit if there is a daemon running already, so it's fine to call this everytime.
    spawn_fork("ydotoold", None).expect("failed to spawn ydotoold");

    // todo eventually use a propper rust client for this, for now rbw is good enough
    let provider = PasswordProvider::new(&config);

    show(config, provider)
}

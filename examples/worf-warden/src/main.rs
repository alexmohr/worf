use std::collections::HashMap;
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
        let output = rbw("list", Some(vec!["--fields", "id,name"]));
        let items = match output {
            Ok(output) => {
                let mut items = output
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
                            Some(MenuItemMetaData { ids: value.clone() }),
                        )
                    })
                    .collect::<Vec<_>>();
                gui::apply_sort(&mut items, &config.sort_order());
                items
            }
            Err(error) => {
                let item = MenuItem::new(
                    format!("Error from rbw: {error}"),
                    None,
                    None,
                    vec![],
                    None,
                    0.0,
                    None,
                );
                vec![item]
            }
        };

        Self { items }
    }

    fn sub_provider(ids: Vec<String>) -> Result<Self, String> {
        let items = ids
            .iter()
            .map(|id| {
                Ok(MenuItem::new(
                    rbw_get_user(id, false)?,
                    None,
                    None,
                    vec![],
                    None,
                    0.0,
                    Some(MenuItemMetaData {
                        ids: vec![id.clone()],
                    }),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(Self { items })
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
        .arg("TAB")
        .output()
        .expect("Failed to execute ydotool");
}

fn rbw(cmd: &str, args: Option<Vec<&str>>) -> Result<String, String> {
    let mut command = Command::new("rbw");
    command.arg(cmd);

    if let Some(args) = args {
        for arg in args {
            command.arg(arg);
        }
    }

    let output = command
        .output()
        .map_err(|e| format!("Failed to execute command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("rbw command failed: {}", stderr.trim()));
    }

    let stdout =
        String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8 output: {}", e))?;

    Ok(stdout.trim().to_string())
}

fn rbw_get(id: &str, field: &str, copy: bool) -> Result<String, String> {
    let mut args = vec![id, "--field", field];
    if copy {
        args.push("--clipboard");
    }
    rbw("get", Some(args))
}

fn rbw_get_user(id: &str, copy: bool) -> Result<String, String> {
    rbw_get(id, "user", copy)
}

fn rbw_get_password(id: &str, copy: bool) -> Result<String, String> {
    rbw_get(id, "password", copy)
}

fn rbw_get_totp(id: &str, copy: bool) -> Result<String, String> {
    rbw_get(id, "totp", copy)
}

fn key_type_all() -> KeyBinding {
    KeyBinding {
        key: Key::Num1,
        modifiers: Modifier::Alt,
        label: "<b>Alt+1</b> Type All".to_string(),
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
        label: "<b>Alt+4</b> Type Totp".to_string(),
    }
}

fn key_sync() -> KeyBinding {
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
        label: "<b>Alt+u</b> Urls".to_string(),
    }
}

fn key_names() -> KeyBinding {
    KeyBinding {
        key: Key::N,
        modifiers: Modifier::Alt,
        label: "<b>Alt+n</b> NAmes".to_string(),
    }
}

fn key_folders() -> KeyBinding {
    KeyBinding {
        key: Key::C,
        modifiers: Modifier::Alt,
        label: "<b>Alt+c</b> Folders".to_string(),
    }
}

/// copies totp to clipboard
fn key_totp() -> KeyBinding {
    KeyBinding {
        key: Key::T,
        modifiers: Modifier::Alt,
        label: "<b>Alt+t</b> Totp".to_string(),
    }
}

fn key_lock() -> KeyBinding {
    KeyBinding {
        key: Key::L,
        modifiers: Modifier::Alt,
        label: "<b>Alt+l</b> Lock".to_string(),
    }
}

fn show(config: Config, provider: PasswordProvider) -> Result<(), String> {
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
            key_sync(),
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
                    return show(config, PasswordProvider::sub_provider(meta.ids)?);
                }

                let id = meta.ids.first().unwrap_or(&selection.menu.label);

                sleep(Duration::from_millis(250));
                if let Some(key) = selection.custom_key {
                    if key == key_type_all() {
                        keyboard_type(&rbw_get_user(id, false)?);
                        keyboard_tab();
                        keyboard_type(&rbw_get_password(id, false)?);
                    } else if key == key_type_user() {
                        keyboard_type(&rbw_get_user(id, false)?);
                    } else if key == key_type_password() {
                        keyboard_type(&rbw_get_password(id, false)?);
                    } else if key == key_type_totp() {
                        keyboard_type(&rbw_get_totp(id, false)?);
                    } else if key == key_lock() {
                        rbw("lock", None)?;
                    } else if key == key_sync() {
                        rbw("sync", None)?;
                    } else if key == key_urls() {
                        todo!("key urls");
                    } else if key == key_names() {
                        todo!("key names");
                    } else if key == key_folders() {
                        todo!("key folders");
                    } else if key == key_totp() {
                        rbw_get_totp(id, true)?;
                    }
                } else {
                    rbw_get_password(id, true)?;
                }
                Ok(())
            } else {
                Err("missing meta data".to_owned())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn main() -> Result<(), String> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let args = config::parse_args();
    let config = config::load_config(Some(&args)).unwrap_or(args);

    if !groups().contains("input") {
        log::error!("User must be in input group. 'sudo usermod -aG input $USER', then login again");
        std::process::exit(1)
    }

    // will exit if there is a daemon running already, so it's fine to call this everytime.
    spawn_fork("ydotoold", None).expect("failed to spawn ydotoold");

    // todo eventually use a propper rust client for this, for now rbw is good enough
    let provider = PasswordProvider::new(&config);

    show(config, provider)
}

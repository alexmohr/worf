use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use worf_lib::config::Config;
use worf_lib::desktop::spawn_fork;
use worf_lib::gui::{ItemProvider, Key, KeyBinding, MenuItem, Modifier};
use worf_lib::{config, gui};

#[derive(Clone)]
struct PasswordProvider {
    items: Vec<MenuItem<String>>,
}

impl PasswordProvider {
    fn new(config: &Config) -> Self {
        let output = Command::new("rbw")
            .arg("list")
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);

        // todo the own solution should support images.
        let mut items: Vec<_> = stdout
            .lines()
            .map(|line| {
                MenuItem::new(
                    line.to_owned(),
                    None,
                    None,
                    vec![],
                    None,
                    0.0,
                    Some(String::new()),
                )
            })
            .collect();
        gui::apply_sort(&mut items, &config.sort_order());

        Self { items }
    }
}

impl ItemProvider<String> for PasswordProvider {
    fn get_elements(&mut self, _: Option<&str>) -> (bool, Vec<MenuItem<String>>) {
        (false, self.items.clone())
    }

    fn get_sub_elements(&mut self, _: &MenuItem<String>) -> (bool, Option<Vec<MenuItem<String>>>) {
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

fn rbw_get(name: &str, field: &str) -> String {
    let output = Command::new("rbw")
        .arg("get")
        .arg(name)
        .arg("--field")
        .arg(field)
        .output()
        .expect("Failed to execute command");

    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

fn rbw_get_user(name: &str) -> String {
    rbw_get(name, "user")
}

fn rbw_get_password(name: &str) -> String {
    rbw_get(name, "password")
}

fn rbw_get_totp(name: &str) -> String {
    rbw_get(name, "totp")
}

fn main() -> anyhow::Result<()> {

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

    let type_all = KeyBinding {
        key: Key::Num1,
        modifiers: Modifier::Alt,
        label: "<b>Alt+1</b> Type All".to_string(),
    };

    let type_user = KeyBinding {
        key: Key::Num2,
        modifiers: Modifier::Alt,
        label: "<b>Alt+2</b> Type User".to_string(),
    };

    let type_password = KeyBinding {
        key: Key::Num3,
        modifiers: Modifier::Alt,
        label: "<b>Alt+3</b> Type Password".to_string(),
    };

    let type_totp = KeyBinding {
        key: Key::Num4,
        modifiers: Modifier::Alt,
        label: "<b>Alt+3</b> Type Totp".to_string(),
    };

    let reload = KeyBinding {
        key: Key::R,
        modifiers: Modifier::Alt,
        label: "<b>Alt+r</b> Sync".to_string(),
    };

    let urls = KeyBinding {
        key: Key::U, // switch view to urls
        modifiers: Modifier::Alt,
        label: "<b>Alt+u</b> Sync".to_string(),
    };

    let names = KeyBinding {
        key: Key::N, // switch view to names
        modifiers: Modifier::Alt,
        label: "<b>Alt+n</b> Sync".to_string(),
    };

    let folders = KeyBinding {
        key: Key::C, // switch view to folders
        modifiers: Modifier::Alt,
        label: "<b>Alt+c</b> Sync".to_string(),
    };

    let totp = KeyBinding {
        key: Key::T,
        modifiers: Modifier::Alt, // switch view to totp
        label: "<b>Alt+t</b> Sync".to_string(),
    };

    let lock = KeyBinding {
        key: Key::L,
        modifiers: Modifier::Alt,
        label: "<b>Alt+l</b> Sync".to_string(),
    };

    match gui::show(
        config,
        provider,
        false,
        None,
        Some(vec![
            type_all.clone(),
            type_user.clone(),
            type_password.clone(),
            type_totp.clone(),
            reload.clone(),
            urls.clone(),
            names.clone(),
            folders.clone(),
            totp.clone(),
            lock.clone(),
        ]),
    ) {
        Ok(selection) => {
            let id = selection.menu.label.replace("\n", "");
            sleep(Duration::from_millis(250));
            if let Some(key) = selection.custom_key {
                if key == type_all {
                    keyboard_type(&rbw_get_user(&id));
                    keyboard_tab();
                    keyboard_type(&rbw_get_password(&id));
                } else if key == type_user {
                    keyboard_type(&rbw_get_user(&id));
                } else if key == type_password {
                    keyboard_type(&rbw_get_password(&id));
                } else if key == type_totp {
                    keyboard_type(&rbw_get_totp(&id));
                }
            }
        }
        Err(e) => return Err(anyhow::anyhow!(e)),
    }
    Ok(())
}

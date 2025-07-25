use std::{
    collections::HashMap,
    env,
    process::Command,
    sync::{Arc, Mutex, RwLock},
    thread::sleep,
    time::Duration,
};

use clap::Parser;
use serde::{Deserialize, Serialize};
use worf::{
    config::{self, Config, CustomKeyHintLocation, Key},
    desktop::{copy_to_clipboard, spawn_fork},
    gui::{
        self, CustomKeyHint, CustomKeys, ExpandMode, ItemProvider, KeyBinding, MenuItem, Modifier,
        ProviderData,
    },
};

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
    fn new(config: &Config) -> Result<Self, String> {
        let output = rbw("list", Some(vec!["--fields", "id,name"]))?;
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
                    vec![].into_iter().collect(),
                    None,
                    0.0,
                    Some(MenuItemMetaData { ids: value.clone() }),
                )
            })
            .collect::<Vec<_>>();
        gui::apply_sort(&mut items, &config.sort_order());

        Ok(Self { items })
    }

    fn sub_provider(ids: Vec<String>) -> Result<Self, String> {
        let items = ids
            .iter()
            .map(|id| {
                Ok(MenuItem::new(
                    rbw_get_user(id, false)?,
                    None,
                    None,
                    vec![].into_iter().collect(),
                    None,
                    0.0,
                    Some(MenuItemMetaData {
                        ids: vec![id.clone()].into_iter().collect(),
                    }),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(Self { items })
    }
}

impl ItemProvider<MenuItemMetaData> for PasswordProvider {
    fn get_elements(&mut self, query: Option<&str>) -> ProviderData<MenuItemMetaData> {
        if query.is_some() {
            ProviderData { items: None }
        } else {
            ProviderData {
                items: Some(self.items.clone()),
            }
        }
    }

    fn get_sub_elements(
        &mut self,
        _: &MenuItem<MenuItemMetaData>,
    ) -> ProviderData<MenuItemMetaData> {
        ProviderData { items: None }
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

fn parse_cmd(cmd: &str) -> (&str, Option<u64>, Option<&str>) {
    if let Some(pos) = cmd.find("$S") {
        let left = &cmd[..pos];
        let rest = &cmd[pos + 2..]; // Skip "$S"

        // Extract digits after "$S"
        let num_part: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();

        if let Ok(number) = num_part.parse::<u64>() {
            let right = &rest[num_part.len()..];
            return (left, Some(number), Some(right));
        }
    }

    (cmd, None, None)
}

fn keyboard_return() {
    keyboard_type("\n");
}

fn keyboard_auto_type(cmd: &str, id: &str) -> Result<(), String> {
    let user = rbw_get_user(id, false)?;
    let pw = rbw_get_password(id, false)?;

    let ydo_string = cmd.replace('_', "").replace("$U", &user).replace("$P", &pw);

    let (left, sleep_ms, right) = parse_cmd(&ydo_string);
    keyboard_type(left);
    if let Some(sleep_ms) = sleep_ms {
        sleep(Duration::from_millis(sleep_ms));
    }

    if let Some(right) = right {
        keyboard_type(right);
    }

    Ok(())
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
        .map_err(|e| format!("Failed to execute command: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("rbw command failed: {}", stderr.trim()));
    }

    let stdout =
        String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8 output: {e}"))?;

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
        modifiers: vec![Modifier::Alt].into_iter().collect(),
        label: "<b>Alt+1</b> Type All".to_string(),
        visible: true,
    }
}

fn key_type_all_and_enter() -> KeyBinding {
    KeyBinding {
        key: Key::Num1,
        modifiers: vec![Modifier::Alt, Modifier::Shift].into_iter().collect(),
        label: String::new(),
        visible: false,
    }
}

fn key_type_user() -> KeyBinding {
    KeyBinding {
        key: Key::Num2,
        modifiers: vec![Modifier::Alt].into_iter().collect(),
        label: "<b>Alt+2</b> Type User".to_string(),
        visible: true,
    }
}

fn key_type_user_and_enter() -> KeyBinding {
    KeyBinding {
        key: Key::Num2,
        modifiers: vec![Modifier::Alt, Modifier::Shift].into_iter().collect(),
        label: String::new(),
        visible: false,
    }
}

fn key_type_password() -> KeyBinding {
    KeyBinding {
        key: Key::Num3,
        modifiers: vec![Modifier::Alt].into_iter().collect(),
        label: "<b>Alt+3</b> Type Password".to_string(),
        visible: true,
    }
}

fn key_type_password_and_enter() -> KeyBinding {
    KeyBinding {
        key: Key::Num3,
        modifiers: vec![Modifier::Alt, Modifier::Shift].into_iter().collect(),
        label: String::new(),
        visible: false,
    }
}

fn key_type_totp() -> KeyBinding {
    KeyBinding {
        key: Key::Num4,
        modifiers: vec![Modifier::Alt].into_iter().collect(),
        label: "<b>Alt+4</b> Type Totp".to_string(),
        visible: true,
    }
}

fn key_type_totp_and_enter() -> KeyBinding {
    KeyBinding {
        key: Key::Num4,
        modifiers: vec![Modifier::Alt, Modifier::Shift].into_iter().collect(),
        label: String::new(),
        visible: false,
    }
}

fn key_sync() -> KeyBinding {
    KeyBinding {
        key: Key::R,
        modifiers: vec![Modifier::Alt].into_iter().collect(),
        label: "<b>Alt+r</b> Sync".to_string(),
        visible: true,
    }
}

/// copies totp to clipboard
fn key_totp_to_clipboard() -> KeyBinding {
    KeyBinding {
        key: Key::T,
        modifiers: vec![Modifier::Alt].into_iter().collect(),
        label: "<b>Alt+t</b> Copy Totp".to_string(),
        visible: true,
    }
}

fn key_lock() -> KeyBinding {
    KeyBinding {
        key: Key::L,
        modifiers: vec![Modifier::Alt].into_iter().collect(),
        label: "<b>Alt+l</b> Lock".to_string(),
        visible: true,
    }
}

fn show(
    config: Arc<RwLock<Config>>,
    provider: Arc<Mutex<PasswordProvider>>,
    warden_config: WardenConfig,
) -> Result<(), String> {
    match gui::show(
        &config,
        provider,
        None,
        None,
        ExpandMode::Verbatim,
        Some(CustomKeys {
            bindings: vec![
                key_type_all(),
                key_type_all_and_enter(),
                key_type_user(),
                key_type_user_and_enter(),
                key_type_password(),
                key_type_password_and_enter(),
                key_type_totp(),
                key_type_totp_and_enter(),
                key_sync(),
                key_totp_to_clipboard(),
                key_lock(),
            ],
            hint: Some(CustomKeyHint {
                label: "Use Shift as additional modifier to send enter".to_string(),
                location: CustomKeyHintLocation::Top,
            }),
        }),
    ) {
        Ok(selection) => {
            if let Some(meta) = selection.menu.data {
                if meta.ids.len() > 1 {
                    return show(
                        config,
                        Arc::new(Mutex::new(PasswordProvider::sub_provider(meta.ids)?)),
                        warden_config.clone(),
                    );
                }

                let id = meta.ids.first().unwrap_or(&selection.menu.label);

                sleep(Duration::from_millis(500));
                if let Some(key) = selection.custom_key {
                    if key == key_type_all() || key == key_type_all_and_enter() {
                        let default = "$U\t$P".to_owned();
                        let typing = warden_config
                            .custom_auto_types
                            .get(id)
                            .or(warden_config.custom_auto_types.get(&selection.menu.label))
                            .unwrap_or(&default);
                        keyboard_auto_type(typing, id)?;
                    } else if key == key_type_user() || key == key_type_user_and_enter() {
                        keyboard_type(&rbw_get_user(id, false)?);
                    } else if key == key_type_password() || key == key_type_password_and_enter() {
                        keyboard_type(&rbw_get_password(id, false)?);
                    } else if key == key_type_totp() || key == key_type_totp_and_enter() {
                        keyboard_type(&rbw_get_totp(id, false)?);
                    } else if key == key_lock() {
                        rbw("lock", None)?;
                    } else if key == key_sync() {
                        rbw("sync", None)?;
                    } else if key == key_totp_to_clipboard() {
                        rbw_get_totp(id, true)?;
                    }

                    if key.modifiers.contains(&Modifier::Shift) {
                        keyboard_return();
                    }
                } else {
                    let pw = rbw_get_password(id, true)?;
                    if let Err(e) = copy_to_clipboard(pw, None) {
                        log::error!("failed to copy to clipboard: {e}");
                    }
                }
                Ok(())
            } else {
                Err("missing meta data".to_owned())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct WardenConfig {
    custom_auto_types: HashMap<String, String>,
}

#[derive(Debug, Parser, Clone)]
struct WardenArgs {
    /// Configuration file for worf warden
    #[clap(long = "warden-config")]
    warden_config: Option<String>,

    #[command(flatten)]
    worf: Config,
}

fn main() -> Result<(), String> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let mut cfg = WardenArgs::parse();
    cfg.worf = config::load_worf_config(Some(&cfg.worf)).unwrap_or(cfg.worf);

    let warden_config: WardenConfig =
        config::load_config(cfg.warden_config.as_deref(), "worf", "warden")
            .map_err(|e| format!("failed to parse warden config {e}"))?;

    if !groups().contains("input") {
        log::error!(
            "User must be in input group. 'sudo usermod -aG input $USER', then login again"
        );
        std::process::exit(1)
    }

    // will exit if there is a daemon running already, so it's fine to call this everytime.
    if let Err(e) = spawn_fork("ydotoold", None) {
        log::error!("Failed to start ydotool daemon: {e}");
    }

    let worf_config = Arc::new(RwLock::new(cfg.worf.clone()));

    // todo eventually use a propper rust client for this, for now rbw is good enough
    let provider = Arc::new(Mutex::new(PasswordProvider::new(
        &worf_config.read().unwrap(),
    )?));
    show(worf_config, provider, warden_config)
}

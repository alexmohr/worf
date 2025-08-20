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

fn keyboard_type(text: &str, cfg: &WardenConfig) {
    let mut cmd = Command::new(cfg.typing_cmd());
    for arg in cfg.typing_cmd_args() {
        cmd.arg(arg);
    }
    cmd.arg(text);

    cmd.output()
        .unwrap_or_else(|_| panic!("Failed to execute {}", cfg.typing_cmd()));
}

fn keyboard_return(config: &WardenConfig) {
    keyboard_type("\n", config);
}

fn keyboard_auto_type(cmd: &str, id: &str, config: &WardenConfig) -> Result<(), String> {
    let mut input = cmd.replace('_', "");

    while !input.is_empty() {
        // Remove up to and including the first '$'

        if let Some(pos) = input.find('$') {
            // Extract substring before '$'
            let extracted = input[..pos].to_string();

            // Remove extracted part + '$' from input
            input.drain(..=pos);

            if !extracted.is_empty() {
                keyboard_type(extracted.as_str(), config);
            }
        }

        // Match the next character
        match input.chars().next() {
            Some('S') => {
                // Remove the 'S' command character
                input.remove(0);

                // Collect digits following 'S'
                let digits: String = input.chars().take_while(|c| c.is_ascii_digit()).collect();

                // Remove the digits from input
                let len = digits.len();
                input.drain(..len);

                // Parse and sleep
                if let Ok(ms) = digits.parse::<u64>() {
                    sleep(Duration::from_millis(ms));
                } else {
                    log::error!("Failed to parse digits: {digits}");
                }
                continue;
            }

            Some('U') => {
                let user = rbw_get_user(id, false)?;
                keyboard_type(&user, config);
            }

            Some('P') => {
                let pw = rbw_get_password(id, false)?;
                keyboard_type(&pw, config);
            }

            Some('T') => {
                let totp = rbw_get_totp(id, false)?;
                keyboard_type(&totp, config);
            }

            Some(c) => {
                log::error!("Unknown character found: {c}");
            }

            None => {
                log::error!("No command found after '$'");
            }
        }

        input.drain(..1);
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
        .map_err(|e| format!("Failed to execute rbw command: {e}"))?;

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
                        keyboard_auto_type(typing, id, &warden_config)?;
                    } else if key == key_type_user() || key == key_type_user_and_enter() {
                        keyboard_type(&rbw_get_user(id, false)?, &warden_config);
                    } else if key == key_type_password() || key == key_type_password_and_enter() {
                        keyboard_type(&rbw_get_password(id, false)?, &warden_config);
                    } else if key == key_type_totp() || key == key_type_totp_and_enter() {
                        keyboard_type(&rbw_get_totp(id, false)?, &warden_config);
                    } else if key == key_lock() {
                        rbw("lock", None)?;
                    } else if key == key_sync() {
                        rbw("sync", None)?;
                    } else if key == key_totp_to_clipboard() {
                        rbw_get_totp(id, true)?;
                    }

                    if key.modifiers.contains(&Modifier::Shift) {
                        keyboard_return(&warden_config);
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

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
struct WardenConfig {
    typing_cmd: Option<String>,
    typing_cmd_args: Option<Vec<String>>,
    custom_auto_types: HashMap<String, String>,
}

impl WardenConfig {
    fn typing_cmd(&self) -> String {
        self.typing_cmd.clone().unwrap_or("ydotool".to_owned())
    }

    fn typing_cmd_args(&self) -> Vec<String> {
        self.typing_cmd_args
            .clone()
            .unwrap_or(vec!["type".to_owned()])
    }
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
        config::load_config(cfg.warden_config.as_deref(), "worf", "warden").unwrap_or_else(|e| {
            log::warn!(
                "Failed to parse or find the worf-warden configuration: {e}, using defaults"
            );
            WardenConfig::default()
        });

    if !groups().contains("input") {
        log::error!(
            "User must be in input group. 'sudo usermod -aG input $USER', then login again"
        );
        std::process::exit(1)
    }

    // ydotool is our special default value, give it some love and start the daemon
    // if other tools need this it must be run beforehand (or can be added here)
    // in case another tool is added it might make sense to make it configurable
    if warden_config.typing_cmd() == "ydotool" {
        // will exit if there is a daemon running already, so it's fine to call this everytime.
        if let Err(e) = spawn_fork("ydotoold", None) {
            log::error!("Failed to start ydotool daemon: {e}");
        }
    }

    let worf_config = Arc::new(RwLock::new(cfg.worf.clone()));

    // todo eventually use a propper rust client for this, for now rbw is good enough
    let provider = Arc::new(Mutex::new(PasswordProvider::new(
        &worf_config.read().unwrap(),
    )?));
    show(worf_config, provider, warden_config)
}

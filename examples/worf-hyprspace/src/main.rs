use std::{
    env,
    fmt::{Display, Formatter},
    str::FromStr,
    sync::{Arc, LazyLock, Mutex, RwLock},
    thread::sleep,
    time::{Duration, Instant},
};

use clap::Parser;
use hyprland::{
    data::{Client, Workspace, Workspaces},
    dispatch::{Dispatch, DispatchType, WindowIdentifier, WorkspaceIdentifierWithSpecial},
    prelude::HyprData,
    shared::HyprDataActive,
};
use nix::libc::{SIGTERM, kill};
use regex::Regex;
use serde::Deserialize;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use worf::gui::{
    self, ArcFactory, ArcProvider, ExpandMode, ItemFactory, ItemProvider, MenuItem, ProviderData,
    Selection,
};

#[derive(Clone)]
struct Action {
    workspace: Option<Workspace>,
    mode: Mode,
}

#[derive(Clone)]
struct HyprspaceProvider {
    cfg: HyprSpaceConfig,
    search_ignored_words: Vec<Regex>,
    detected_mode: Option<Mode>,
}

#[derive(Debug, Clone, Deserialize, EnumIter, PartialEq, Eq)]
enum Mode {
    Auto,
    Rename,
    SwitchToWorkspace,
    MoveCurrentWindowToOtherWorkspace,
    MoveCurrentWindowToOtherWorkspaceSilent,
    MoveAllWindowsToOtherWorkSpace,
    DeleteWorkspace,
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Mode::Auto),
            "rename" => Ok(Mode::Rename),
            "switchtoworkspace" => Ok(Mode::SwitchToWorkspace),
            "movecurrentwindowtootherworkspace" => Ok(Mode::MoveCurrentWindowToOtherWorkspace),
            "movecurrentwindowtootherworkspacesilent" => {
                Ok(Mode::MoveCurrentWindowToOtherWorkspaceSilent)
            }
            "moveallwindowstootherworkspace" => Ok(Mode::MoveCurrentWindowToOtherWorkspace),
            "deleteworkspace" => Ok(Mode::DeleteWorkspace),
            _ => Err(format!("Invalid mode: {s}")),
        }
    }
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let variant = format!("{self:?}");
        // Convert PascalCase to Title Case with spaces
        let spaced = inflector::cases::titlecase::to_title_case(&variant);
        write!(f, "{spaced}")
    }
}

#[derive(Debug, Clone, Parser, Deserialize)]
#[clap(about = "Worf-Hyprspace is a Hyprland workspace manager built on top of Worf")]
struct HyprSpaceConfig {
    #[command(flatten)]
    worf: worf::config::Config,

    #[arg(long)]
    hypr_space_mode: Option<Mode>,

    #[arg(long)]
    add_id_prefix: Option<bool>,

    #[arg(long)]
    max_workspace_id: Option<i32>,
}

impl HyprSpaceConfig {
    fn hypr_space_mode(&self) -> Mode {
        self.hypr_space_mode.clone().unwrap_or(Mode::Auto)
    }

    fn add_id_prefix(&self) -> bool {
        self.add_id_prefix.unwrap_or(true)
    }

    fn max_workspace_id(&self) -> i32 {
        self.max_workspace_id.unwrap_or(10)
    }
}

impl HyprspaceProvider {
    fn new(cfg: &HyprSpaceConfig, search_ignored_words: Vec<Regex>) -> Result<Self, String> {
        Ok(Self {
            cfg: cfg.clone(),
            search_ignored_words,
            detected_mode: None,
        })
    }
}

impl ItemProvider<Action> for HyprspaceProvider {
    fn get_elements(&mut self, query: Option<&str>) -> ProviderData<Action> {
        let auto = if self.cfg.hypr_space_mode() == Mode::Auto {
            query.and_then(|q| {
                Mode::iter()
                    .find(|m| m.to_string().to_lowercase().trim() == q.to_lowercase())
                    .map(|m| {
                        self.detected_mode = Some(m.clone());
                        ProviderData {
                            items: Some(get_modes_actions(
                                &m,
                                query,
                                self.search_ignored_words.as_ref(),
                            )),
                        }
                    })
            })
        } else {
            self.detected_mode = None;
            None
        };
        auto.unwrap_or(ProviderData {
            items: Some(get_modes_actions(
                &self.cfg.hypr_space_mode(),
                query,
                self.search_ignored_words.as_ref(),
            )),
        })
    }

    fn get_sub_elements(&mut self, item: &MenuItem<Action>) -> ProviderData<Action> {
        if let Some(mode) = Mode::iter()
            .find(|m| {
                m.to_string()
                    .to_lowercase()
                    .trim()
                    .contains(&item.label.to_lowercase())
            })
            .map(|m| {
                self.detected_mode = Some(m.clone());
                ProviderData {
                    items: Some(get_modes_actions(
                        &m,
                        Some(&item.label),
                        self.search_ignored_words.as_ref(),
                    )),
                }
            })
        {
            mode
        } else {
            ProviderData { items: None }
        }
    }
}

impl ItemFactory<Action> for HyprspaceProvider {
    fn new_menu_item(&self, label: String) -> Option<MenuItem<Action>> {
        Some(MenuItem::new(
            label,
            None,
            None,
            Vec::new(),
            None,
            0.0,
            Some(Action {
                workspace: None,
                mode: if self.cfg.hypr_space_mode() == Mode::Auto {
                    self.detected_mode.clone().unwrap_or(Mode::Auto)
                } else {
                    self.cfg.hypr_space_mode()
                },
            }),
        ))
    }
}

fn build_menu_items<'a, F>(
    mode: &Mode,
    aws: &'a Workspace,
    workspaces: &'a Workspaces,
    query: Option<&'a str>,
    search_ignored_words: &Vec<Regex>,
    filter_fn: F,
) -> Vec<MenuItem<Action>>
where
    F: for<'b> Fn(&'b Workspace) -> bool + Copy,
{
    workspaces
        .iter()
        .filter(|ws| filter_fn(ws))
        .map(|ws| workspace_to_menu_item(mode, aws, ws))
        .chain(query.map(|q| {
            MenuItem::new(
                gui::filtered_query(Some(search_ignored_words), q),
                None,
                None,
                Vec::new(),
                None,
                0.0,
                Some(Action {
                    workspace: None,
                    mode: mode.clone(),
                }),
            )
        }))
        .collect()
}

fn get_modes_actions(
    mode: &Mode,
    query: Option<&str>,
    search_ignored_words: &Vec<Regex>,
) -> Vec<MenuItem<Action>> {
    let workspaces = match hyprland::data::Workspaces::get() {
        Ok(ws) => ws,
        Err(e) => {
            log::error!("Failed to get workspaces {e}");
            return Vec::new();
        }
    };

    let aws = if let Ok(ws) = hyprland::data::Workspace::get_active() {
        ws
    } else {
        log::error!("No active workspace found");
        return Vec::<MenuItem<Action>>::new();
    };

    match mode {
        Mode::Auto => Mode::iter()
            .filter(|m| m != &Mode::Auto)
            .map(|mode| {
                MenuItem::new(
                    mode.to_string(),
                    None,
                    None,
                    Vec::new(),
                    None,
                    0.0,
                    Some(Action {
                        workspace: None,
                        mode,
                    }),
                )
            })
            .collect(),

        Mode::Rename | Mode::DeleteWorkspace => {
            build_menu_items(mode, &aws, &workspaces, query, search_ignored_words, |_| {
                true
            })
        }

        Mode::SwitchToWorkspace
        | Mode::MoveAllWindowsToOtherWorkSpace
        | Mode::MoveCurrentWindowToOtherWorkspace
        | Mode::MoveCurrentWindowToOtherWorkspaceSilent => {
            build_menu_items(mode, &aws, &workspaces, query, search_ignored_words, |ws| {
                ws.id != aws.id
            })
        }
    }
}

fn workspace_to_menu_item(mode: &Mode, aws: &Workspace, ws: &Workspace) -> MenuItem<Action> {
    MenuItem::new(
        ws.name.clone(),
        None,
        None,
        Vec::new(),
        None,
        if aws.id == ws.id { 1.0 } else { 0.0 },
        Some(Action {
            workspace: Some(ws.clone()),
            mode: mode.clone(),
        }),
    )
}

fn handle_sub_selection(
    item: &MenuItem<Action>,
    query: Option<&str>,
    search_ignored_words: &Vec<Regex>,
) -> ProviderData<Action> {
    if let Some(mode) = Mode::iter()
        .find(|m| {
            m.to_string()
                .to_lowercase()
                .contains(&item.label.to_lowercase())
        })
        .map(|m| ProviderData {
            items: Some(get_modes_actions(&m, query, search_ignored_words)),
        })
    {
        mode
    } else {
        ProviderData { items: None }
    }
}

#[derive(Clone)]
struct EmptyProvider {}

impl ItemProvider<Action> for EmptyProvider {
    fn get_elements(&mut self, search: Option<&str>) -> ProviderData<Action> {
        ProviderData {
            items: Some(vec![MenuItem::new(
                search.unwrap_or_default().to_owned(),
                None,
                None,
                Vec::new(),
                None,
                0.0,
                Some(Action {
                    workspace: None,
                    mode: Mode::Auto,
                }),
            )]),
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<Action>) -> ProviderData<Action> {
        ProviderData { items: None }
    }
}

impl ItemFactory<Action> for EmptyProvider {
    fn new_menu_item(&self, label: String) -> Option<MenuItem<Action>> {
        Some(MenuItem::new(
            label,
            None,
            None,
            Vec::new(),
            None,
            0.0,
            Some(Action {
                workspace: None,
                mode: Mode::Auto,
            }),
        ))
    }
}

fn find_first_free_workspace_id(max_id: i32) -> Option<i32> {
    let ws = Workspaces::get().ok()?;
    (1..=max_id).find(|&i| !ws.iter().any(|w| w.id == i))
}

fn show_gui<T: ItemProvider<Action> + ItemFactory<Action> + Send + Clone + 'static>(
    cfg: &HyprSpaceConfig,
    pattern: &Regex,
    provider: Arc<Mutex<T>>,
) -> Result<Selection<Action>, String> {
    gui::show(
        &Arc::new(RwLock::new(cfg.worf.clone())),
        Arc::clone(&provider) as ArcProvider<Action>,
        Some(provider as ArcFactory<Action>),
        Some(vec![pattern.clone()]),
        ExpandMode::WithSpace,
        None,
    )
    .map_err(|e| e.to_string())
}

fn workspace_from_selection<'a>(
    action: Option<Action>,
    max_id: i32,
) -> Result<(WorkspaceIdentifierWithSpecial<'a>, i32, bool), String> {
    if let Some(action) = action {
        if let Some(ws) = action.workspace {
            return Ok((WorkspaceIdentifierWithSpecial::Id(ws.id), ws.id, false));
        }
    }
    find_first_free_workspace_id(max_id)
        .map(|id| (WorkspaceIdentifierWithSpecial::Id(id), id, true))
        .ok_or_else(|| "Failed to get workspace id".to_string())
}

fn set_workspace_name(label: &str, id: i32, add_id_prefix: bool) -> Result<(), String> {
    // todo maybe there is a better way to poll if a workspace has been created
    let start = Instant::now();
    let ws = loop {
        // same as above might break at some point but waiting at the tail
        // end of the loop sometimes leads to timing issues
        // where the workspace exists in some weird state
        sleep(Duration::from_millis(10));
        if start.elapsed().as_millis() >= 1500 {
            break None;
        }

        if let Some(workspace) = get_workspace(id)? {
            break Some(workspace);
        }
    };

    ws.map(|ws| {
        let ws_id = ws.id.to_string();
        let id_prefix = format!("{ws_id}: ");
        let new_name = if add_id_prefix && !ws.name.starts_with(&id_prefix) {
            &format!("{id_prefix}{label}")
        } else {
            label
        };

        Dispatch::call(DispatchType::RenameWorkspace(ws.id, Some(new_name)))
    })
    .transpose()
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn get_workspace(id: i32) -> Result<Option<Workspace>, String> {
    let ws = Workspaces::get()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|ws| ws.id == id);
    Ok(ws)
}

fn process_clients_on_workspace<F>(ws_id: i32, proc: F) -> Result<(), String>
where
    F: for<'a> Fn(&'a Client),
{
    hyprland::data::Clients::get()
        .map_err(|e| format!("failed to get clients for ws {ws_id}, err {e}"))?
        .iter()
        .filter(|client| client.workspace.id == ws_id)
        .for_each(proc);
    Ok(())
}

fn handle_workspace_action<F>(
    cfg: &HyprSpaceConfig,
    label: &str,
    action: Option<Action>,
    dispatch_builder: F,
) -> Result<(), String>
where
    F: FnOnce(WorkspaceIdentifierWithSpecial) -> DispatchType,
{
    let (workspace, id, _new) = workspace_from_selection(action, cfg.max_workspace_id())?;
    Dispatch::call(dispatch_builder(workspace)).map_err(|e| e.to_string())?;
    set_workspace_name(label, id, cfg.add_id_prefix())?;
    Ok(())
}

fn main() -> Result<(), String> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let mut cfg = HyprSpaceConfig::parse();
    cfg.worf = worf::config::load_worf_config(Some(&cfg.worf)).unwrap_or(cfg.worf);
    if cfg.worf.prompt().is_none() {
        cfg.worf.set_prompt(cfg.hypr_space_mode().to_string());
    }

    static PATTERN_RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = Mode::iter()
            .map(|m| regex::escape(&m.to_string().to_lowercase()))
            .collect::<Vec<_>>()
            .join("|");
        Regex::new(&format!("(?i){pattern}")).unwrap()
    });
    let pattern = PATTERN_RE.clone();

    let provider = Arc::new(Mutex::new(HyprspaceProvider::new(
        &cfg,
        vec![pattern.clone()],
    )?));

    process_inputs(&mut cfg, &pattern, provider)?;

    Ok(())
}

fn process_inputs(
    cfg: &mut HyprSpaceConfig,
    pattern: &Regex,
    provider: Arc<Mutex<HyprspaceProvider>>,
) -> Result<(), String> {
    let result = show_gui(cfg, pattern, Arc::clone(&provider))?;

    let result_items = handle_sub_selection(&result.menu, None, vec![pattern.clone()].as_ref());
    let result = if result_items.items.is_some() {
        if let Some(menu) = result.menu.data {
            cfg.hypr_space_mode = Some(menu.mode.clone());
            cfg.worf.set_prompt(cfg.hypr_space_mode().to_string());

            let provider = Arc::new(Mutex::new(HyprspaceProvider::new(
                &cfg.clone(),
                vec![pattern.clone()],
            )?));
            show_gui(cfg, pattern, provider)?
        } else {
            result
        }
    } else {
        result
    };

    let action = result.menu.data;
    let mode = action
        .as_ref()
        .map(|m| m.mode.clone())
        .unwrap_or(cfg.hypr_space_mode());
    match mode {
        Mode::Auto => {
            return process_inputs(cfg, pattern, provider);
        }
        Mode::Rename => {
            if let Some(action) = action {
                cfg.worf
                    .set_prompt(format!("Rename {} to  ", result.menu.label));
                let provider = Arc::new(Mutex::new(EmptyProvider {}));
                let rename_result = show_gui(cfg, pattern, provider)?;

                let new_name = if cfg.add_id_prefix() {
                    let ws_id = action
                        .workspace
                        .as_ref()
                        .map(|ws| ws.id.to_string())
                        .unwrap_or_default();
                    format!("{}: {}", ws_id, rename_result.menu.label)
                } else {
                    rename_result.menu.label.to_string()
                };

                Dispatch::call(DispatchType::RenameWorkspace(
                    action.workspace.as_ref().unwrap().id,
                    Some(&new_name),
                ))
                .map_err(|e| e.to_string())?;
            } else {
                Err("Action is not set, cannot rename workspace".to_owned())?;
            }
        }
        Mode::SwitchToWorkspace => {
            // Clippy suggests removing this closure as redundant,
            // but doing so causes lifetime inference issues with `DispatchType::Workspace`.
            // Keeping the closure avoids `'static` lifetime assumptions.
            #[allow(clippy::redundant_closure)]
            handle_workspace_action(cfg, &result.menu.label, action, |ws| {
                DispatchType::Workspace(ws)
            })?;
        }
        Mode::MoveCurrentWindowToOtherWorkspace => {
            handle_workspace_action(cfg, &result.menu.label, action, |ws| {
                DispatchType::MoveToWorkspace(ws, None)
            })?;
        }
        Mode::MoveCurrentWindowToOtherWorkspaceSilent => {
            handle_workspace_action(cfg, &result.menu.label, action, |ws| {
                DispatchType::MoveToWorkspaceSilent(ws, None)
            })?;
        }
        Mode::DeleteWorkspace => {
            let (_ws, selected_id, _new) =
                workspace_from_selection(action, cfg.max_workspace_id())?;

            process_clients_on_workspace(selected_id, |client| unsafe {
                kill(client.pid, SIGTERM);
            })?;

            let active_ws = Workspace::get_active()
                .map_err(|e| format!("failed to get active workspace {e}"))?;
            if active_ws.id == selected_id {
                Dispatch::call(DispatchType::Workspace(
                    WorkspaceIdentifierWithSpecial::Previous,
                ))
                .map_err(|e| e.to_string())?;
            }
        }
        Mode::MoveAllWindowsToOtherWorkSpace => {
            let active_ws = Workspace::get_active()
                .map_err(|e| format!("failed to get active workspace {e}"))?;

            let (ws, target_id, new) = workspace_from_selection(action, cfg.max_workspace_id())?;
            process_clients_on_workspace(active_ws.id, |client| {
                if let Err(e) = Dispatch::call(DispatchType::MoveToWorkspace(
                    ws,
                    Some(WindowIdentifier::Address(client.address.clone())),
                )) {
                    log::warn!("cannot move client to new workspace, ignoring it, err={e}")
                }
            })?;

            if new {
                set_workspace_name(&result.menu.label, target_id, cfg.add_id_prefix())?;
            }
        }
    }
    Ok(())
}

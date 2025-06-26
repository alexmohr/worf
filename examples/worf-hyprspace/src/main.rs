use std::collections::HashSet;
use clap::Parser;
use hyprland::data::{Workspace, Workspaces};
use hyprland::dispatch::{Dispatch, WorkspaceIdentifierWithSpecial};
use hyprland::shared::HyprDataActive;
use hyprland::{dispatch::DispatchType, prelude::HyprData};

use regex::Regex;
use serde::Deserialize;
use std::env;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use worf::gui::{ProviderData, Selection};
use worf::{
    //Error, desktop,
    //desktop::EntryType,
    gui::{self, ItemProvider, MenuItem},
};

#[derive(Clone)]
struct Action {
    workspace: Option<Workspace>,
    mode: Mode,
}

#[derive(Clone)]
struct HyprspaceProvider {
    //workspaces: Workspaces,
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
    MoveAllWindowsToOtherWorkspace,
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
            "moveallwindowstootherworkspace" => Ok(Mode::MoveAllWindowsToOtherWorkspace),
            "deleteworkspace" => Ok(Mode::DeleteWorkspace),
            _ => Err(format!("Invalid mode: {}", s)),
        }
    }
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let variant = format!("{:?}", self);
        // Convert PascalCase to Title Case with spaces
        let spaced = inflector::cases::titlecase::to_title_case(&variant);
        write!(f, "{}", spaced)
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
    max_workspace_id: Option<u32>
}

impl HyprSpaceConfig {
    fn hypr_space_mode(&self) -> Mode {
        self.hypr_space_mode.clone().unwrap_or(Mode::Auto)
    }

    fn add_id_prefix(&self) -> bool {
        self.add_id_prefix.unwrap_or(true)
    }

    fn max_workspace_id(&self) -> u32 {
        self.max_workspace_id.unwrap_or(10)
    }
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

        Mode::Rename => workspaces
            .iter()
            .map(|ws| {
                workspace_to_menu_item(mode, &aws, ws)
            })
            .collect(),
        Mode::SwitchToWorkspace | Mode::MoveCurrentWindowToOtherWorkspace => workspaces
            .iter()
            .filter(|ws| ws.id != aws.id)
            .map(|ws| {
                workspace_to_menu_item(mode, &aws, ws)
            })
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
            .collect(),
        Mode::MoveAllWindowsToOtherWorkspace => Vec::<MenuItem<Action>>::new(),
        Mode::DeleteWorkspace => Vec::<MenuItem<Action>>::new(),
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

impl HyprspaceProvider {
    fn new(cfg: &HyprSpaceConfig, search_ignored_words: Vec<Regex>) -> Result<Self, String> {
        //let workspaces = hyprland::data::Workspaces::get().map_err(|e| e.to_string())?;
        Ok(Self {
            //workspaces,
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

    fn create_new_element_data(&self, _: &str) -> Option<Action> {
        Some(Action {
            workspace: None,
            mode: self.detected_mode.clone().unwrap_or(Mode::Auto),
        })
    }
}
//
// fn find_first_free_workspace_id(max_id: i32) -> Option<u32> {
//     Workspaces::get()
//         .ok()?
//         .iter()
//         .map(|ws| ws.id as
//         .max()
//         .map(|m| m + 1)
//
// }

fn show_gui<T: ItemProvider<Action> + Send + Clone + 'static>(
    cfg: &HyprSpaceConfig,
    pattern: &Regex,
    provider: T,
) -> Result<Selection<Action>, String> {
    gui::show(
        cfg.worf.clone(),
        provider,
        true,
        Some(vec![pattern.clone()]),
        true,
        None,
    )
        .map_err(|e| e.to_string())
}
fn resolve_workspace_id(label: &str) -> Option<i32> {
    let workspaces = Workspaces::get().ok()?;
    for ws in workspaces {
        if ws.name == label {
            return Some(ws.id);
        }
    }
    None
}

fn workspace_from_selection(label: &str, action: Option<Action>) -> Result<WorkspaceIdentifierWithSpecial, String> {
    Ok(action
        .and_then(|action| {
            action
                .workspace
                .as_ref()
                .map(|ws| WorkspaceIdentifierWithSpecial::Id(ws.id))
        })
        .unwrap_or(WorkspaceIdentifierWithSpecial::Name(label)))
    // todo fix this, it should get a id workspace instead of a named one
}


fn add_id_prefix_by_name(label: &str) -> Result<(), String> {
    Workspaces::get()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|ws| ws.name == label)
        .map(|ws| {
            let ws_id = ws.id.to_string();
            let rename_str = format!("{}: {}", ws_id, label);
            Dispatch::call(DispatchType::RenameWorkspace(ws.id, Some(&rename_str)))
        })
        .transpose()
        .map_err(|e| e.to_string())?; // converts Option<Result<..>> -> Result<Option<..>>

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
    let workspace = workspace_from_selection(label, action)?;
    Dispatch::call(dispatch_builder(workspace)).map_err(|e| e.to_string())?;
    if cfg.add_id_prefix() {
        add_id_prefix_by_name(label)?;
    }
    Ok(())
}

fn main() -> Result<(), String> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let mut cfg = HyprSpaceConfig::parse();
    cfg.worf = worf::config::load_config(Some(&cfg.worf)).unwrap_or(cfg.worf);
    if cfg.worf.prompt().is_empty() {
        cfg.worf.set_prompt(cfg.hypr_space_mode().to_string());
    }

    let pattern = Mode::iter()
        .map(|m| regex::escape(&m.to_string().to_lowercase()))
        .collect::<Vec<_>>()
        .join("|");

    let pattern = Regex::new(&format!("(?i){}", pattern)).map_err(|e| e.to_string())?;

    let provider = HyprspaceProvider::new(&cfg, vec![pattern.clone()])?;
    let result = show_gui(&cfg, &pattern, provider)?;

    let result_items = handle_sub_selection(&result.menu, None, vec![pattern.clone()].as_ref());
    let result = if result_items.items.is_some() {
        if let Some(menu) = result.menu.data {
            cfg.hypr_space_mode = Some(menu.mode.clone());
            cfg.worf.set_prompt(cfg.hypr_space_mode().to_string());

            let provider = HyprspaceProvider::new(&cfg.clone(), vec![pattern.clone()])?;
            show_gui(&cfg, &pattern, provider)?
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
        Mode::Auto=> {
            unreachable!("Auto mode must be set to a specific mode at exit")
        }
        Mode::Rename => {
            if let Some(action) = action {
                cfg.worf
                    .set_prompt(format!("Rename {} to  ", result.menu.label));
                let provider = EmptyProvider {};
                let rename_result = show_gui(&cfg, &pattern, provider)?;

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
            handle_workspace_action(&cfg, &result.menu.label, action, |ws| DispatchType::Workspace(ws))?;
        }
        Mode::MoveCurrentWindowToOtherWorkspace => {
            handle_workspace_action(&cfg, &result.menu.label, action, |ws| DispatchType::MoveToWorkspace(ws, None))?;
        }
        Mode::MoveAllWindowsToOtherWorkspace => {}
        Mode::DeleteWorkspace => {}
    }

    Ok(())
}

use anyhow::Context;
use freedesktop_file_parser::EntryType;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::os::unix::prelude::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use std::{env, fmt, fs, io};

use crate::config::{Config, expand_path};
use crate::desktop::{
    default_icon, find_desktop_files, get_locale_variants, lookup_name_with_locale,
};
use crate::gui;
use crate::gui::{ItemProvider, MenuItem};

#[derive(Debug)]
pub enum ModeError {
    UpdateCacheError(String),
    MissingAction,
    RunError(String),
    MissingCache,
    StdInReadFail,
    InvalidSelection,
}

impl fmt::Display for ModeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ModeError::UpdateCacheError(s) => write!(f, "UpdateCacheError {s}"),
            ModeError::MissingAction => write!(f, "MissingAction"),
            ModeError::RunError(s) => write!(f, "RunError, {s}"),
            ModeError::MissingCache => write!(f, "MissingCache"),
            ModeError::StdInReadFail => write!(f, "StdInReadFail"),
            &ModeError::InvalidSelection => write!(f, "InvalidSelection"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DRunCache {
    desktop_entry: String,
    run_count: usize,
}

#[derive(Clone)]
struct DRunProvider<T: Clone> {
    items: Vec<MenuItem<T>>,
    cache_path: Option<PathBuf>,
    cache: HashMap<String, i64>,
}

impl<T: Clone> DRunProvider<T> {
    fn new(menu_item_data: T) -> Self {
        let locale_variants = get_locale_variants();
        let default_icon = default_icon().unwrap_or_default();

        let (cache_path, d_run_cache) = load_d_run_cache();

        let start = Instant::now();
        let mut entries: Vec<MenuItem<T>> = Vec::new();
        for file in find_desktop_files().iter().filter(|f| {
            f.entry.hidden.is_none_or(|hidden| !hidden)
                && f.entry.no_display.is_none_or(|no_display| !no_display)
        }) {
            let Some(name) = lookup_name_with_locale(
                &locale_variants,
                &file.entry.name.variants,
                &file.entry.name.default,
            ) else {
                log::warn!("Skipping desktop entry without name {file:?}");
                continue;
            };

            let (action, working_dir) = match &file.entry.entry_type {
                EntryType::Application(app) => (app.exec.clone(), app.path.clone()),
                _ => (None, None),
            };

            let cmd_exists = action
                .as_ref()
                .and_then(|a| {
                    a.split(' ')
                        .next()
                        .map(|cmd| cmd.replace('"', ""))
                        .map(|cmd| PathBuf::from(&cmd).exists() || which::which(&cmd).is_ok())
                })
                .unwrap_or(false);

            if !cmd_exists {
                log::warn!(
                    "Skipping desktop entry for {name:?} because action {action:?} does not exist"
                );
                continue;
            }

            let icon = file
                .entry
                .icon
                .as_ref()
                .map(|s| s.content.clone())
                .or(Some(default_icon.clone()));
            log::debug!("file, name={name:?}, icon={icon:?}, action={action:?}");
            let sort_score = d_run_cache.get(&name).unwrap_or(&0);

            let mut entry: MenuItem<T> = MenuItem {
                label: name,
                icon_path: icon.clone(),
                action,
                sub_elements: Vec::default(),
                working_dir: working_dir.clone(),
                initial_sort_score: -(*sort_score),
                search_sort_score: 0.0,
                data: Some(menu_item_data.clone()),
                visible: true,
            };

            file.actions.iter().for_each(|(_, action)| {
                if let Some(action_name) = lookup_name_with_locale(
                    &locale_variants,
                    &action.name.variants,
                    &action.name.default,
                ) {
                    let action_icon = action
                        .icon
                        .as_ref()
                        .map(|s| s.content.clone())
                        .or(icon.clone());

                    log::debug!("sub, action_name={action_name:?}, action_icon={action_icon:?}");

                    let sub_entry = MenuItem {
                        label: action_name,
                        icon_path: action_icon,
                        action: action.exec.clone(),
                        sub_elements: Vec::default(),
                        working_dir: working_dir.clone(),
                        initial_sort_score: 0, // subitems are never sorted right now.
                        search_sort_score: 0.0,
                        data: None,
                        visible: true,
                    };
                    entry.sub_elements.push(sub_entry);
                }
            });

            entries.push(entry);
        }

        log::info!(
            "parsing desktop files took {}ms",
            start.elapsed().as_millis()
        );

        gui::sort_menu_items_alphabetically_honor_initial_score(&mut entries);

        DRunProvider {
            items: entries,
            cache_path,
            cache: d_run_cache,
        }
    }
}

impl<T: Clone> ItemProvider<T> for DRunProvider<T> {
    fn get_elements(&mut self, _: Option<&str>) -> Vec<MenuItem<T>> {
        self.items.clone()
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> Option<Vec<MenuItem<T>>> {
        None
    }
}

#[derive(Clone)]
struct FileItemProvider<T: Clone> {
    last_result: Option<Vec<MenuItem<T>>>,
    menu_item_data: T,
}

impl<T: Clone> FileItemProvider<T> {
    fn new(menu_item_data: T) -> Self {
        FileItemProvider {
            last_result: None,
            menu_item_data,
        }
    }

    fn resolve_icon_for_name(path: &Path) -> String {
        let result = tree_magic_mini::from_filepath(path);
        if let Some(result) = result {
            if result.starts_with("image") {
                "image-x-generic".to_owned()
            } else if result.starts_with("inode") {
                return result.replace('/', "-");
            } else if result.starts_with("text") {
                if result.contains("plain") {
                    "text-x-generic".to_owned()
                } else if result.contains("python") {
                    "text-x-script".to_owned()
                } else if result.contains("html") {
                    return "text-html".to_owned();
                } else {
                    "text-x-generic".to_owned()
                }
            } else if result.starts_with("application") {
                if result.contains("octet") {
                    "application-x-executable".to_owned()
                } else if result.contains("tar")
                    || result.contains("lz")
                    || result.contains("zip")
                    || result.contains("7z")
                    || result.contains("xz")
                {
                    "package-x-generic".to_owned()
                } else {
                    return "text-html".to_owned();
                }
            } else {
                log::debug!("unsupported mime type {result}");
                return "application-x-generic".to_owned();
            }
        } else {
            "image-not-found".to_string()
        }
    }
}

impl<T: Clone> ItemProvider<T> for FileItemProvider<T> {
    fn get_elements(&mut self, search: Option<&str>) -> Vec<MenuItem<T>> {
        let default_path = if let Some(home) = dirs::home_dir() {
            home.display().to_string()
        } else {
            "/".to_string()
        };

        let mut trimmed_search = search.unwrap_or(&default_path).to_owned();
        if !trimmed_search.starts_with('/') && !trimmed_search.starts_with('~') {
            trimmed_search = format!("{default_path}/{trimmed_search}");
        }

        let path = expand_path(&trimmed_search);
        let mut items: Vec<MenuItem<T>> = Vec::new();

        if !path.exists() {
            if let Some(last) = &self.last_result {
                return last.clone();
            }

            return vec![];
        }

        if path.is_dir() {
            if let Ok(entries) = path.read_dir() {
                for entry in entries.flatten() {
                    if let Some(mut path_str) =
                        entry.path().to_str().map(std::string::ToString::to_string)
                    {
                        if trimmed_search.starts_with('~') {
                            if let Some(home_dir) = dirs::home_dir() {
                                if let Some(home_str) = home_dir.to_str() {
                                    path_str = path_str.replace(home_str, "~");
                                }
                            }
                        }

                        if entry.path().is_dir() {
                            path_str.push('/');
                        }

                        items.push(MenuItem {
                            label: path_str.clone(),
                            icon_path: Some(FileItemProvider::<T>::resolve_icon_for_name(
                                &entry.path(),
                            )),
                            action: Some(format!("xdg-open {path_str}")),
                            sub_elements: vec![],
                            working_dir: None,
                            initial_sort_score: 0,
                            search_sort_score: 0.0,
                            data: Some(self.menu_item_data.clone()),
                            visible: true,
                        });
                    }
                }
            }
        } else {
            items.push({
                MenuItem {
                    label: trimmed_search.clone(),
                    icon_path: Some(FileItemProvider::<T>::resolve_icon_for_name(
                        &PathBuf::from(&trimmed_search),
                    )),
                    action: Some(format!("xdg-open {trimmed_search}")),
                    sub_elements: vec![],
                    working_dir: None,
                    initial_sort_score: 0,
                    search_sort_score: 0.0,
                    data: Some(self.menu_item_data.clone()),
                    visible: true,
                }
            });
        }

        gui::sort_menu_items_alphabetically_honor_initial_score(&mut items);

        self.last_result = Some(items.clone());
        items
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> Option<Vec<MenuItem<T>>> {
        self.last_result.clone()
    }
}

#[derive(Clone)]
struct MathProvider<T: Clone> {
    menu_item_data: T,
}

impl<T: Clone> MathProvider<T> {
    fn new(menu_item_data: T) -> Self {
        Self { menu_item_data }
    }

    fn contains_math_functions_or_starts_with_number(input: &str) -> bool {
        // Regex for function names (word boundaries to match whole words)
        let math_functions = r"\b(sqrt|abs|exp|ln|sin|cos|tan|asin|acos|atan|atan2|sinh|cosh|tanh|asinh|acosh|atanh|floor|ceil|round|signum|min|max|pi|e)\b";

        // Regex for strings that start with a number (including decimals)
        let starts_with_number = r"^\s*[+-]?(\d+(\.\d*)?|\.\d+)";

        let math_regex = Regex::new(math_functions).unwrap();
        let number_regex = Regex::new(starts_with_number).unwrap();

        math_regex.is_match(input) || number_regex.is_match(input)
    }
}

impl<T: Clone> ItemProvider<T> for MathProvider<T> {
    fn get_elements(&mut self, search: Option<&str>) -> Vec<MenuItem<T>> {
        if let Some(search_text) = search {
            let result = match meval::eval_str(search_text) {
                Ok(result) => result.to_string(),
                Err(e) => format!("failed to calculate {e:?}"),
            };

            let item = MenuItem {
                label: result,
                icon_path: None,
                action: search.map(String::from),
                sub_elements: vec![],
                working_dir: None,
                initial_sort_score: 0,
                search_sort_score: 0.0,
                data: Some(self.menu_item_data.clone()),
                visible: true,
            };

            vec![item]
        } else {
            vec![]
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> Option<Vec<MenuItem<T>>> {
        None
    }
}

#[derive(Clone)]
struct DMenuProvider {
    items: Vec<MenuItem<String>>,
}

impl DMenuProvider {
    fn new() -> Result<DMenuProvider, ModeError> {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|_| ModeError::StdInReadFail)?;

        let items: Vec<MenuItem<String>> = input
            .lines()
            .map(String::from)
            .map(|s| MenuItem {
                label: s,
                icon_path: None,
                action: None,
                sub_elements: vec![],
                working_dir: None,
                initial_sort_score: 0,
                search_sort_score: 0.0,
                data: None,
                visible: true,
            })
            .collect();

        Ok(Self { items })
    }
}

impl ItemProvider<String> for DMenuProvider {
    fn get_elements(&mut self, _: Option<&str>) -> Vec<MenuItem<String>> {
        self.items.clone()
    }

    fn get_sub_elements(&mut self, _: &MenuItem<String>) -> Option<Vec<MenuItem<String>>> {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
enum AutoRunType {
    Math,
    DRun,
    File,
    // Ssh,
    // WebSearch,
    // Emoji,
    // Run,
}

#[derive(Clone)]
struct AutoItemProvider {
    drun: DRunProvider<AutoRunType>,
    file: FileItemProvider<AutoRunType>,
    math: MathProvider<AutoRunType>,
}

impl AutoItemProvider {
    fn new() -> Self {
        AutoItemProvider {
            drun: DRunProvider::new(AutoRunType::DRun),
            file: FileItemProvider::new(AutoRunType::File),
            math: MathProvider::new(AutoRunType::Math),
        }
    }
}

impl ItemProvider<AutoRunType> for AutoItemProvider {
    fn get_elements(&mut self, search_opt: Option<&str>) -> Vec<MenuItem<AutoRunType>> {
        if let Some(search) = search_opt {
            let trimmed_search = search.trim();
            if trimmed_search.is_empty() {
                self.drun.get_elements(search_opt)
            } else if MathProvider::<AutoRunType>::contains_math_functions_or_starts_with_number(
                trimmed_search,
            ) {
                self.math.get_elements(search_opt)
            } else if trimmed_search.starts_with('$')
                || trimmed_search.starts_with('/')
                || trimmed_search.starts_with('~')
            {
                self.file.get_elements(search_opt)
            } else {
                self.drun.get_elements(search_opt)
            }
        } else {
            self.drun.get_elements(search_opt)
        }
    }

    fn get_sub_elements(
        &mut self,
        item: &MenuItem<AutoRunType>,
    ) -> Option<Vec<MenuItem<AutoRunType>>> {
        Some(self.get_elements(Some(item.label.as_ref())))
    }
}

/// # Errors
///
/// Will return `Err` if it was not able to spawn the process
pub fn d_run(config: &Config) -> Result<(), ModeError> {
    let provider = DRunProvider::new(String::new());
    let cache_path = provider.cache_path.clone();
    let mut cache = provider.cache.clone();

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(config.clone(), provider);
    match selection_result {
        Ok(s) => update_drun_cache_and_run(cache_path, &mut cache, s)?,
        Err(_) => {
            log::error!("No item selected");
        }
    }

    Ok(())
}

/// # Errors
///
/// Will return `Err`
/// * if it was not able to spawn the process
pub fn auto(config: &Config) -> Result<(), ModeError> {
    let provider = AutoItemProvider::new();
    let cache_path = provider.drun.cache_path.clone();
    let mut cache = provider.drun.cache.clone();

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(config.clone(), provider);

    match selection_result {
        Ok(selection_result) => {
            if let Some(data) = &selection_result.data {
                match data {
                    AutoRunType::Math => {}
                    AutoRunType::DRun => {
                        update_drun_cache_and_run(cache_path, &mut cache, selection_result)?;
                    }
                    AutoRunType::File => {
                        if let Some(action) = selection_result.action {
                            spawn_fork(&action, selection_result.working_dir.as_ref())?;
                        }
                    }
                }
            }
        }
        Err(_) => {
            log::error!("No item selected");
        }
    }

    Ok(())
}

/// # Errors
///
/// Will return `Err`
/// * if it was not able to spawn the process
pub fn file(config: &Config) -> Result<(), ModeError> {
    let provider = FileItemProvider::new(String::new());

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(config.clone(), provider);
    match selection_result {
        Ok(s) => {
            if let Some(action) = s.action {
                spawn_fork(&action, s.working_dir.as_ref())?;
            }
        }
        Err(_) => {
            log::error!("No item selected");
        }
    }

    Ok(())
}

pub fn math(config: &Config) {
    let provider = MathProvider::new(String::new);

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(config.clone(), provider);
    match selection_result {
        Ok(_) => {}
        Err(_) => {
            log::error!("No item selected");
        }
    }
}

/// # Errors
///
/// todo
pub fn dmenu(config: &Config) -> Result<(), ModeError> {
    let provider = DMenuProvider::new()?;

    let selection_result = gui::show(config.clone(), provider);
    match selection_result {
        Ok(s) => {
            println!("{}", s.label);
            Ok(())
        }
        Err(_) => Err(ModeError::InvalidSelection),
    }
}

fn update_drun_cache_and_run<T: Clone>(
    cache_path: Option<PathBuf>,
    cache: &mut HashMap<String, i64>,
    selection_result: MenuItem<T>,
) -> Result<(), ModeError> {
    if let Some(cache_path) = cache_path {
        *cache.entry(selection_result.label).or_insert(0) += 1;
        if let Err(e) = save_cache_file(&cache_path, cache) {
            log::warn!("cannot save drun cache {e:?}");
        }
    }

    if let Some(action) = selection_result.action {
        spawn_fork(&action, selection_result.working_dir.as_ref())
    } else {
        Err(ModeError::MissingAction)
    }
}

fn load_d_run_cache() -> (Option<PathBuf>, HashMap<String, i64>) {
    let cache_path = dirs::cache_dir().map(|x| x.join("worf-drun"));
    let d_run_cache = {
        if let Some(ref cache_path) = cache_path {
            if let Err(e) = create_file_if_not_exists(cache_path) {
                log::warn!("No drun cache file and cannot create: {e:?}");
            }
        }

        load_cache_file(cache_path.as_ref()).unwrap_or_default()
    };
    (cache_path, d_run_cache)
}

fn save_cache_file(path: &PathBuf, data: &HashMap<String, i64>) -> anyhow::Result<()> {
    // Convert the HashMap to TOML string
    let toml_string = toml::ser::to_string(&data).map_err(|e| anyhow::anyhow!(e))?;
    fs::write(path, toml_string).map_err(|e| anyhow::anyhow!(e))
}

fn load_cache_file(cache_path: Option<&PathBuf>) -> Result<HashMap<String, i64>, ModeError> {
    let Some(path) = cache_path else {
        return Err(ModeError::MissingCache);
    };

    let toml_content =
        fs::read_to_string(path).map_err(|e| ModeError::UpdateCacheError(format!("{e}")))?;
    let parsed: toml::Value = toml_content.parse().expect("Failed to parse TOML");

    let mut result: HashMap<String, i64> = HashMap::new();
    if let toml::Value::Table(table) = parsed {
        for (key, val) in table {
            if let toml::Value::Integer(i) = val {
                result.insert(key, i);
            } else {
                log::warn!("Skipping key '{key}' because it's not an integer");
            }
        }
    }
    Ok(result)
}

fn create_file_if_not_exists(path: &PathBuf) -> anyhow::Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path);

    match file {
        Ok(_) => Ok(()),

        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e).context(format!("Failed to create file {}", path.display()))?,
    }
}

fn spawn_fork(cmd: &str, working_dir: Option<&String>) -> Result<(), ModeError> {
    // todo fix actions ??
    // todo graphical disk map icon not working

    let parts = cmd.split(' ').collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(ModeError::MissingAction);
    }

    if let Some(dir) = working_dir {
        env::set_current_dir(dir)
            .map_err(|e| ModeError::RunError(format!("cannot set workdir {e}")))?;
    }

    let exec = parts[0].replace('"', "");
    let args: Vec<_> = parts
        .iter()
        .skip(1)
        .filter(|arg| !arg.starts_with('%'))
        .map(|arg| expand_path(arg))
        .collect();

    unsafe {
        let _ = Command::new(exec)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn();
    }
    Ok(())
}

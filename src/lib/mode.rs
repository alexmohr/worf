use crate::config::{Config, expand_path};
use crate::desktop::{
    default_icon, find_desktop_files, get_locale_variants, lookup_name_with_locale,
};
use crate::gui::{ItemProvider, MenuItem};
use crate::{config, desktop, gui};
use anyhow::{Context, Error, anyhow};
use freedesktop_file_parser::EntryType;
use gtk4::Image;
use libc::option;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::prelude::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, fs, io};

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DRunCache {
    desktop_entry: String,
    run_count: usize,
}

#[derive(Clone)]
struct DRunProvider<T: std::clone::Clone> {
    items: Vec<MenuItem<T>>,
    cache_path: Option<PathBuf>,
    cache: HashMap<String, i64>,
}

impl<T: Clone> DRunProvider<T> {
    fn new(menu_item_data: T) -> Self {
        let locale_variants = get_locale_variants();
        let default_icon = default_icon().unwrap_or_default();

        let (cache_path, d_run_cache) = load_d_run_cache();

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
            };

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
                    };
                    entry.sub_elements.push(sub_entry);
                }
            });

            entries.push(entry);
        }

        gui::initialize_sort_scores(&mut entries);

        DRunProvider {
            items: entries,
            cache_path,
            cache: d_run_cache,
        }
    }
}

impl<T: std::clone::Clone> ItemProvider<T> for DRunProvider<T> {
    fn get_elements(&mut self, _: Option<&str>) -> Vec<MenuItem<T>> {
        self.items.clone()
    }

    fn get_sub_elements(&mut self, item: &MenuItem<T>) -> Option<Vec<MenuItem<T>>> {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
enum AutoRunType {
    Math,
    DRun,
    File,
    Ssh,
    WebSearch,
    Emoji,
    Run,
}

#[derive(Clone)]
struct AutoItemProvider {
    drun_provider: DRunProvider<AutoRunType>,
    last_result: Option<Vec<MenuItem<AutoRunType>>>,
}

impl AutoItemProvider {
    fn new() -> Self {
        AutoItemProvider {
            drun_provider: DRunProvider::new(AutoRunType::DRun),

            last_result: None,
        }
    }

    fn auto_run_handle_files(&mut self, trimmed_search: &str) -> Vec<MenuItem<AutoRunType>> {
        let folder_icon = "inode-directory";

        let path = config::expand_path(trimmed_search);
        let mut items: Vec<MenuItem<AutoRunType>> = Vec::new();

        if !path.exists() {
            if let Some(last) = &self.last_result {
                if !last.is_empty()
                    && last.first().is_some_and(|l| {
                        l.as_ref()
                            .data
                            .as_ref()
                            .is_some_and(|t| t == &AutoRunType::File)
                    })
                {
                    return last.clone();
                }
            }

            return vec![];
        }

        if path.is_dir() {
            for entry in path.read_dir().unwrap() {
                if let Ok(entry) = entry {
                    let mut path_str = entry.path().to_str().unwrap_or("").to_string();
                    if trimmed_search.starts_with("~") {
                        if let Some(home_dir) = dirs::home_dir() {
                            path_str = path_str.replace(home_dir.to_str().unwrap_or(""), "~");
                        }
                    }

                    if entry.path().is_dir() {
                        path_str += "/";
                    }

                    items.push({
                        MenuItem {
                            label: path_str.clone(),
                            icon_path: if entry.path().is_dir() {
                                Some(folder_icon.to_owned())
                            } else {
                                Some(resolve_icon_for_name(entry.path()))
                            },
                            action: Some(format!("xdg-open {path_str}")),
                            sub_elements: vec![],
                            working_dir: None,
                            initial_sort_score: 0,
                            search_sort_score: 0.0,
                            data: Some(AutoRunType::File),
                        }
                    });
                }
            }
        } else {
            items.push({
                MenuItem {
                    label: trimmed_search.to_owned(),
                    icon_path: Some(resolve_icon_for_name(PathBuf::from(trimmed_search))),
                    action: Some(format!("xdg-open {trimmed_search}")),
                    sub_elements: vec![],
                    working_dir: None,
                    initial_sort_score: 0,
                    search_sort_score: 0.0,
                    data: Some(AutoRunType::File),
                }
            });
        }

        self.last_result = Some(items.clone());
        items
    }
}

fn resolve_icon_for_name(path: PathBuf) -> String {
    // todo use https://docs.rs/tree_magic_mini/latest/tree_magic_mini/ instead
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() {
            return "inode-symlink".to_owned();
        } else if metadata.is_dir() {
            return "inode-directory".to_owned();
        } else if metadata.permissions().mode() & 0o111 != 0 {
            return "application-x-executable".to_owned();
        }
    }

    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("")
        .to_lowercase();

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "sh" | "py" | "rb" | "pl" | "bash" => "text-x-script".to_owned(),
        "c" | "cpp" | "rs" | "java" | "js" | "h" | "hpp" => "text-x-generic".to_owned(),
        "txt" | "md" | "log" => "text-x-generic".to_owned(),
        "html" | "htm" => "text-html".to_owned(),
        "jpg" | "jpeg" | "png" | "gif" | "svg" | "webp" => "image-x-generic".to_owned(),
        "mp3" | "wav" | "ogg" => "audio-x-generic".to_owned(),
        "mp4" | "mkv" | "avi" => "video-x-generic".to_owned(),
        "ttf" | "otf" | "woff" => "font-x-generic".to_owned(),
        "zip" | "tar" | "gz" | "xz" | "7z" | "lz4" => "package-x-generic".to_owned(),
        "deb" | "rpm" | "apk" => "x-package-repository".to_owned(),
        "odt" => "x-office-document".to_owned(),
        "ott" => "x-office-document-template".to_owned(),
        "ods" => "x-office-spreadsheet".to_owned(),
        "ots" => "x-office-spreadsheet-template".to_owned(),
        "odp" => "x-office-presentation".to_owned(),
        "otp" => "x-office-presentation-template".to_owned(),
        "odg" => "x-office-drawing".to_owned(),
        "vcf" => "x-office-addressbook".to_owned(),
        _ => "application-x-generic".to_owned(),
    }
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

impl ItemProvider<AutoRunType> for AutoItemProvider {
    fn get_elements(&mut self, search_opt: Option<&str>) -> Vec<MenuItem<AutoRunType>> {
        if let Some(search) = search_opt {
            let trimmed_search = search.trim();
            if trimmed_search.is_empty() {
                self.drun_provider.get_elements(search_opt)
            } else if contains_math_functions_or_starts_with_number(trimmed_search) {
                let result = match meval::eval_str(trimmed_search) {
                    Ok(result) => result.to_string(),
                    Err(e) => format!("failed to calculate {e:?}"),
                };

                let item = MenuItem {
                    label: result,
                    icon_path: None,
                    action: None,
                    sub_elements: vec![],
                    working_dir: None,
                    initial_sort_score: 0,
                    search_sort_score: 0.0,
                    data: Some(AutoRunType::Math),
                };

                return vec![item];
            } else if trimmed_search.starts_with("$")
                || trimmed_search.starts_with("/")
                || trimmed_search.starts_with("~")
            {
                self.auto_run_handle_files(trimmed_search)
            } else {
                return self.drun_provider.get_elements(search_opt);
            }
        } else {
            self.drun_provider.get_elements(search_opt)
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
pub fn d_run(config: &mut Config) -> anyhow::Result<()> {
    let provider = DRunProvider::new("".to_owned());
    let cache_path = provider.cache_path.clone();
    let mut cache = provider.cache.clone();
    if config.prompt.is_none() {
        config.prompt = Some("drun".to_owned());
    }

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(config.clone(), provider);
    match selection_result {
        Ok(s) => {
            update_drun_cache_and_run(cache_path, &mut cache, s)?;
        }
        Err(_) => {
            log::error!("No item selected");
        }
    }

    Ok(())
}

pub fn auto(config: &mut Config) -> anyhow::Result<()> {
    let provider = AutoItemProvider::new();
    let cache_path = provider.drun_provider.cache_path.clone();
    let mut cache = provider.drun_provider.cache.clone();

    if config.prompt.is_none() {
        config.prompt = Some("auto".to_owned());
    }

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
                            spawn_fork(&action, selection_result.working_dir.as_ref())?
                        }
                    }
                    _ => {
                        todo!("not supported yet");
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

fn update_drun_cache_and_run<T: Clone>(
    cache_path: Option<PathBuf>,
    cache: &mut HashMap<String, i64>,
    selection_result: MenuItem<T>,
) -> Result<(), Error> {
    if let Some(cache_path) = cache_path {
        *cache.entry(selection_result.label).or_insert(0) += 1;
        if let Err(e) = save_cache_file(&cache_path, &cache) {
            log::warn!("cannot save drun cache {e:?}");
        }
    }

    if let Some(action) = selection_result.action {
        spawn_fork(&action, selection_result.working_dir.as_ref())
    } else {
        Err(anyhow::anyhow!("cannot find drun action"))
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

fn load_cache_file(cache_path: Option<&PathBuf>) -> anyhow::Result<HashMap<String, i64>> {
    let Some(path) = cache_path else {
        return Err(anyhow!("Cache is missing"));
    };

    let toml_content = fs::read_to_string(path)?;
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

fn spawn_fork(cmd: &str, working_dir: Option<&String>) -> anyhow::Result<()> {
    // todo fix actions ??
    // todo graphical disk map icon not working

    let parts = cmd.split(' ').collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(anyhow!("empty command passed"));
    }

    if let Some(dir) = working_dir {
        env::set_current_dir(dir)?;
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

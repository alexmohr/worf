use crate::config::{Config, expand_path};
use crate::desktop::{
    create_file_if_not_exists, find_desktop_files, get_locale_variants, load_cache_file,
    lookup_name_with_locale, save_cache_file, spawn_fork,
};
use crate::gui::{ItemProvider, MenuItem};
use crate::{Error, gui};
use freedesktop_file_parser::EntryType;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{fs, io};

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DRunCache {
    desktop_entry: String,
    run_count: usize,
}

#[derive(Clone)]
struct DRunProvider<T: Clone> {
    items: Option<Vec<MenuItem<T>>>,
    cache_path: Option<PathBuf>,
    cache: HashMap<String, i64>,
    data: T,
}

impl<T: Clone + Send + Sync> DRunProvider<T> {
    fn new(menu_item_data: T) -> Self {
        let (cache_path, d_run_cache) = load_d_run_cache();
        DRunProvider {
            items: None,
            cache_path,
            cache: d_run_cache,
            data: menu_item_data,
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_precision_loss)]
    fn load(&self) -> Vec<MenuItem<T>> {
        let locale_variants = get_locale_variants();
        let default_icon = "application-x-executable".to_string();
        let start = Instant::now();

        let entries: Vec<MenuItem<T>> = find_desktop_files()
            .into_par_iter()
            .filter(|file| {
                !file.entry.no_display.unwrap_or(false)
                    && !file.entry.hidden.unwrap_or(false)
            })
            .filter_map(|file| {
                let name = lookup_name_with_locale(
                    &locale_variants,
                    &file.entry.name.variants,
                    &file.entry.name.default,
                )?;

                let (action, working_dir) = match &file.entry.entry_type {
                    EntryType::Application(app) => (app.exec.clone(), app.path.clone()),
                    _ => return None,
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
                    log::warn!("Skipping desktop entry for {name:?} because action {action:?} does not exist");
                    return None;
                }

                let icon = file
                    .entry
                    .icon
                    .as_ref()
                    .map(|s| s.content.clone())
                    .or(Some(default_icon.clone()));

                let sort_score = *self.cache.get(&name).unwrap_or(&0) as f64;

                let mut entry = MenuItem::new(
                     name.clone(),
                     icon.clone(),
                     action.clone(),
                     Vec::new(),
                     working_dir.clone(),
                     sort_score,
                     Some(self.data.clone()),
                     );

                for action in file.actions.values() {
                    if let Some(action_name) = lookup_name_with_locale(
                        &locale_variants,
                        &action.name.variants,
                        &action.name.default,
                    ) {
                        let action_icon = action
                            .icon
                            .as_ref()
                            .map(|s| s.content.clone())
                            .or(icon.clone())
                            .unwrap_or("application-x-executable".to_string());


                        entry.sub_elements.push(MenuItem::new(
                            action_name,
                            Some(action_icon),
                            action.exec.clone(),
                            Vec::new(),
                            working_dir.clone(),
                            0.0,
                            Some(self.data.clone()),
                        ));
                    }
                }

                Some(entry)
            })
            .collect();

        let mut seen_actions = HashSet::new();
        let mut entries: Vec<MenuItem<T>> = entries
            .into_iter()
            .filter(|entry| seen_actions.insert(entry.action.clone()))
            .collect();

        log::info!(
            "parsing desktop files took {}ms",
            start.elapsed().as_millis()
        );

        gui::sort_menu_items_alphabetically_honor_initial_score(&mut entries);
        entries
    }
}

impl<T: Clone + Send + Sync> ItemProvider<T> for DRunProvider<T> {
    fn get_elements(&mut self, _: Option<&str>) -> Vec<MenuItem<T>> {
        if self.items.is_none() {
            self.items = Some(self.load().clone());
        }
        self.items.clone().unwrap()
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

                        items.push(MenuItem::new(
                            path_str.clone(),
                            Some(FileItemProvider::<T>::resolve_icon_for_name(&entry.path())),
                            Some(format!("xdg-open {path_str}")),
                            vec![],
                            None,
                            0.0,
                            Some(self.menu_item_data.clone()),
                        ));
                    }
                }
            }
        } else {
            items.push({
                MenuItem::new(
                    trimmed_search.clone(),
                    Some(FileItemProvider::<T>::resolve_icon_for_name(
                        &PathBuf::from(&trimmed_search),
                    )),
                    Some(format!("xdg-open {trimmed_search}")),
                    vec![],
                    None,
                    0.0,
                    Some(self.menu_item_data.clone()),
                )
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
struct SshProvider<T: Clone> {
    elements: Vec<MenuItem<T>>,
}

impl<T: Clone> SshProvider<T> {
    fn new(menu_item_data: T, config: &Config) -> Self {
        let re = Regex::new(r"(?m)^\s*Host\s+(.+)$").unwrap();
        let items: Vec<_> = dirs::home_dir()
            .map(|home| home.join(".ssh").join("config"))
            .filter(|path| path.exists())
            .map(|path| fs::read_to_string(&path).unwrap_or_default())
            .into_iter()
            .flat_map(|content| {
                re.captures_iter(&content)
                    .flat_map(|cap| {
                        cap[1]
                            .split_whitespace()
                            .map(|host| {
                                log::debug!("found ssh host {host}");
                                MenuItem::new(
                                    host.to_owned(),
                                    Some("computer".to_owned()),
                                    config.term.clone().map(|cmd| format!("{cmd} ssh {host}")),
                                    vec![],
                                    None,
                                    0.0,
                                    Some(menu_item_data.clone()),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        Self { elements: items }
    }
}

impl<T: Clone> ItemProvider<T> for SshProvider<T> {
    fn get_elements(&mut self, _: Option<&str>) -> Vec<MenuItem<T>> {
        self.elements.clone()
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> Option<Vec<MenuItem<T>>> {
        None
    }
}

#[derive(Clone)]
struct MathProvider<T: Clone> {
    menu_item_data: T,
    elements: Vec<MenuItem<T>>,
}

impl<T: Clone> MathProvider<T> {
    fn new(menu_item_data: T) -> Self {
        Self {
            menu_item_data,
            elements: vec![],
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

    fn add_elements(&mut self, elements: &mut Vec<MenuItem<T>>) {
        self.elements.append(elements);
    }
}

impl<T: Clone> ItemProvider<T> for MathProvider<T> {
    fn get_elements(&mut self, search: Option<&str>) -> Vec<MenuItem<T>> {
        if let Some(search_text) = search {
            let result = match meval::eval_str(search_text) {
                Ok(result) => result.to_string(),
                Err(e) => format!("failed to calculate {e:?}"),
            };

            let item = MenuItem::new(
                result,
                None,
                search.map(String::from),
                vec![],
                None,
                0.0,
                Some(self.menu_item_data.clone()),
            );
            let mut result = vec![item];
            result.append(&mut self.elements.clone());
            result
        } else {
            self.elements.clone()
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
    fn new() -> Result<DMenuProvider, Error> {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|_| Error::StdInReadFail)?;

        let items: Vec<MenuItem<String>> = input
            .lines()
            .map(String::from)
            .map(|s| MenuItem::new(s.clone(), None, None, vec![], None, 0.0, None))
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
    Ssh,
    // WebSearch,
    // Emoji,
    // Run,
}

#[derive(Clone)]
struct AutoItemProvider {
    drun: DRunProvider<AutoRunType>,
    file: FileItemProvider<AutoRunType>,
    math: MathProvider<AutoRunType>,
    ssh: SshProvider<AutoRunType>,
}

impl AutoItemProvider {
    fn new(config: &Config) -> Self {
        AutoItemProvider {
            drun: DRunProvider::new(AutoRunType::DRun),
            file: FileItemProvider::new(AutoRunType::File),
            math: MathProvider::new(AutoRunType::Math),
            ssh: SshProvider::new(AutoRunType::Ssh, config),
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
            } else if trimmed_search.starts_with("ssh") {
                self.ssh.get_elements(search_opt)
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

/// Shows the drun mode
/// # Errors
///
/// Will return `Err` if it was not able to spawn the process
pub fn d_run(config: &Config) -> Result<(), Error> {
    let provider = DRunProvider::new(String::new());
    let cache_path = provider.cache_path.clone();
    let mut cache = provider.cache.clone();

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(config.clone(), provider, false);
    match selection_result {
        Ok(s) => update_drun_cache_and_run(cache_path, &mut cache, s)?,
        Err(_) => {
            log::error!("No item selected");
        }
    }

    Ok(())
}

/// Shows the auto mode
/// # Errors
///
/// Will return `Err`
/// * if it was not able to spawn the process
pub fn auto(config: &Config) -> Result<(), Error> {
    let mut provider = AutoItemProvider::new(config);
    let cache_path = provider.drun.cache_path.clone();
    let mut cache = provider.drun.cache.clone();
    let mut cfg_clone = config.clone();

    loop {
        // todo ues a arc instead of cloning the config
        let selection_result = gui::show(cfg_clone.clone(), provider.clone(), true);

        if let Ok(mut selection_result) = selection_result {
            if let Some(data) = &selection_result.data {
                match data {
                    AutoRunType::Math => {
                        cfg_clone.prompt = Some(selection_result.label.clone());
                        provider.math.elements.push(selection_result);
                    }
                    AutoRunType::DRun => {
                        update_drun_cache_and_run(cache_path, &mut cache, selection_result)?;
                        break;
                    }
                    AutoRunType::File => {
                        if let Some(action) = selection_result.action {
                            spawn_fork(&action, selection_result.working_dir.as_ref())?;
                        }
                        break;
                    }
                    AutoRunType::Ssh => {
                        ssh_launch(&selection_result, config)?;
                        break;
                    }
                }
            } else if selection_result.label.starts_with("ssh") {
                selection_result.label = selection_result.label.chars().skip(4).collect();
                ssh_launch(&selection_result, config)?;
            }
        } else {
            log::error!("No item selected");
            break;
        }
    }

    Ok(())
}

/// Shows the file browser mode
/// # Errors
///
/// Will return `Err`
/// * if it was not able to spawn the process
pub fn file(config: &Config) -> Result<(), Error> {
    let provider = FileItemProvider::new(String::new());

    // todo ues a arc instead of cloning the config
    let selection_result = gui::show(config.clone(), provider, false);
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

fn ssh_launch<T: Clone>(menu_item: &MenuItem<T>, config: &Config) -> Result<(), Error> {
    if let Some(action) = &menu_item.action {
        spawn_fork(action, None)?;
    } else {
        let cmd = config
            .term
            .clone()
            .map(|s| format!("{s} ssh {}", menu_item.label));
        if let Some(cmd) = cmd {
            spawn_fork(&cmd, None)?;
        }
    }
    Err(Error::MissingAction)
}

/// Shows the ssh mode
/// # Errors
///
/// Will return `Err`
/// * if it was not able to spawn the process
/// * if it didn't find a terminal
pub fn ssh(config: &Config) -> Result<(), Error> {
    let provider = SshProvider::new(String::new(), config);
    let selection_result = gui::show(config.clone(), provider, true);
    if let Ok(mi) = selection_result {
        ssh_launch(&mi, config)?;
    } else {
        log::error!("No item selected");
    }
    Ok(())
}

/// Shows the math mode
pub fn math(config: &Config) {
    let mut cfg_clone = config.clone();
    let mut calc: Vec<MenuItem<String>> = vec![];
    loop {
        let mut provider = MathProvider::new(String::new());
        provider.add_elements(&mut calc.clone());
        let selection_result = gui::show(cfg_clone.clone(), provider, true);
        if let Ok(mi) = selection_result {
            cfg_clone.prompt = Some(mi.label.clone());
            calc.push(mi);
        } else {
            log::error!("No item selected");
            break;
        }
    }
}

/// Shows the dmenu mode
/// # Errors
///
/// Forwards errors from the gui. See `gui::show` for details.
pub fn dmenu(config: &Config) -> Result<(), Error> {
    let provider = DMenuProvider::new()?;

    let selection_result = gui::show(config.clone(), provider, true);
    match selection_result {
        Ok(s) => {
            println!("{}", s.label);
            Ok(())
        }
        Err(_) => Err(Error::InvalidSelection),
    }
}

fn update_drun_cache_and_run<T: Clone>(
    cache_path: Option<PathBuf>,
    cache: &mut HashMap<String, i64>,
    selection_result: MenuItem<T>,
) -> Result<(), Error> {
    if let Some(cache_path) = cache_path {
        *cache.entry(selection_result.label).or_insert(0) += 1;
        if let Err(e) = save_cache_file(&cache_path, cache) {
            log::warn!("cannot save drun cache {e:?}");
        }
    }

    if let Some(action) = selection_result.action {
        spawn_fork(&action, selection_result.working_dir.as_ref())
    } else {
        Err(Error::MissingAction)
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

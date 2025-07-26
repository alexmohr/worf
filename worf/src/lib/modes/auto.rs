use std::sync::{Arc, LazyLock, Mutex, RwLock};

use regex::Regex;

use crate::{
    Error,
    config::Config,
    desktop::spawn_fork,
    gui::{
        self, ArcProvider, DefaultItemFactory, ExpandMode, ItemProvider, MenuItem, ProviderData,
    },
    modes::{
        drun::{DRunProvider, update_drun_cache_and_run},
        file::FileItemProvider,
        math::MathProvider,
        search::SearchProvider,
        ssh,
        ssh::SshProvider,
    },
};

#[derive(Debug, Clone, PartialEq)]
enum AutoRunType {
    Math,
    DRun,
    File,
    Ssh,
    WebSearch,
    Auto,
}

#[derive(Clone)]
struct AutoItemProvider {
    drun: DRunProvider<AutoRunType>,
    file: FileItemProvider<AutoRunType>,
    math: MathProvider<AutoRunType>,
    ssh: SshProvider<AutoRunType>,
    search: SearchProvider<AutoRunType>,
    last_mode: Option<AutoRunType>,
}

impl AutoItemProvider {
    fn new(config: &Config) -> Self {
        AutoItemProvider {
            drun: DRunProvider::new(AutoRunType::DRun, config),
            file: FileItemProvider::new(AutoRunType::File, config.sort_order()),
            math: MathProvider::new(AutoRunType::Math),
            ssh: SshProvider::new(AutoRunType::Ssh, &config.sort_order()),
            search: SearchProvider::new(AutoRunType::WebSearch, config.search_query()),
            last_mode: None,
        }
    }

    fn default_auto_elements(&mut self) -> ProviderData<AutoRunType> {
        // return ssh and drun items
        if self.last_mode.is_none()
            || self
                .last_mode
                .as_ref()
                .is_some_and(|t| t != &AutoRunType::Auto)
        {
            let mut data = self.drun.get_elements(None);
            if let Some(items) = data.items.as_mut()
                && let Some(mut ssh) = self.ssh.get_elements(None).items
            {
                items.append(&mut ssh);
            }

            self.last_mode = Some(AutoRunType::Auto);
            data
        } else {
            ProviderData { items: None }
        }
    }
}

fn contains_math_functions_or_starts_with_number(input: &str) -> bool {
    // Regex for function names (word boundaries to match whole words)
    static MATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"\b(
            sqrt|abs|exp|ln|sin|cos|tan|
            asin|acos|atan|atan2|
            sinh|cosh|tanh|asinh|acosh|atanh|
            floor|ceil|round|signum|min|max|
            pi|e|
            0x|0b|
            \||&|<<|>>|\^
        )\b",
        )
        .unwrap()
    });

    // Regex for strings that start with a number (including decimals)
    static NUMBER_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*[+-]?(\d+(\.\d*)?|\.\d+)").unwrap());

    MATH_REGEX.is_match(input) || NUMBER_REGEX.is_match(input)
}

impl ItemProvider<AutoRunType> for AutoItemProvider {
    fn get_elements(&mut self, search_opt: Option<&str>) -> ProviderData<AutoRunType> {
        let search = match search_opt {
            Some(s) if !s.trim().is_empty() => s.trim(),
            _ => "",
        };

        let (mode, provider_data) = if contains_math_functions_or_starts_with_number(search) {
            (AutoRunType::Math, self.math.get_elements(search_opt))
        } else if search.starts_with('$') || search.starts_with('/') || search.starts_with('~') {
            (AutoRunType::File, self.file.get_elements(search_opt))
        } else if search.starts_with("ssh") {
            (AutoRunType::Ssh, self.ssh.get_elements(search_opt))
        } else if search.starts_with('?') {
            static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\?\s*").unwrap());
            let re = &*RE;
            let query = re.replace(search, "");
            (
                AutoRunType::WebSearch,
                self.search.get_elements(Some(&query)),
            )
        } else {
            (AutoRunType::Auto, self.default_auto_elements())
        };

        self.last_mode = Some(mode);
        provider_data
    }

    fn get_sub_elements(&mut self, item: &MenuItem<AutoRunType>) -> ProviderData<AutoRunType> {
        if let Some(auto_run_type) = item.data.as_ref() {
            match auto_run_type {
                AutoRunType::Math => self.math.get_sub_elements(item),
                AutoRunType::DRun => self.drun.get_sub_elements(item),
                AutoRunType::File => self.file.get_sub_elements(item),
                AutoRunType::Ssh => self.ssh.get_sub_elements(item),
                AutoRunType::WebSearch => self.search.get_sub_elements(item),
                AutoRunType::Auto => ProviderData { items: None },
            }
        } else {
            ProviderData { items: None }
        }
    }
}

/// Shows the auto mode
/// # Errors
///
/// Will return `Err`
/// * if it was not able to spawn the process
///
/// # Panics
/// Panics if an internal static regex cannot be passed anymore, should never happen
pub fn show(config: &Arc<RwLock<Config>>) -> Result<(), Error> {
    let provider = Arc::new(Mutex::new(AutoItemProvider::new(&config.read().unwrap())));
    let arc_provider = Arc::clone(&provider) as ArcProvider<AutoRunType>;
    let cache_path = provider.lock().unwrap().drun.cache_path.clone();
    let mut cache = provider.lock().unwrap().drun.cache.clone();

    loop {
        provider.lock().unwrap().last_mode = None;
        let selection_result = gui::show(
            config,
            Arc::clone(&arc_provider),
            Some(Arc::new(Mutex::new(DefaultItemFactory::new()))),
            Some(
                vec!["ssh", "emoji", "^\\$\\w+", "^\\?\\s*"]
                    .into_iter()
                    .map(|s| Regex::new(s).unwrap())
                    .collect(),
            ),
            ExpandMode::Verbatim,
            None,
        );

        if let Ok(selection_result) = selection_result {
            let mut selection_result = selection_result.menu;
            if let Some(data) = &selection_result.data {
                match data {
                    AutoRunType::Math => {
                        provider
                            .lock()
                            .unwrap()
                            .math
                            .elements
                            .push(selection_result);
                    }
                    AutoRunType::DRun => {
                        update_drun_cache_and_run(&cache_path, &mut cache, selection_result)?;
                        break;
                    }
                    AutoRunType::File => {
                        if let Some(action) = selection_result.action {
                            spawn_fork(&action, selection_result.working_dir.as_ref())?;
                        }
                        break;
                    }
                    AutoRunType::Ssh => {
                        ssh::launch(&selection_result, &config.read().unwrap())?;
                        break;
                    }
                    AutoRunType::WebSearch => {
                        if let Some(action) = selection_result.action {
                            spawn_fork(&action, None)?;
                        }
                        break;
                    }
                    AutoRunType::Auto => {
                        unreachable!("Auto mode should never be set for show.")
                    }
                }
            } else if selection_result.label.starts_with("ssh") {
                selection_result.label = selection_result.label.chars().skip(4).collect();
                ssh::launch(&selection_result, &config.read().unwrap())?;
            }
        } else {
            log::error!("No item selected");
            break;
        }
    }

    Ok(())
}

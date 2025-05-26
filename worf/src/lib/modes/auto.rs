use crate::config::Config;
use crate::desktop::spawn_fork;
use crate::gui::{ItemProvider, MenuItem};
use crate::modes::drun::{DRunProvider, update_drun_cache_and_run};
use crate::modes::file::FileItemProvider;
use crate::modes::math::MathProvider;
use crate::modes::ssh;
use crate::modes::ssh::SshProvider;
use crate::{Error, gui};
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
enum AutoRunType {
    Math,
    DRun,
    File,
    Ssh,
    // WebSearch,
}

#[derive(Clone)]
struct AutoItemProvider {
    drun: DRunProvider<AutoRunType>,
    file: FileItemProvider<AutoRunType>,
    math: MathProvider<AutoRunType>,
    ssh: SshProvider<AutoRunType>,
    last_mode: Option<AutoRunType>,
}

impl AutoItemProvider {
    fn new(config: &Config) -> Self {
        AutoItemProvider {
            drun: DRunProvider::new(
                AutoRunType::DRun,
                config.no_actions(),
                config.sort_order(),
                config.term(),
            ),
            file: FileItemProvider::new(AutoRunType::File, config.sort_order()),
            math: MathProvider::new(AutoRunType::Math),
            ssh: SshProvider::new(AutoRunType::Ssh, &config.sort_order()),
            last_mode: None,
        }
    }

    fn default_auto_elements(
        &mut self,
        search_opt: Option<&str>,
    ) -> (bool, Vec<MenuItem<AutoRunType>>) {
        // return ssh and drun items
        let (changed, mut items) = self.drun.get_elements(search_opt);
        items.append(&mut self.ssh.get_elements(search_opt).1);
        if self.last_mode == Some(AutoRunType::DRun) {
            (changed, items)
        } else {
            self.last_mode = Some(AutoRunType::DRun);
            (true, items)
        }
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
    fn get_elements(&mut self, search_opt: Option<&str>) -> (bool, Vec<MenuItem<AutoRunType>>) {
        let search = match search_opt {
            Some(s) if !s.trim().is_empty() => s.trim(),
            _ => return self.default_auto_elements(search_opt),
        };

        let (mode, (changed, items)) = if contains_math_functions_or_starts_with_number(search) {
            (AutoRunType::Math, self.math.get_elements(search_opt))
        } else if search.starts_with('$') || search.starts_with('/') || search.starts_with('~') {
            (AutoRunType::File, self.file.get_elements(search_opt))
        } else if search.starts_with("ssh") {
            (AutoRunType::Ssh, self.ssh.get_elements(search_opt))
        } else {
            return self.default_auto_elements(search_opt);
        };

        if self.last_mode.as_ref().is_some_and(|m| m == &mode) {
            (changed, items)
        } else {
            self.last_mode = Some(mode);
            (true, items)
        }
    }

    fn get_sub_elements(
        &mut self,
        item: &MenuItem<AutoRunType>,
    ) -> (bool, Option<Vec<MenuItem<AutoRunType>>>) {
        let (changed, items) = self.get_elements(Some(item.label.as_ref()));
        (changed, Some(items))
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
pub fn show(config: &Config) -> Result<(), Error> {
    let mut provider = AutoItemProvider::new(config);
    let cache_path = provider.drun.cache_path.clone();
    let mut cache = provider.drun.cache.clone();

    loop {
        // todo ues a arc instead of cloning the config
        let selection_result = gui::show(
            config.clone(),
            provider.clone(),
            true,
            Some(
                vec!["ssh", "emoji", "^\\$\\w+"]
                    .into_iter()
                    .map(|s| Regex::new(s).unwrap())
                    .collect(),
            ),
            None,
        );

        if let Ok(selection_result) = selection_result {
            let mut selection_result = selection_result.menu;
            if let Some(data) = &selection_result.data {
                match data {
                    AutoRunType::Math => {
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
                        ssh::launch(&selection_result, config)?;
                        break;
                    }
                }
            } else if selection_result.label.starts_with("ssh") {
                selection_result.label = selection_result.label.chars().skip(4).collect();
                ssh::launch(&selection_result, config)?;
            }
        } else {
            log::error!("No item selected");
            break;
        }
    }

    Ok(())
}

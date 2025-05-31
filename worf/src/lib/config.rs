use std::{env, fs, path::PathBuf, str::FromStr};

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::Error;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Anchor {
    Top,
    Left,
    Bottom,
    Right,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum MatchMethod {
    Fuzzy,
    Contains,
    MultiContains,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum Align {
    Fill,
    Start,
    Center,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WrapMode {
    None,
    Word,
    Inherit,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SortOrder {
    Default,
    Alphabetical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum CustomKeyHintLocation {
    Top,
    Bottom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum KeyDetectionType {
    /// Raw keyboard value, might not be correct all layouts
    Code,
    /// The value of the key, but note that shift+3 != 3 (as shift+3 = #)
    Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Mode {
    /// searches `$PATH` for executables and allows them to be run by selecting them.
    Run,
    /// searches `$XDG_DATA_HOME/applications` and `$XDG_DATA_DIRS/applications`
    /// for desktop files and allows them to be run by selecting them.
    Drun,

    /// reads from stdin and displays options which when selected will be output to stdout.
    Dmenu,

    /// tries to determine automatically what to do
    Auto,

    /// use worf as file browser
    File,

    /// Use is as calculator
    Math,

    /// Connect via ssh to a given host
    Ssh,

    /// Emoji browser
    Emoji,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

impl FromStr for Layer {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Background" => Ok(Layer::Background),
            "Bottom" => Ok(Layer::Bottom),
            "Top" => Ok(Layer::Top),
            "Overlay" => Ok(Layer::Overlay),
            _ => Err(format!("{s} is not a valid layer.")),
        }
    }
}

#[derive(Debug, Error)]
pub enum ArgsError {
    #[error("input is not valid {0}")]
    InvalidParameter(String),
}

impl FromStr for Anchor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "top" => Ok(Anchor::Top),
            "left" => Ok(Anchor::Left),
            "bottom" => Ok(Anchor::Bottom),
            "right" => Ok(Anchor::Right),
            other => Err(format!("Invalid anchor: {other}")),
        }
    }
}

impl FromStr for Mode {
    type Err = ArgsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "run" => Ok(Mode::Run),
            "drun" => Ok(Mode::Drun),
            "dmenu" => Ok(Mode::Dmenu),
            "file" => Ok(Mode::File),
            "math" => Ok(Mode::Math),
            "ssh" => Ok(Mode::Ssh),
            "emoji" => Ok(Mode::Emoji),
            "auto" => Ok(Mode::Auto),
            _ => Err(ArgsError::InvalidParameter(
                format!("{s} is not a valid argument, see help for details").to_owned(),
            )),
        }
    }
}

impl FromStr for WrapMode {
    type Err = ArgsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(WrapMode::None),
            "word" => Ok(WrapMode::Word),
            "inherit" => Ok(WrapMode::Inherit),
            _ => Err(ArgsError::InvalidParameter(
                format!("{s} is not a valid argument, see help for details").to_owned(),
            )),
        }
    }
}

impl FromStr for SortOrder {
    type Err = ArgsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "alphabetical" => Ok(SortOrder::Alphabetical),
            "default" => Ok(SortOrder::Default),
            _ => Err(ArgsError::InvalidParameter(
                format!("{s} is not a valid argument, see help for details").to_owned(),
            )),
        }
    }
}

impl FromStr for KeyDetectionType {
    type Err = ArgsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "value" => Ok(KeyDetectionType::Value),
            "code" => Ok(KeyDetectionType::Code),
            _ => Err(ArgsError::InvalidParameter(
                format!("{s} is not a valid argument, see help for details").to_owned(),
            )),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Parser)]
#[clap(about = "Worf is a wofi clone written in rust, it aims to be a drop-in replacement")]
#[derive(Default)]
pub struct Config {
    /// Forks the menu so you can close the terminal
    #[clap(short = 'f', long = "fork")]
    fork: Option<bool>,

    /// Selects a config file to use
    #[clap(short = 'c', long = "conf")]
    cfg_path: Option<String>,

    /// Prints the version and then exits
    #[clap(short = 'v', long = "version")]
    #[serde(default = "default_false")]
    version: bool,

    /// Defines the style sheet to be loaded.
    /// Defaults to `$XDG_CONF_DIR/worf/style.css`
    /// or `$HOME/.config/worf/style.css` if `$XDG_CONF_DIR` is not set.
    #[clap(long = "style")]
    style: Option<String>,

    /// Defines the mode worf is running in
    #[clap(long = "show")]
    show: Option<Mode>,

    /// Default width of the window, defaults to 50% of the screen
    #[clap(long = "width")]
    width: Option<String>,

    /// Default height of the window, defaults to 40% of the screen
    #[clap(long = "height")]
    height: Option<String>,

    /// Defines which prompt is used. Default is selected 'show'
    #[clap(short = 'p', long = "prompt")]
    prompt: Option<String>,

    #[clap(short = 'x', long = "xoffset")]
    xoffset: Option<i32>, // todo support this

    #[clap(short = 'y', long = "yoffset")]
    yoffset: Option<i32>, // todo support this

    /// If true a normal window instead of a layer shell will be used
    #[clap(short = 'n', long = "normal-window")]
    #[serde(default = "default_false")]
    normal_window: bool,

    /// Set to 'false' to disable images, defaults to true
    #[clap(short = 'I', long = "allow-images")]
    allow_images: Option<bool>,

    /// If `true` pango markup is parsed
    #[clap(short = 'm', long = "allow-markup")]
    allow_markup: Option<bool>,

    #[clap(short = 'k', long = "cache-file")]
    cache_file: Option<String>, // todo support this

    /// Defines which terminal to use. defaults to the first one found:
    /// * kitty
    /// * gnome-terminal
    /// * konsole
    /// * xfce4-terminal
    /// * lxterminal
    /// * xterm
    /// * alacritty
    /// * terminator
    ///
    /// Must be configured including the needed arguments to launch something
    /// i.e. 'kitty -c'
    #[clap(short = 't', long = "term")]
    term: Option<String>,

    #[clap(short = 'P', long = "password")]
    password: Option<String>,

    #[clap(short = 'e', long = "exec-search")]
    exec_search: Option<bool>, // todo support this

    /// Defines whether the scrollbar is visible
    #[clap(short = 'b', long = "hide-scroll")]
    hide_scroll: Option<bool>,

    /// Defines the matching method, defaults to contains
    #[clap(short = 'M', long = "matching")]
    matching: Option<MatchMethod>,

    /// Control if search is case-insensitive or not.
    /// Defaults to true
    #[clap(short = 'i', long = "insensitive")]
    insensitive: Option<bool>,

    #[clap(short = 'q', long = "parse-search")]
    parse_search: Option<bool>, // todo support this

    /// set where the window is displayed.
    /// can be used to anchor a window to an edge by
    /// setting top,left for example
    #[clap(short = 'l', long = "location", value_delimiter = ',', value_parser = clap::builder::ValueParser::new(Anchor::from_str)
    )]
    location: Option<Vec<Anchor>>,

    #[clap(short = 'a', long = "no-actions")]
    no_actions: Option<bool>,

    /// If set, the given amount tof lines will be shown
    #[clap(short = 'L', long = "lines")]
    lines: Option<i32>,

    /// Additional space to add to the window when `lines` is used.
    #[clap(long = "line-additional-space")]
    lines_additional_space: Option<i32>,

    #[clap(short = 'w', long = "columns")]
    columns: Option<u32>,

    #[clap(short = 'O', long = "sort-order")]
    sort_order: Option<SortOrder>,

    #[clap(short = 'Q', long = "search")]
    search: Option<String>,

    #[clap(short = 'o', long = "monitor")]
    monitor: Option<String>, // todo support this

    #[clap(short = 'r', long = "pre-display-cmd")]
    pre_display_cmd: Option<String>, // todo support this

    #[clap(long = "orientation")]
    orientation: Option<Orientation>,

    /// Horizontal alignment
    #[clap(long = "halign")]
    halign: Option<Align>,

    /// Alignment of content
    #[clap(long = "content-halign")]
    content_halign: Option<Align>,

    /// Vertical alignment
    #[clap(long = "valign")]
    valign: Option<Align>,

    /// Defines the image size in pixels
    #[clap(long = "image-size")]
    image_size: Option<u16>,

    key_up: Option<String>,          // todo support this
    key_down: Option<String>,        // todo support this
    key_left: Option<String>,        // todo support this
    key_right: Option<String>,       // todo support this
    key_forward: Option<String>,     // todo support this
    key_backward: Option<String>,    // todo support this
    key_submit: Option<String>,      // todo support this
    key_exit: Option<String>,        // todo support this
    key_pgup: Option<String>,        // todo support this
    key_pgdn: Option<String>,        // todo support this
    key_expand: Option<String>,      // todo support this
    key_hide_search: Option<String>, // todo support this
    key_copy: Option<String>,        // todo support this

    // todo re-add this
    // #[serde(flatten)]
    // key_custom: Option<HashMap<String, String>>,
    global_coords: Option<bool>, // todo support this

    /// If set to `true` the search field willOption<> be hidden.
    #[clap(long = "hide-search")]
    hide_search: Option<bool>,

    #[clap(long = "dynamic-lines")]
    dynamic_lines: Option<bool>,

    #[clap(long = "layer")]
    layer: Option<Layer>,

    copy_exec: Option<String>, // todo support this
    #[clap(long = "single_click")]
    single_click: Option<bool>, // todo support this
    #[clap(long = "pre-display-exec")]
    pre_display_exec: Option<bool>, // todo support this

    /// Minimum score for a fuzzy search to be shown
    #[clap(long = "fuzzy-min-score")]
    fuzzy_min_score: Option<f64>,

    /// Orientation of items in the row box where items are displayed
    #[clap(long = "row-box-orientation")]
    row_box_orientation: Option<Orientation>,

    /// Defines if lines should wrap.
    /// Can be None, Inherit, Word
    /// Defaults to None
    #[clap(long = "line-wrap")]
    line_wrap: Option<WrapMode>,

    /// Truncate labels after reaching this amount of chars.
    #[clap(long = "line-max-chars")]
    line_max_chars: Option<usize>,

    /// Defines the maximum width of a label in chars.
    /// After reaching this, lines will break into a new line.
    /// Does not truncate.
    #[clap(long = "line-max-width-chars")]
    line_max_width_chars: Option<i32>,

    /// Display only icon in emoji mode
    #[clap(long = "emoji-hide-string")]
    emoji_hide_label: Option<bool>,

    /// Defines the key detection type.
    /// See `KeyDetectionType` for details.
    #[clap(long = "key-detection-type")]
    key_detection_type: Option<KeyDetectionType>,
}

impl Config {
    #[must_use]
    pub fn fork(&self) -> bool {
        self.fork.unwrap_or(false)
    }

    #[must_use]
    pub fn image_size(&self) -> u16 {
        self.image_size.unwrap_or(32)
    }

    #[must_use]
    pub fn match_method(&self) -> MatchMethod {
        self.matching.unwrap_or(MatchMethod::Contains)
    }

    #[must_use]
    pub fn fuzzy_min_score(&self) -> f64 {
        self.fuzzy_min_score.unwrap_or(0.0)
    }

    #[must_use]
    pub fn style(&self) -> Option<String> {
        style_path(self.style.as_ref())
            .ok()
            .map(|pb| pb.display().to_string())
            .or_else(|| {
                log::error!("no stylesheet found, using system styles");
                None
            })
    }

    #[must_use]
    pub fn normal_window(&self) -> bool {
        self.normal_window
    }

    #[must_use]
    pub fn location(&self) -> Option<&Vec<Anchor>> {
        self.location.as_ref()
    }

    #[must_use]
    pub fn hide_scroll(&self) -> bool {
        self.hide_scroll.unwrap_or(false)
    }

    #[must_use]
    pub fn columns(&self) -> u32 {
        self.columns.unwrap_or(1)
    }

    #[must_use]
    pub fn halign(&self) -> Align {
        self.halign.unwrap_or(Align::Fill)
    }

    #[must_use]
    pub fn content_halign(&self) -> Align {
        self.content_halign.unwrap_or(Align::Fill)
    }

    #[must_use]
    pub fn valign(&self) -> Align {
        self.valign.unwrap_or(Align::Center)
    }
    #[must_use]
    pub fn orientation(&self) -> Orientation {
        self.orientation.unwrap_or(Orientation::Vertical)
    }

    #[must_use]
    pub fn prompt(&self) -> String {
        match &self.prompt {
            None => match &self.show {
                None => String::new(),
                Some(mode) => match mode {
                    Mode::Run => "run".to_owned(),
                    Mode::Drun => "drun".to_owned(),
                    Mode::Dmenu => "dmenu".to_owned(),
                    Mode::Math => "math".to_owned(),
                    Mode::File => "file".to_owned(),
                    Mode::Auto => "auto".to_owned(),
                    Mode::Ssh => "ssh".to_owned(),
                    Mode::Emoji => "emoji".to_owned(),
                },
            },

            Some(prompt) => prompt.clone(),
        }
    }

    #[must_use]
    pub fn height(&self) -> String {
        self.height.clone().unwrap_or("40%".to_owned())
    }

    #[must_use]
    pub fn width(&self) -> String {
        self.width.clone().unwrap_or("50%".to_owned())
    }

    #[must_use]
    pub fn row_box_orientation(&self) -> Orientation {
        self.row_box_orientation.unwrap_or(Orientation::Horizontal)
    }

    #[must_use]
    pub fn allow_images(&self) -> bool {
        self.allow_images.unwrap_or(true)
    }

    #[must_use]
    pub fn line_wrap(&self) -> WrapMode {
        self.line_wrap.clone().unwrap_or(WrapMode::None)
    }

    #[must_use]
    pub fn line_max_chars(&self) -> Option<usize> {
        self.line_max_chars
    }

    #[must_use]
    pub fn line_max_width_chars(&self) -> Option<i32> {
        self.line_max_width_chars
    }

    #[must_use]
    pub fn term(&self) -> Option<String> {
        self.term.clone().or_else(|| {
            let terminals = [
                ("gnome-terminal", vec!["--"]),
                ("konsole", vec!["-e"]),
                ("xfce4-terminal", vec!["--command"]),
                ("xterm", vec!["-e"]),
                ("alacritty", vec!["-e"]),
                ("lxterminal", vec!["-e"]),
                ("kitty", vec!["-e"]),
                ("tilix", vec!["-e"]),
            ];

            for (term, launch) in &terminals {
                if which::which(term).is_ok() {
                    return Some(format!("{} {}", term, launch.join(" ")));
                }
            }

            None
        })
    }

    #[must_use]
    pub fn show(&self) -> Option<Mode> {
        self.show.clone()
    }

    #[must_use]
    pub fn insensitive(&self) -> bool {
        self.insensitive.unwrap_or(true)
    }

    #[must_use]
    pub fn hide_search(&self) -> bool {
        self.hide_search.unwrap_or(false)
    }

    #[must_use]
    pub fn search(&self) -> Option<String> {
        self.search.clone()
    }

    #[must_use]
    pub fn allow_markup(&self) -> bool {
        self.allow_markup.unwrap_or(false)
    }

    #[must_use]
    pub fn password(&self) -> Option<String> {
        self.password.clone()
    }

    #[must_use]
    pub fn no_actions(&self) -> bool {
        self.no_actions.unwrap_or(false)
    }

    #[must_use]
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.clone().unwrap_or(SortOrder::Alphabetical)
    }

    #[must_use]
    pub fn emoji_hide_label(&self) -> bool {
        self.emoji_hide_label.unwrap_or(false)
    }

    #[must_use]
    pub fn key_detection_type(&self) -> KeyDetectionType {
        self.key_detection_type
            .clone()
            .unwrap_or(KeyDetectionType::Value)
    }

    #[must_use]
    pub fn lines(&self) -> Option<i32> {
        self.lines
    }

    #[must_use]
    pub fn lines_additional_space(&self) -> i32 {
        self.lines_additional_space.unwrap_or(0)
    }

    #[must_use]
    pub fn version(&self) -> bool {
        self.version
    }

    #[must_use]
    pub fn layer(&self) -> Layer {
        self.layer.clone().unwrap_or(Layer::Top)
    }

    #[must_use]
    pub fn dynamic_lines(&self) -> bool {
        self.dynamic_lines.unwrap_or(false)
    }
}

fn default_false() -> bool {
    false
}

// fn default_true() -> bool {
//     true
// }

#[must_use]
pub fn parse_args() -> Config {
    Config::parse()
}

/// # Errors
///
/// Will return Err when it cannot resolve any path or no style is found
fn style_path(full_path: Option<&String>) -> Result<PathBuf, Error> {
    let alternative_paths = path_alternatives(
        vec![dirs::config_dir()],
        &PathBuf::from("worf").join("style.css"),
    );
    resolve_path(full_path, alternative_paths.into_iter().collect())
}

/// # Errors
///
/// Will return Err when it cannot resolve any path or no style is found
pub fn conf_path(full_path: Option<&String>) -> Result<PathBuf, Error> {
    let alternative_paths = path_alternatives(
        vec![dirs::config_dir()],
        &PathBuf::from("worf").join("config"),
    );
    resolve_path(full_path, alternative_paths.into_iter().collect())
}

#[must_use]
pub fn path_alternatives(base_paths: Vec<Option<PathBuf>>, sub_path: &PathBuf) -> Vec<PathBuf> {
    base_paths
        .into_iter()
        .flatten()
        .map(|pb| pb.join(sub_path))
        .filter_map(|pb| pb.canonicalize().ok())
        .filter(|c| c.exists())
        .collect()
}

/// # Errors
///
/// Will return `Err` if it is not able to find any valid path
pub fn resolve_path(
    full_path: Option<&String>,
    alternatives: Vec<PathBuf>,
) -> Result<PathBuf, Error> {
    log::debug!("resolving path for {full_path:?}, with alternatives: {alternatives:?}");
    full_path
        .map(PathBuf::from)
        .and_then(|p| p.canonicalize().ok().filter(|c| c.exists()))
        .or_else(|| {
            alternatives
                .into_iter()
                .filter(|p| p.exists())
                .find_map(|pb| pb.canonicalize().ok().filter(|c| c.exists()))
        })
        .ok_or(Error::MissingFile)
}

/// # Errors
///
/// Will return Err when it
/// * cannot read the config file
/// * cannot parse the config file
/// * no config file exists
/// * config file and args cannot be merged
pub fn load_config(args_opt: Option<&Config>) -> Result<Config, Error> {
    let config_path = conf_path(args_opt.as_ref().and_then(|c| c.cfg_path.as_ref()));
    match config_path {
        Ok(path) => {
            log::debug!("loading config from {}", path.display());
            let toml_content = fs::read_to_string(path).map_err(|e| Error::Io(format!("{e}")))?;
            let mut config: Config =
                toml::from_str(&toml_content).map_err(|e| Error::ParsingError(format!("{e}")))?;

            if let Some(args) = args_opt {
                let merge_result = merge_config_with_args(&mut config, args)
                    .map_err(|e| Error::ParsingError(format!("{e}")))?;
                Ok(merge_result)
            } else {
                Ok(config)
            }
        }

        Err(e) => Err(Error::Io(format!("{e}"))),
    }
}

#[must_use]
pub fn expand_path(input: &str) -> PathBuf {
    let mut path = input.to_string();

    // Expand ~ to home directory
    if path.starts_with('~') {
        if let Some(home_dir) = dirs::home_dir() {
            path = path.replacen('~', home_dir.to_str().unwrap_or(""), 1);
        }
    }

    // Expand $VAR style environment variables
    if path.contains('$') {
        for (key, value) in env::vars() {
            let var_pattern = format!("${key}");
            if path.contains(&var_pattern) {
                path = path.replace(&var_pattern, &value);
            }
        }
    }

    PathBuf::from(path)
}

/// # Errors
///
/// Will return Err when it fails to merge the config with the arguments.
pub fn merge_config_with_args(config: &mut Config, args: &Config) -> anyhow::Result<Config> {
    let args_json = serde_json::to_value(args)?;
    let mut config_json = serde_json::to_value(config)?;

    merge_json(&mut config_json, &args_json);
    Ok(serde_json::from_value(config_json).unwrap_or_default())
}

fn merge_json(a: &mut Value, b: &Value) {
    match (a, b) {
        (Value::Object(a_map), Value::Object(b_map)) => {
            for (k, v) in b_map {
                merge_json(a_map.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (a_val, b_val) => {
            if *b_val != Value::Null {
                *a_val = b_val.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_keyboard_type() {
        let toml_str = r#"
        key_detection_type="Code"
    "#;

        let config: Config = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.key_detection_type(), KeyDetectionType::Code);
    }
}

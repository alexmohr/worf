use std::path::PathBuf;
use std::str::FromStr;
use std::{env, fmt, fs};

use anyhow::anyhow;
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug)]
pub enum ConfigurationError {
    Open(String),
    Parse(String),
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigurationError::Open(e) | ConfigurationError::Parse(e) => write!(f, "{e}"),
        }
    }
}

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

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum Animation {
    None,
    Expand,
    ExpandVertical,
    ExpandHorizontal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WrapMode {
    None,
    Word,
    Inherit,
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

#[derive(Debug, Deserialize, Serialize, Clone, Parser)]
#[clap(about = "Worf is a wofi clone written in rust, it aims to be a drop-in replacement")]
pub struct Config {
    /// Forks the menu so you can close the terminal
    #[clap(short = 'f', long = "fork")]
    pub fork: Option<bool>,

    /// Selects a config file to use
    #[clap(short = 'c', long = "conf")]
    pub config: Option<String>,

    /// Prints the version and then exits
    #[clap(short = 'v', long = "version")]
    pub version: Option<bool>,

    /// Defines the style sheet to be loaded.
    /// Defaults to `$XDG_CONF_DIR/worf/style.css`
    /// or `$HOME/.config/worf/style.css` if `$XDG_CONF_DIR` is not set.
    #[serde(default = "default_style")]
    #[clap(long = "style")]
    pub style: Option<String>,

    /// Defines the mode worf is running in
    #[clap(long = "show")]
    pub show: Option<Mode>,

    /// Default width of the window, defaults to 50% of the screen
    #[serde(default = "default_width")]
    #[clap(long = "width")]
    pub width: Option<String>,

    /// Default height of the window, defaults to 40% of the screen
    #[serde(default = "default_height")]
    #[clap(long = "height")]
    pub height: Option<String>,

    /// Defines which prompt is used. Default is selected 'show'
    #[clap(short = 'p', long = "prompt")]
    pub prompt: Option<String>,

    #[clap(short = 'x', long = "xoffset")]
    pub xoffset: Option<i32>,

    #[clap(short = 'y', long = "yoffset")]
    pub yoffset: Option<i32>,

    /// If true a normal window instead of a layer shell will be used
    #[serde(default = "default_normal_window")]
    #[clap(short = 'n', long = "normal-window")]
    pub normal_window: bool,

    #[clap(short = 'I', long = "allow-images")]
    pub allow_images: Option<bool>,

    #[clap(short = 'm', long = "allow-markup")]
    pub allow_markup: Option<bool>,

    #[clap(short = 'k', long = "cache-file")]
    pub cache_file: Option<String>,

    #[clap(short = 't', long = "term")]
    pub term: Option<String>,

    #[serde(default = "default_password_char")]
    #[clap(short = 'P', long = "password")]
    pub password: Option<String>,

    #[clap(short = 'e', long = "exec-search")]
    pub exec_search: Option<bool>,

    #[clap(short = 'b', long = "hide-scroll")]
    pub hide_scroll: Option<bool>,

    #[serde(default = "default_match_method")]
    #[clap(short = 'M', long = "matching")]
    pub matching: Option<MatchMethod>,

    #[clap(short = 'i', long = "insensitive")]
    pub insensitive: Option<bool>,

    #[clap(short = 'q', long = "parse-search")]
    pub parse_search: Option<bool>,

    /// set where the window is displayed.
    /// can be used to anchor a window to an edge by
    /// setting top,left for example
    #[clap(short = 'l', long = "location", value_delimiter = ',', value_parser = clap::builder::ValueParser::new(Anchor::from_str))]
    pub location: Option<Vec<Anchor>>,

    #[clap(short = 'a', long = "no-actions")]
    pub no_actions: Option<bool>,

    #[clap(short = 'L', long = "lines")]
    pub lines: Option<u32>,

    #[serde(default = "default_columns")]
    #[clap(short = 'w', long = "columns")]
    pub columns: Option<u32>,

    #[clap(short = 'O', long = "sort-order")]
    pub sort_order: Option<String>,

    #[clap(short = 'G', long = "gtk-dark")]
    pub gtk_dark: Option<bool>,

    #[clap(short = 'Q', long = "search")]
    pub search: Option<String>,

    #[clap(short = 'o', long = "monitor")]
    pub monitor: Option<String>,

    #[clap(short = 'r', long = "pre-display-cmd")]
    pub pre_display_cmd: Option<String>,

    #[serde(default = "default_orientation")]
    #[clap(long = "orientation")]
    pub orientation: Option<Orientation>,

    /// Horizontal alignment
    #[serde(default = "default_halign")]
    #[clap(long = "halign")]
    pub halign: Option<Align>,

    /// Alignment of content
    #[serde(default = "default_content_halign")]
    #[clap(long = "content-halign")]
    pub content_halign: Option<Align>,

    /// Vertical alignment
    #[clap(long = "valign")]
    pub valign: Option<Align>,

    pub filter_rate: Option<u32>,

    /// Defines the image size in pixels
    #[serde(default = "default_image_size")]
    #[clap(long = "image-size")]
    pub image_size: Option<i32>,

    pub key_up: Option<String>,
    pub key_down: Option<String>,
    pub key_left: Option<String>,
    pub key_right: Option<String>,
    pub key_forward: Option<String>,
    pub key_backward: Option<String>,
    pub key_submit: Option<String>,
    pub key_exit: Option<String>,
    pub key_pgup: Option<String>,
    pub key_pgdn: Option<String>,
    pub key_expand: Option<String>,
    pub key_hide_search: Option<String>,
    pub key_copy: Option<String>,

    // todo re-add this
    // #[serde(flatten)]
    // pub key_custom: Option<HashMap<String, String>>,
    pub global_coords: Option<bool>,
    pub hide_search: Option<bool>,
    pub dynamic_lines: Option<bool>,
    pub layer: Option<String>,
    pub copy_exec: Option<String>,
    pub single_click: Option<bool>,
    pub pre_display_exec: Option<bool>,

    /// Minimum score for a fuzzy search to be shown
    #[serde(default = "default_fuzzy_min_score")]
    #[clap(long = "fuzzy-min-score")]
    pub fuzzy_min_score: Option<f64>,

    /// Orientation of items in the row box where items are displayed
    #[serde(default = "default_row_box_orientation")]
    #[clap(long = "row-box-orientation")]
    pub row_bow_orientation: Option<Orientation>,

    // /// Set to to true to wrap text after a given amount of chars
    // #[serde(default = "default_text_wrap")]
    // #[clap(long = "text-wrap")]
    // pub text_wrap: Option<bool>,
    //
    // /// Defines after how many chars a line is broken over.
    // /// Only cuts at spaces.
    // #[serde(default = "default_text_wrap_length")]
    // #[clap(long = "text-wrap-length")]
    // pub text_wrap_length: Option<usize>,
    /// Defines the animation when the window is show.
    /// Defaults to Expand
    #[serde(default = "default_show_animation")]
    #[clap(long = "show-animation")]
    pub show_animation: Option<Animation>,

    /// Defines how long it takes for the show animation to finish
    /// Defaults to 70ms
    #[serde(default = "default_show_animation_time")]
    #[clap(long = "show-animation-time")]
    pub show_animation_time: Option<u64>,

    /// Defines the animation when the window is hidden.
    /// Defaults to None, because it is a bit buggy with
    /// gtk layer shell. works fine with normal window though
    #[serde(default = "default_hide_animation")]
    #[clap(long = "hide-animation")]
    pub hide_animation: Option<Animation>,

    /// Defines how long it takes for the hide animation to finish
    /// Defaults to 100ms
    #[serde(default = "default_hide_animation_time")]
    #[clap(long = "hide-animation-time")]
    pub hide_animation_time: Option<u64>,

    #[serde(default = "default_line_wrap")]
    #[clap(long = "line-wrap")]
    pub line_wrap: Option<WrapMode>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fork: None,
            config: None,
            version: None,
            style: default_style(),
            show: None,
            width: default_width(),
            height: default_height(),
            prompt: None,
            xoffset: None,
            yoffset: None,
            normal_window: default_normal_window(),
            allow_images: None,
            allow_markup: None,
            cache_file: None,
            term: None,
            password: None,
            exec_search: None,
            hide_scroll: None,
            matching: None,
            insensitive: None,
            parse_search: None,
            location: None,
            no_actions: None,
            lines: None,
            columns: default_columns(),
            sort_order: None,
            gtk_dark: None,
            search: None,
            monitor: None,
            pre_display_cmd: None,
            orientation: default_row_box_orientation(),
            halign: default_halign(),
            content_halign: default_content_halign(),
            valign: None,
            filter_rate: None,
            image_size: default_image_size(),
            key_up: None,
            key_down: None,
            key_left: None,
            key_right: None,
            key_forward: None,
            key_backward: None,
            key_submit: None,
            key_exit: None,
            key_pgup: None,
            key_pgdn: None,
            key_expand: None,
            key_hide_search: None,
            key_copy: None,
            //key_custom: None,
            line_wrap: default_line_wrap(),
            global_coords: None,
            hide_search: None,
            dynamic_lines: None,
            layer: None,
            copy_exec: None,
            single_click: None,
            pre_display_exec: None,
            fuzzy_min_score: default_fuzzy_min_score(),
            row_bow_orientation: default_row_box_orientation(),
            show_animation: default_show_animation(),
            show_animation_time: default_show_animation_time(),
            hide_animation: default_hide_animation(),
            hide_animation_time: default_hide_animation_time(),
        }
    }
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_show_animation_time() -> Option<u64> {
    Some(30)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_show_animation() -> Option<Animation> {
    Some(Animation::Expand)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_hide_animation_time() -> Option<u64> {
    Some(100)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_hide_animation() -> Option<Animation> {
    Some(Animation::None)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_row_box_orientation() -> Option<Orientation> {
    Some(Orientation::Horizontal)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_orientation() -> Option<Orientation> {
    Some(Orientation::Vertical)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_halign() -> Option<Align> {
    Some(Align::Fill)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_content_halign() -> Option<Align> {
    Some(Align::Fill)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_columns() -> Option<u32> {
    Some(1)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_normal_window() -> bool {
    false
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_line_wrap() -> Option<WrapMode> {
    Some(WrapMode::Word)
}

// TODO
// GtkOrientation orientation = config_get_mnemonic(config, "orientation", "vertical", 2, "vertical", "horizontal");
// outer_orientation = config_get_mnemonic(cstoonfig, "orientation", "vertical", 2, "horizontal", "vertical");
// GtkAlign halign = config_get_mnemonic(config, "halign", "fill", 4, "fill", "start", "end", "center");
// content_halign = config_get_mnemonic(config, "content_halign", "fill", 4, "fill", "start", "end", "center");
// char* default_valign = "start";
// if(outer_orientation == GTK_ORIENTATION_HORIZONTAL) {
// default_valign = "center";
// }
// GtkAlign valign = config_get_mnemonic(config, "valign", default_valign, 4, "fill", "start", "end", "center");
// char* prompt = config_get(config, "prompt", mode);
// uint64_t filter_rate = strtol(config_get(config, "filter_rate", "100"), NULL, 10);
// allow_images = strcmp(config_get(config, "allow_images", "false"), "true") == 0;
// allow_markup = strcmp(config_get(config, "allow_markup", "false"), "true") == 0;
// image_size = strtol(config_get(config, "image_size", "32"), NULL, 10);
// cache_file = map_get(config, "cache_file");
// config_dir = map_get(config, "config_dir");
// terminal = map_get(config, "term");
// exec_search = strcmp(config_get(config, "exec_search", "false"), "true") == 0;
// bool hide_scroll = strcmp(config_get(config, "hide_scroll", "false"), "true") == 0;
// matching = config_get_mnemonic(config, "matching", "contains", 3, "contains", "multi-contains", "fuzzy");
// insensitive = strcmp(config_get(config, "insensitive", "false"), "true") == 0;
// parse_search = strcmp(config_get(config, "parse_search", "false"), "true") == 0;
// location = config_get_mnemonic(config, "location", "center", 18,
// "center", "top_left", "top", "top_right", "right", "bottom_right", "bottom", "bottom_left", "left",
// "0", "1", "2", "3", "4", "5", "6", "7", "8");
// no_actions = strcmp(config_get(config, "no_actions", "false"), "true") == 0;
// lines = strtol(config_get(config, "lines", "0"), NULL, 10);
// max_lines = lines;
// columns = strtol(config_get(config, "columns", "1"), NULL, 10);
// sort_order = config_get_mnemonic(config, "sort_order", "default", 2, "default", "alphabetical");
// bool global_coords = strcmp(config_get(config, "global_coords", "false"), "true") == 0;
// hide_search = strcmp(config_get(config, "hide_search", "false"), "true") == 0;
// char* search = map_get(config, "search");
// dynamic_lines = strcmp(config_get(config, "dynamic_lines", "false"), "true") == 0;
// char* monitor = map_get(config, "monitor");
// char* layer = config_get(config, "layer", "top");
// copy_exec = config_get(config, "copy_exec", "wl-copy");
// pre_display_cmd = map_get(config, "pre_display_cmd");
// pre_display_exec = strcmp(config_get(config, "pre_display_exec", "false"), "true") == 0;
// single_click = strcmp(config_get(config, "single_click", "false"), "true") == 0;
//
// keys = map_init_void();
// mods = map_init_void();
//
// map_put_void(mods, "Shift", &shift_mask);
// map_put_void(mods, "Ctrl", &ctrl_mask);
// map_put_void(mods, "Alt", &alt_mask);
//
// key_default = "Up";
// char* key_up = (i == 0) ? "Up" : config_get(config, "key_up", key_default);
// key_default = "Down";
// char* key_down = (i == 0) ? key_default : config_get(config, "key_down", key_default);
// key_default = "Left";
// char* key_left = (i == 0) ? key_default : config_get(config, "key_left", key_default);
// key_default = "Right";
// char* key_right = (i == 0) ? key_default : config_get(config, "key_right", key_default);
// key_default = "Tab";
// char* key_forward = (i == 0) ? key_default : config_get(config, "key_forward", key_default);
// key_default = "Shift-ISO_Left_Tab";
// char* key_backward = (i == 0) ? key_default : config_get(config, "key_backward", key_default);
// key_default = "Return";
// char* key_submit = (i == 0) ? key_default : config_get(config, "key_submit", key_default);
// key_default = "Escape";
// char* key_exit = (i == 0) ? key_default : config_get(config, "key_exit", key_default);
// key_default = "Page_Up";
// char* key_pgup = (i == 0) ? key_default : config_get(config, "key_pgup", key_default);
// key_default = "Page_Down";
// char* key_pgdn = (i == 0) ? key_default : config_get(config, "key_pgdn", key_default);
// key_default = "";
// char* key_expand = (i == 0) ? key_default: config_get(config, "key_expand", key_default);
// key_default = "";
// char* key_hide_search = (i == 0) ? key_default: config_get(config, "key_hide_search", key_default);
// key_default = "Ctrl-c";
// char* key_copy = (i == 0) ? key_default : config_get(config, "key_copy", key_default);

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_style() -> Option<String> {
    style_path(None)
        .ok()
        .map(|pb| pb.display().to_string())
        .or_else(|| {
            log::error!("no stylesheet found, using system styles");
            None
        })
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_height() -> Option<String> {
    Some("40%".to_owned())
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_width() -> Option<String> {
    Some("50%".to_owned())
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_password_char() -> Option<String> {
    Some("*".to_owned())
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_fuzzy_min_length() -> Option<i32> {
    Some(10)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_fuzzy_min_score() -> Option<f64> {
    Some(0.0)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_match_method() -> Option<MatchMethod> {
    Some(MatchMethod::Contains)
}

// allowed because option is needed for serde macro
#[allow(clippy::unnecessary_wraps)]
#[must_use]
pub fn default_image_size() -> Option<i32> {
    Some(32)
}

#[must_use]
pub fn parse_args() -> Config {
    Config::parse()
}

/// # Errors
///
/// Will return Err when it cannot resolve any path or no style is found
pub fn style_path(full_path: Option<String>) -> Result<PathBuf, anyhow::Error> {
    let alternative_paths = path_alternatives(
        vec![dirs::config_dir()],
        &PathBuf::from("worf").join("style.css"),
    );
    resolve_path(full_path, alternative_paths.into_iter().collect())
}

/// # Errors
///
/// Will return Err when it cannot resolve any path or no style is found
pub fn conf_path(full_path: Option<String>) -> Result<PathBuf, anyhow::Error> {
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
    full_path: Option<String>,
    alternatives: Vec<PathBuf>,
) -> Result<PathBuf, anyhow::Error> {
    full_path
        .map(PathBuf::from)
        .and_then(|p| p.canonicalize().ok().filter(|c| c.exists()))
        .or_else(|| {
            alternatives
                .into_iter()
                .filter(|p| p.exists())
                .find_map(|pb| pb.canonicalize().ok().filter(|c| c.exists()))
        })
        .ok_or_else(|| anyhow!("Could not find a valid file."))
}

/// # Errors
///
/// Will return Err when it
/// * cannot read the config file
/// * cannot parse the config file
/// * no config file exists
/// * config file and args cannot be merged
pub fn load_config(args_opt: Option<Config>) -> Result<Config, ConfigurationError> {
    let config_path = conf_path(args_opt.as_ref().and_then(|c| c.config.clone()));
    match config_path {
        Ok(path) => {
            let toml_content =
                fs::read_to_string(path).map_err(|e| ConfigurationError::Open(format!("{e}")))?;
            let mut config: Config = toml::from_str(&toml_content)
                .map_err(|e| ConfigurationError::Parse(format!("{e}")))?;

            if let Some(args) = args_opt {
                let mut merge_result = merge_config_with_args(&mut config, &args)
                    .map_err(|e| ConfigurationError::Parse(format!("{e}")))?;

                if merge_result.prompt.is_none() {
                    match &merge_result.show {
                        None => {}
                        Some(mode) => match mode {
                            Mode::Run => merge_result.prompt = Some("run".to_owned()),
                            Mode::Drun => merge_result.prompt = Some("drun".to_owned()),
                            Mode::Dmenu => merge_result.prompt = Some("dmenu".to_owned()),
                            Mode::Math => merge_result.prompt = Some("math".to_owned()),
                            Mode::File => merge_result.prompt = Some("file".to_owned()),
                            Mode::Auto => merge_result.prompt = Some("auto".to_owned()),
                        },
                    }
                }

                Ok(merge_result)
            } else {
                Ok(config)
            }
        }

        Err(e) => Err(ConfigurationError::Open(format!("{e}"))),
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

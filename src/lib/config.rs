use crate::args::Args;
use crate::lib::system;
use anyhow::anyhow;
use clap::ValueEnum;
use gtk4::prelude::ToValue;
use merge::Merge;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::path::PathBuf;

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

#[derive(Debug, Deserialize, Serialize, Merge, Clone)]
pub struct Config {
    /// Defines the path to the stylesheet being used.
    /// Defaults to XDG_CONFIG_DIR/worf/style.css
    /// If XDG_CONFIG_DIR is not defined $HOME/.config will be used instead
    #[serde(default = "default_style")]
    pub style: Option<String>,
    pub show: Option<String>,
    pub mode: Option<String>,
    #[serde(default = "default_width")]
    pub width: Option<String>,
    #[serde(default = "default_height")]
    pub height: Option<String>,
    pub prompt: Option<String>,
    pub xoffset: Option<i32>,
    pub x: Option<i32>,
    pub yoffset: Option<i32>,
    pub y: Option<i32>,
    #[serde(default = "default_normal_window")]
    pub normal_window: Option<bool>,
    pub allow_images: Option<bool>,
    pub allow_markup: Option<bool>,
    pub cache_file: Option<String>,
    pub term: Option<String>,
    #[serde(default = "default_password_char")]
    pub password: Option<String>,
    pub exec_search: Option<bool>,
    pub hide_scroll: Option<bool>,

    /// Defines how matching is done
    #[serde(default = "default_match_method")]
    pub matching: Option<MatchMethod>,
    pub insensitive: Option<bool>,
    pub parse_search: Option<bool>,
    pub location: Option<String>,
    pub no_actions: Option<bool>,
    pub lines: Option<u32>,
    /// Defines how many columns are shown per row
    #[serde(default = "default_columns")]
    pub columns: Option<u32>,
    pub sort_order: Option<String>,
    pub gtk_dark: Option<bool>,
    pub search: Option<String>,
    pub monitor: Option<String>,
    pub pre_display_cmd: Option<String>,
    /// Defines how the entries root container are ordered
    /// Default is vertical
    #[serde(default = "default_orientation")]
    pub orientation: Option<Orientation>,
    /// Specifies the horizontal align for the entire scrolled area,
    /// it can be any of fill, start, end, or center, default is fill.
    #[serde(default = "default_halign")]
    pub halign: Option<Align>,
    //// Specifies the horizontal align for the individual entries,
    // it can be any of fill, start, end, or center, default is fill.
    #[serde(default = "default_content_halign")]
    pub content_halign: Option<Align>,

    /// Specifies the vertical align for the entire scrolled area, it can be any of fill, start, e
    /// nd, or center, the default is orientation dependent. If vertical then  it  defaults  to
    /// start, if horizontal it defaults to center.
    pub valign: Option<Align>,

    pub filter_rate: Option<u32>,
    /// Specifies the image size when enabled.
    /// Defaults to 32.
    #[serde(default = "default_image_size")]
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
    #[serde(flatten)]
    pub custom_keys: Option<std::collections::HashMap<String, String>>,
    pub line_wrap: Option<String>,
    pub global_coords: Option<bool>,
    pub hide_search: Option<bool>,
    pub dynamic_lines: Option<bool>,
    pub layer: Option<String>,
    pub copy_exec: Option<String>,
    pub single_click: Option<bool>,
    pub pre_display_exec: Option<bool>,

    // Exclusive options
    /// Minimum score for the fuzzy finder to accept a match.
    /// Must be a value between 0 and 1
    /// Defaults to 0.1.
    #[serde(default = "default_fuzzy_min_score")]
    pub fuzzy_min_score: Option<f64>,

    /// Defines how the content in the row box is aligned
    /// Defaults to vertical
    #[serde(default = "default_row_box_orientation")]
    pub row_bow_orientation: Option<Orientation>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            style: default_style(),
            show: None,
            mode: None,
            width: default_width(),
            height: default_height(),
            prompt: None,
            xoffset: None,
            x: None,
            yoffset: None,
            y: None,
            normal_window: None,
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
            custom_keys: None,
            line_wrap: None,
            global_coords: None,
            hide_search: None,
            dynamic_lines: None,
            layer: None,
            copy_exec: None,
            single_click: None,
            pre_display_exec: None,
            fuzzy_min_score: default_fuzzy_min_score(),
            row_bow_orientation: default_row_box_orientation(),
        }
    }
}

fn default_row_box_orientation() -> Option<Orientation> {
    Some(Orientation::Horizontal)
}

fn default_orientation() -> Option<Orientation> {
    Some(Orientation::Vertical)
}

fn default_halign() -> Option<Align> {
    Some(Align::Fill)
}

fn default_content_halign() -> Option<Align> {
    Some(Align::Fill)
}

fn default_columns() -> Option<u32> {
    Some(1)
}

fn default_normal_window() -> Option<bool> {
    Some(false)
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
// line_wrap = config_get_mnemonic(config, "line_wrap", "off", 4, "off", "word", "char", "word_char") - 1;
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

fn default_style() -> Option<String> {
    system::config_path(None)
        .ok()
        .and_then(|pb| Some(pb.display().to_string()))
        .or_else(|| {
            log::error!("no stylesheet found, using system styles");
            None
        })
}

pub fn default_height() -> Option<String> {
    Some("40%".to_owned())
}

pub fn default_width() -> Option<String> {
    Some("50%".to_owned())
}

pub fn default_password_char() -> Option<String> {
    Some("*".to_owned())
}

pub fn default_fuzzy_min_length() -> Option<i32> {
    Some(10)
}

pub fn default_fuzzy_min_score() -> Option<f64> {
    Some(0.1)
}

pub fn default_match_method() -> Option<MatchMethod> {
    Some(MatchMethod::Contains)
}

pub fn default_image_size() -> Option<i32> {
    Some(32)
}

pub fn merge_config_with_args(config: &mut Config, args: &Args) -> anyhow::Result<Config> {
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

use std::{env, fs, path::PathBuf, str::FromStr};

use clap::{Parser, ValueEnum};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    None,
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

impl FromStr for WrapMode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(WrapMode::None),
            "word" => Ok(WrapMode::Word),
            "inherit" => Ok(WrapMode::Inherit),
            _ => Err(Error::InvalidArgument(
                format!("{s} is not a valid argument, see help for details").to_owned(),
            )),
        }
    }
}

impl FromStr for SortOrder {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "alphabetical" => Ok(SortOrder::Alphabetical),
            "default" => Ok(SortOrder::Default),
            _ => Err(Error::InvalidArgument(
                format!("{s} is not a valid argument, see help for details").to_owned(),
            )),
        }
    }
}

impl FromStr for KeyDetectionType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "value" => Ok(KeyDetectionType::Value),
            "code" => Ok(KeyDetectionType::Code),
            _ => Err(Error::InvalidArgument(
                format!("{s} is not a valid argument, see help for details").to_owned(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub enum Key {
    None,

    // Letters
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // Numbers
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    // Function Keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Navigation / Editing
    Escape,
    Enter,
    Space,
    Tab,
    Backspace,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,

    // Special characters
    Exclamation,  // !
    At,           // @
    Hash,         // #
    Dollar,       // $
    Percent,      // %
    Caret,        // ^
    Ampersand,    // &
    Asterisk,     // *
    LeftParen,    // (
    RightParen,   // )
    Minus,        // -
    Underscore,   // _
    Equal,        // =
    Plus,         // +
    LeftBracket,  // [
    RightBracket, // ]
    LeftBrace,    // {
    RightBrace,   // }
    Backslash,    // \
    Pipe,         // |
    Semicolon,    // ;
    Colon,        // :
    Apostrophe,   // '
    Quote,        // "
    Comma,        // ,
    Period,       // .
    Slash,        // /
    Question,     // ?
    Grave,        // `
    Tilde,        // ~
}

impl FromStr for Key {
    type Err = Error;

    #[allow(clippy::too_many_lines)] // won't fix, need all of them
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = match s {
            // Letters
            "A" | "a" => Key::A,
            "B" | "b" => Key::B,
            "C" | "c" => Key::C,
            "D" | "d" => Key::D,
            "E" | "e" => Key::E,
            "F" | "f" => Key::F,
            "G" | "g" => Key::G,
            "H" | "h" => Key::H,
            "I" | "i" => Key::I,
            "J" | "j" => Key::J,
            "K" | "k" => Key::K,
            "L" | "l" => Key::L,
            "M" | "m" => Key::M,
            "N" | "n" => Key::N,
            "O" | "o" => Key::O,
            "P" | "p" => Key::P,
            "Q" | "q" => Key::Q,
            "R" | "r" => Key::R,
            "S" | "s" => Key::S,
            "T" | "t" => Key::T,
            "U" | "u" => Key::U,
            "V" | "v" => Key::V,
            "W" | "w" => Key::W,
            "X" | "x" => Key::X,
            "Y" | "y" => Key::Y,
            "Z" | "z" => Key::Z,

            // Numbers
            "0" => Key::Num0,
            "1" => Key::Num1,
            "2" => Key::Num2,
            "3" => Key::Num3,
            "4" => Key::Num4,
            "5" => Key::Num5,
            "6" => Key::Num6,
            "7" => Key::Num7,
            "8" => Key::Num8,
            "9" => Key::Num9,

            // Function keys
            "F1" => Key::F1,
            "F2" => Key::F2,
            "F3" => Key::F3,
            "F4" => Key::F4,
            "F5" => Key::F5,
            "F6" => Key::F6,
            "F7" => Key::F7,
            "F8" => Key::F8,
            "F9" => Key::F9,
            "F10" => Key::F10,
            "F11" => Key::F11,
            "F12" => Key::F12,

            // Navigation / Editing
            "Escape" => Key::Escape,
            "Enter" => Key::Enter,
            "Space" => Key::Space,
            "Tab" => Key::Tab,
            "Backspace" => Key::Backspace,
            "Insert" => Key::Insert,
            "Delete" => Key::Delete,
            "Home" => Key::Home,
            "End" => Key::End,
            "PageUp" => Key::PageUp,
            "PageDown" => Key::PageDown,
            "Left" => Key::Left,
            "Right" => Key::Right,
            "Up" => Key::Up,
            "Down" => Key::Down,

            // Special characters
            "!" => Key::Exclamation,
            "@" => Key::At,
            "#" => Key::Hash,
            "$" => Key::Dollar,
            "%" => Key::Percent,
            "^" => Key::Caret,
            "&" => Key::Ampersand,
            "*" => Key::Asterisk,
            "(" => Key::LeftParen,
            ")" => Key::RightParen,
            "-" => Key::Minus,
            "_" => Key::Underscore,
            "=" => Key::Equal,
            "+" => Key::Plus,
            "[" => Key::LeftBracket,
            "]" => Key::RightBracket,
            "{" => Key::LeftBrace,
            "}" => Key::RightBrace,
            "\\" => Key::Backslash,
            "|" => Key::Pipe,
            ";" => Key::Semicolon,
            ":" => Key::Colon,
            "'" => Key::Apostrophe,
            "\"" => Key::Quote,
            "," => Key::Comma,
            "." => Key::Period,
            "/" => Key::Slash,
            "?" => Key::Question,
            "`" => Key::Grave,
            "~" => Key::Tilde,
            _ => Key::None,
        };

        if key == Key::None {
            Err(Error::InvalidArgument(format!("{s} is not a valid key")))
        } else {
            Ok(key)
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Parser)]
#[clap(
    about = "Worf is a wofi like launcher, written in rust, it aims to be a drop-in replacement"
)]
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

    /// Default width of the window, defaults to 50% of the screen
    #[clap(long = "width")]
    width: Option<String>,

    /// Default height of the window, defaults to 40% of the screen
    #[clap(long = "height")]
    height: Option<String>,

    /// Defines which prompt is used. Default is selected 'show'
    #[clap(short = 'p', long = "prompt")]
    prompt: Option<String>,

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

    /// If set to a value a custom cache file will be used
    /// instead the default one associated with the selected mode.
    /// May also be for usage in the api
    #[clap(short = 'k', long = "cache-file")]
    cache_file: Option<String>,

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
    /// i.e. '--term "kitty -c"'
    #[clap(short = 't', long = "term")]
    term: Option<String>,

    #[clap(short = 'P', long = "password")]
    password: Option<String>,

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

    /// If set to `true` sub actions will be disabled
    #[clap(short = 'a', long = "no-actions")]
    no_actions: Option<bool>,

    /// If set, the given amount tof lines will be shown
    #[clap(short = 'L', long = "lines")]
    lines: Option<i32>,

    /// Additional space to add to the window when `lines` is used.
    #[clap(long = "line-additional-space")]
    lines_additional_space: Option<i32>,

    /// factor to multiple the line height with.
    #[clap(long = "lines-size-factor")]
    lines_size_factor: Option<f64>,

    /// How many columns to display at most in the window
    /// Shows less and wraps if not enough space is available
    #[clap(short = 'w', long = "columns")]
    columns: Option<u32>,

    /// Defines how elements are sorted
    /// Options:
    /// * Alphabetical
    /// * Default (no sort applied)
    #[clap(short = 'O', long = "sort-order")]
    sort_order: Option<SortOrder>,

    /// Search for given value at startup
    #[clap(short = 'Q', long = "search")]
    search: Option<String>,

    //  #[clap(short = 'o', long = "monitor")]
    //  monitor: Option<String>, // todo support this

    //  #[clap(short = 'r', long = "pre-display-cmd")]
    //  pre_display_cmd: Option<String>, // todo support this
    /// Defines if window is aligned vertically or horizontally.
    #[clap(long = "orientation")]
    orientation: Option<Orientation>,

    /// Horizontal alignment
    #[clap(long = "halign")]
    halign: Option<Align>,

    /// Alignment of content
    #[clap(long = "content-halign")]
    content_halign: Option<Align>,

    /// center content on vertical axis
    #[clap(long = "content-vcenter")]
    content_vcenter: Option<bool>,

    /// Vertical alignment
    #[clap(long = "valign")]
    valign: Option<Align>,

    /// Defines the image size in pixels
    #[clap(long = "image-size")]
    image_size: Option<u16>,

    /// If set to `true` the search field will be hidden.
    #[clap(long = "hide-search")]
    hide_search: Option<bool>,

    /// can be set to a key to toggle the search bar.
    /// default is not set.
    #[clap(long = "key-hide-search")]
    key_hide_search: Option<Key>,

    /// Key to run the associated thing.
    /// Defaults to enter
    #[clap(long = "key-submit")]
    key_submit: Option<Key>,

    /// Key to close the window.
    /// Defaults to escape
    #[clap(long = "key-exit")]
    key_exit: Option<Key>,

    /// Can be set to a Key which copies the action to the clipboard.
    /// Copying to clipboard does not affect any cache file
    #[clap(long = "key-copy")]
    key_copy: Option<Key>,

    /// Used to expand or autocomplete entries. Defaults to tab
    #[clap(long = "key-expand")]
    key_expand: Option<Key>,

    /// If enabled, worf will resize according to the amount of displayed rows
    /// defaults to false
    #[clap(long = "dynamic-lines")]
    dynamic_lines: Option<bool>,

    /// If enabled, dynamic lines do not exceed the maximum height specified in the
    /// `height` option. It does ot evaluate the `lines` option though
    /// defaults to true
    #[clap(long = "dynamic-lines-limit")]
    dynamic_lines_limit: Option<bool>,

    /// Defines the layer worf is running on.
    /// Has no effect when normal window is used.
    /// defaults to `Top`
    #[clap(long = "layer")]
    layer: Option<Layer>,

    /// If set to `true` single click instead of double click will select
    /// Defaults to `false`
    #[clap(long = "single-click")]
    single_click: Option<bool>,

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

    /// Defines the search query to use.
    /// Defaults to `<https://duckduckgo.com/?q=>`
    #[clap(long = "search-query")]
    search_query: Option<String>,

    /// Blur the background of the screen
    /// can be styled via `background`
    #[clap(long = "blurred-background")]
    blurred_background: Option<bool>,

    /// Set the background to full screen.
    /// Might look better for some things, but
    /// there can only be one fullscreened app at the time.
    /// Defaults to false.
    #[clap(long = "blurred-background-fullscreen")]
    blurred_background_fullscreen: Option<bool>,

    /// Allow submitting selected entry with expand key if there is only 1 item left.
    #[clap(long = "submit-with-expand")]
    submit_with_expand: Option<bool>,

    /// Auto select when only 1 possible choice is left
    #[clap(long = "auto-select-on-search")]
    auto_select_on_search: Option<bool>,
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
    pub fn single_click(&self) -> bool {
        self.single_click.unwrap_or(false)
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
    pub fn content_vcenter(&self) -> bool {
        self.content_vcenter.unwrap_or(false)
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
    pub fn prompt(&self) -> Option<String> {
        self.prompt.clone()
    }

    pub fn set_prompt(&mut self, val: String) {
        self.prompt = Some(val);
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
    pub fn insensitive(&self) -> bool {
        self.insensitive.unwrap_or(true)
    }

    #[must_use]
    pub fn hide_search(&self) -> bool {
        self.hide_search.unwrap_or(false)
    }

    #[must_use]
    pub fn key_hide_search(&self) -> Option<Key> {
        self.key_hide_search
    }

    #[must_use]
    pub fn key_submit(&self) -> Key {
        self.key_submit.unwrap_or(Key::Enter)
    }

    #[must_use]
    pub fn key_exit(&self) -> Key {
        self.key_exit.unwrap_or(Key::Escape)
    }

    #[must_use]
    pub fn key_copy(&self) -> Option<Key> {
        self.key_copy
    }

    #[must_use]
    pub fn key_expand(&self) -> Key {
        self.key_expand.unwrap_or(Key::Tab)
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
    pub fn cache_file(&self) -> Option<String> {
        self.cache_file.clone()
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
    pub fn lines_size_factor(&self) -> f64 {
        self.lines_size_factor.unwrap_or(1.4)
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

    #[must_use]
    pub fn dynamic_lines_limit(&self) -> bool {
        self.dynamic_lines_limit.unwrap_or(true)
    }

    #[must_use]
    pub fn search_query(&self) -> String {
        self.search_query
            .clone()
            .unwrap_or_else(|| "https://duckduckgo.com/?q=".to_owned())
    }

    #[must_use]
    pub fn blurred_background(&self) -> bool {
        self.blurred_background.unwrap_or(false)
    }

    #[must_use]
    pub fn blurred_background_fullscreen(&self) -> bool {
        self.blurred_background_fullscreen.unwrap_or(false)
    }

    #[must_use]
    pub fn submit_with_expand(&self) -> bool {
        self.submit_with_expand.unwrap_or(true)
    }

    #[must_use]
    pub fn auto_select_on_search(&self) -> bool {
        self.auto_select_on_search.unwrap_or(false)
    }
}

fn default_false() -> bool {
    false
}

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
pub fn conf_path(full_path: Option<&String>, folder: &str, name: &str) -> Result<PathBuf, Error> {
    let alternative_paths =
        path_alternatives(vec![dirs::config_dir()], &PathBuf::from(folder).join(name));
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
pub fn load_worf_config(args_opt: Option<&Config>) -> Result<Config, Error> {
    let mut config = load_config(args_opt, "worf", "config")?;
    if let Some(args) = args_opt {
        let merge_result = merge_config_with_args(&mut config, args)
            .map_err(|e| Error::ParsingError(format!("{e}")))?;
        Ok(merge_result)
    } else {
        Ok(config)
    }
}

/// # Errors
///
/// Will return Err when it
/// * cannot read the config file
/// * cannot parse the config file
/// * no config file exists
/// * config file and args cannot be merged
pub fn load_config<T: DeserializeOwned>(
    args_opt: Option<&Config>,
    folder: &str,
    name: &str,
) -> Result<T, Error> {
    let config_path = conf_path(
        args_opt.as_ref().and_then(|c| c.cfg_path.as_ref()),
        folder,
        name,
    );
    match config_path {
        Ok(path) => {
            log::debug!("loading config from {}", path.display());
            let toml_content = fs::read_to_string(path).map_err(|e| Error::Io(format!("{e}")))?;
            toml::from_str(&toml_content).map_err(|e| Error::ParsingError(format!("{e}")))
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
pub fn merge_config_with_args(config: &mut Config, args: &Config) -> Result<Config, Error> {
    let args_json = serde_json::to_value(args).map_err(|e| Error::ParsingError(e.to_string()))?;
    let mut config_json =
        serde_json::to_value(config).map_err(|e| Error::ParsingError(e.to_string()))?;

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

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

// Define a custom error type using the `thiserror` crate
#[derive(Debug, Error)]
pub enum ArgsError {
    #[error("input is not valid {0}")]
    InvalidParameter(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Mode {
    /// searches $PATH for executables and allows them to be run by selecting them.
    Run,
    ///  searches $XDG_DATA_HOME/applications and $XDG_DATA_DIRS/applications for desktop files and allows them to be run by selecting them.
    Drun,

    /// reads from stdin and displays options which when selected will be output to stdout.
    Dmenu,
}

impl FromStr for Mode {
    type Err = ArgsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "run" => Ok(Mode::Run),
            "drun" => Ok(Mode::Drun),
            "dmenu" => Ok(Mode::Dmenu),
            _ => Err(ArgsError::InvalidParameter(
                format!("{s} is not a valid argument show this, see help for details").to_owned(),
            )),
        }
    }
}

#[derive(Parser, Debug, Deserialize, Serialize)]
#[clap(about = "Ravi is a wofi clone written in rust, it aims to be a drop in replacement")]
pub struct Args {
    /// Forks the menu so you can close the terminal
    #[clap(short = 'f', long = "fork")]
    fork: bool,

    /// Selects a config file to use
    #[clap(short = 'c', long = "conf")]
    pub config: Option<String>,

    /// Selects a stylesheet to use
    #[clap(short = 's', long = "style")]
    style: Option<String>,

    /// Selects a colors file to use
    #[clap(short = 'C', long = "color")]
    color: Option<String>,

    /// Runs in dmenu mode
    #[clap(short = 'd', long = "dmenu")]
    dmenu: bool,

    /// Specifies the mode to run in. A list can be found in wofi(7)
    #[clap(long = "show")]
    pub mode: Mode,

    /// Specifies the surface width
    #[clap(short = 'W', long = "width")]
    width: Option<String>,

    /// Specifies the surface height
    #[clap(short = 'H', long = "height")]
    height: Option<String>,

    /// Prompt to display
    #[clap(short = 'p', long = "prompt")]
    pub prompt: Option<String>,

    /// The x offset
    #[clap(short = 'x', long = "xoffset")]
    x: Option<String>,

    /// The y offset
    #[clap(short = 'y', long = "yoffset")]
    y: Option<String>,

    /// Render to a normal window
    #[clap(short = 'n', long = "normal-window")]
    normal_window: bool,

    /// Allows images to be rendered
    #[clap(short = 'I', long = "allow-images")]
    allow_images: bool,

    /// Allows pango markup
    #[clap(short = 'm', long = "allow-markup")]
    allow_markup: bool,

    /// Sets the cache file to use
    #[clap(short = 'k', long = "cache-file")]
    cache_file: Option<String>,

    /// Specifies the terminal to use when running in a term
    #[clap(short = 't', long = "term")]
    terminal: Option<String>,

    /// Runs in password mode
    #[clap(short = 'P', long = "password")]
    password_char: Option<String>,

    /// Makes enter always use the search contents, not the first result
    #[clap(short = 'e', long = "exec-search")]
    exec_search: bool,

    /// Hides the scroll bars
    #[clap(short = 'b', long = "hide-scroll")]
    hide_scroll: bool,

    /// Sets the matching method, default is contains
    #[clap(short = 'M', long = "matching")]
    matching: Option<String>,

    /// Allows case insensitive searching
    #[clap(short = 'i', long = "insensitive")]
    insensitive: bool,

    /// Parses the search text removing image escapes and pango
    #[clap(short = 'q', long = "parse-search")]
    parse_search: bool,

    /// Prints the version and then exits
    #[clap(short = 'v', long = "version")]
    version: bool,

    /// Sets the location
    #[clap(short = 'l', long = "location")]
    location: Option<String>,

    /// Disables multiple actions for modes that support it
    #[clap(short = 'a', long = "no-actions")]
    no_actions: bool,

    /// Sets a config option
    #[clap(short = 'D', long = "define")]
    define: Option<String>,

    /// Sets the height in number of lines
    #[clap(short = 'L', long = "lines")]
    lines: Option<String>,

    /// Sets the number of columns to display
    #[clap(short = 'w', long = "columns")]
    columns: Option<String>,

    /// Sets the sort order
    #[clap(short = 'O', long = "sort-order")]
    sort_order: Option<String>,

    /// Uses the dark variant of the current GTK theme
    #[clap(short = 'G', long = "gtk-dark")]
    gtk_dark: bool,

    /// Search for something immediately on open
    #[clap(short = 'Q', long = "search")]
    search: Option<String>,

    /// Sets the monitor to open on
    #[clap(short = 'o', long = "monitor")]
    monitor: Option<String>,

    /// Runs command for the displayed entries, without changing the output. %s for the real string
    #[clap(short = 'r', long = "pre-display-cmd")]
    pre_display_cmd: Option<String>,
}

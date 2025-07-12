use std::{
    env,
    fmt::Display,
    str::FromStr,
    sync::{Arc, RwLock},
};

use clap::Parser;
use worf::{Error, config, desktop::fork_if_configured, modes};

#[derive(Clone, Debug)]
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

    /// Open search engine.
    WebSearch,
}

#[derive(Debug, Parser)]
#[clap(
    about = "Worf is a wofi like launcher, written in rust, it aims to be a drop-in replacement"
)]
struct MainConfig {
    /// Defines the mode worf is running in
    #[clap(long = "show")]
    show: Mode,

    #[command(flatten)]
    worf: config::Config,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Run => write!(f, "run"),
            Mode::Drun => write!(f, "drun"),
            Mode::Dmenu => write!(f, "dmenu"),
            Mode::Math => write!(f, "math"),
            Mode::File => write!(f, "file"),
            Mode::Auto => write!(f, "auto"),
            Mode::Ssh => write!(f, "ssh"),
            Mode::Emoji => write!(f, "emoji"),
            Mode::WebSearch => write!(f, "websearch"),
        }
    }
}

impl FromStr for Mode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "run" => Ok(Mode::Run),
            "drun" => Ok(Mode::Drun),
            "dmenu" => Ok(Mode::Dmenu),
            "file" => Ok(Mode::File),
            "math" => Ok(Mode::Math),
            "ssh" => Ok(Mode::Ssh),
            "emoji" => Ok(Mode::Emoji),
            "websearch" => Ok(Mode::WebSearch),
            "auto" => Ok(Mode::Auto),
            _ => Err(Error::InvalidArgument(
                format!("{s} is not a valid argument, see help for details").to_owned(),
            )),
        }
    }
}

fn main() {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let mut config = MainConfig::parse();
    config.worf = config::load_worf_config(Some(&config.worf)).unwrap_or(config.worf);
    if config.worf.prompt().is_none() {
        config.worf.set_prompt(config.show.to_string());
    }

    if config.worf.version() {
        println!("worf version {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    fork_if_configured(&config.worf); // may exit the program

    let cfg_arc = Arc::new(RwLock::new(config.worf));
    let result = match config.show {
        Mode::Run => modes::run::show(&cfg_arc),
        Mode::Drun => modes::drun::show(&cfg_arc),
        Mode::Dmenu => modes::dmenu::show(&cfg_arc),
        Mode::File => modes::file::show(&cfg_arc),
        Mode::Math => {
            modes::math::show(&cfg_arc);
            Ok(())
        }
        Mode::Ssh => modes::ssh::show(&cfg_arc),
        Mode::Emoji => modes::emoji::show(&cfg_arc),
        Mode::Auto => modes::auto::show(&cfg_arc),
        Mode::WebSearch => modes::search::show(&cfg_arc),
    };

    if let Err(err) = result {
        if err == Error::NoSelection {
            log::info!("no selection made");
        } else {
            log::error!("Error occurred {err:?}");
            std::process::exit(1);
        }
    }
}

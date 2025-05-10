use std::fmt;

/// Configuration and command line parsing
pub mod config;
/// Desktop action like parsing desktop files and launching programs
pub mod desktop;
/// All things related to the user interface
pub mod gui;
/// Out of the box supported modes, like drun, dmenu, etc...
pub mod modes;

/// Defines error the lib can encounter
#[derive(Debug, PartialEq)]
pub enum Error {
    /// Failed to update a cache file with the given reason.
    UpdateCacheError(String),
    /// A given or configured file was not found, will also be used when
    /// cache files are missing.
    MissingFile,
    /// Failed to read form standard input. I.e. used for dmenu.
    StdInReadFail,
    /// The selection was invalid or looking up the element failed or another reason.
    InvalidSelection,
    /// The given parameters did not yield an icon.
    MissingIcon,
    /// Parsing a configuration or cache file failed.
    ParsingError(String),
    /// A menu item was expected to have an action but none was found.
    MissingAction,
    /// Running the action failed with the given reason.
    RunFailed(String),
    /// An IO operation failed
    Io(String),
    /// An error occurred while accessing the clipboard
    Clipboard(String),
    /// Graphical subsystem related error
    Graphics(String),
    /// Nothing selected
    NoSelection,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::UpdateCacheError(s) => write!(f, "UpdateCacheError {s}"),
            Error::MissingAction => write!(f, "MissingAction"),
            Error::StdInReadFail => write!(f, "StdInReadFail"),
            Error::InvalidSelection => write!(f, "InvalidSelection"),
            Error::MissingFile => {
                write!(f, "MissingFile")
            }
            Error::MissingIcon => {
                write!(f, "MissingIcon")
            }
            Error::ParsingError(s) => {
                write!(f, "ParsingError {s}")
            }
            Error::RunFailed(s) => {
                write!(f, "RunFailed {s}")
            }
            Error::Io(s) => {
                write!(f, "IO {s}")
            }
            Error::Clipboard(s) => {
                write!(f, "Clipboard {s}")
            }
            Error::Graphics(s) => {
                write!(f, "graphics {s}")
            }
            Error::NoSelection => {
                write!(f, "NoSelection")
            }
        }
    }
}

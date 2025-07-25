use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    fs,
    hash::BuildHasher,
    io,
    os::unix::{fs::PermissionsExt, prelude::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::LazyLock,
    time::Instant,
};

// re-export freedesktop_file_parser for easier access
pub use freedesktop_file_parser::{DesktopFile, EntryType};
use notify_rust::Notification;
use rayon::prelude::*;
use regex::Regex;
use wl_clipboard_rs::copy::{ClipboardType, MimeType, ServeRequests, Source};

use crate::{
    Error,
    config::{Config, expand_path},
};

/// Returns a regex with supported image extensions
/// # Panics
///
/// When it cannot parse the internal regex
#[must_use]
pub fn known_image_extension_regex_pattern() -> Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i).*\.(png|jpg|gif|svg|jpeg)$").unwrap());
    RE.clone()
}

/// Helper function to retrieve a file with given regex.
fn find_files(folder: &Path, file_name: &Regex) -> Option<Vec<PathBuf>> {
    if !folder.exists() || !folder.is_dir() {
        return None;
    }
    fs::read_dir(folder).ok().map(|entries| {
        entries
            .filter_map(Result::ok)
            .par_bridge() // Convert to parallel iterator
            .filter_map(|entry| entry.path().canonicalize().ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .and_then(|e| e.to_str())
                    .is_some_and(|name| file_name.is_match(name))
            })
            .collect()
    })
}

/// Parse all desktop files in known locations
/// * /usr/share/applications
/// * /usr/local/share/applications
/// * /var/lib/flatpak/exports/share/applications
/// # Panics
///
/// When it cannot parse the internal regex
#[must_use]
pub fn find_desktop_files() -> Vec<DesktopFile> {
    static DESKTOP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i).*\.desktop$").unwrap());

    let mut paths = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];

    let start = Instant::now();

    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local/share/applications"));
    }

    if let Ok(xdg_data_home) = env::var("XDG_DATA_HOME") {
        paths.push(PathBuf::from(xdg_data_home).join(".applications"));
    }

    if let Ok(xdg_data_dirs) = env::var("XDG_DATA_DIRS") {
        for dir in xdg_data_dirs.split(':') {
            paths.push(PathBuf::from(dir).join(".applications"));
        }
    }

    let p: Vec<_> = paths
        .into_par_iter()
        .filter(|desktop_dir| desktop_dir.exists())
        .filter_map(|desktop_dir| find_files(&desktop_dir, &DESKTOP_RE))
        .flat_map(|desktop_files| {
            desktop_files.into_par_iter().filter_map(|desktop_file| {
                fs::read_to_string(desktop_file)
                    .ok()
                    .and_then(|content| freedesktop_file_parser::parse(&content).ok())
            })
        })
        .collect();
    log::debug!("Found {} desktop files in {:?}", p.len(), start.elapsed());
    p
}

/// Return all possible locales based on the users preferences
#[must_use]
pub fn get_locale_variants() -> Vec<String> {
    let locale = env::var("LC_ALL")
        .or_else(|_| env::var("LC_MESSAGES"))
        .or_else(|_| env::var("LANG"))
        .unwrap_or_else(|_| "c".to_string());

    let lang = locale.split('.').next().unwrap_or(&locale).to_lowercase();
    let mut variants = vec![];

    if let Some((lang_part, region)) = lang.split_once('_') {
        variants.push(format!("{lang_part}_{region}")); // en_us
        variants.push(lang_part.to_string()); // en
    } else {
        variants.push(lang.clone()); // e.g. "fr"
    }

    variants
}

/// Lookup a value from a hashmap with respect to current locale
#[must_use]
pub fn lookup_name_with_locale<S: BuildHasher>(
    locale_variants: &[String],
    variants: &HashMap<String, String, S>,
    fallback: &str,
) -> Option<String> {
    locale_variants
        .iter()
        .find_map(|local| variants.get(local))
        .map(std::borrow::ToOwned::to_owned)
        .or_else(|| Some(fallback.to_owned()))
}

/// Fork into background if configured
/// # Panics
/// Panics if preexec and or setsid do not work
pub fn fork_if_configured(config: &Config) {
    let fork_env_var = "WORF_PROCESS_IS_FORKED";
    if config.fork() && env::var(fork_env_var).is_err() {
        let mut cmd = Command::new(env::current_exe().expect("Failed to get current executable"));

        for arg in env::args().skip(1) {
            cmd.arg(arg);
        }

        start_forked_cmd(cmd).expect("Failed to fork to background");
        std::process::exit(0);
    }
}

/// Spawn a new process and forks it away from the current worf process
/// # Errors
/// * No action in menu item
/// * Cannot run command (i.e. not found)
/// # Panics
/// When internal regex unwrapping fails. Should not happen as the regex is static
pub fn spawn_fork(cmd: &str, working_dir: Option<&String>) -> Result<(), Error> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"'([^']*)'|"([^"]*)"|(\S+)"#).unwrap());
    let re = &*RE;
    let parts: Vec<String> = re
        .captures_iter(cmd)
        .map(|cap| {
            cap.get(1)
                .or_else(|| cap.get(2))
                .or_else(|| cap.get(3))
                .unwrap()
                .as_str()
                .to_string()
        })
        .collect();

    if parts.is_empty() {
        return Err(Error::MissingAction);
    }

    if let Some(dir) = working_dir {
        env::set_current_dir(dir)
            .map_err(|e| Error::RunFailed(format!("cannot set workdir {e}")))?;
    }

    let exec = parts[0].replace('"', "");
    let args: Vec<_> = parts
        .iter()
        .skip(1)
        .filter(|arg| !arg.starts_with('%'))
        .map(|arg| expand_path(arg))
        .collect();

    start_forked(&exec, args)
}

fn start_forked<I, S>(exec: &str, args: I) -> Result<(), Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(exec);
    cmd.args(args);
    start_forked_cmd(cmd)
}

fn start_forked_cmd(mut cmd: Command) -> Result<(), Error> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    cmd.spawn().map_err(|e| Error::Io(e.to_string()))?;
    Ok(())
}

/// Get the path of a given cache file
/// # Errors
/// Will return Error if the cache file cannot be created or not found.
pub fn cache_file_path(config: &Config, name: &str) -> Result<PathBuf, Error> {
    let path = if let Some(cfg) = config.cache_file() {
        PathBuf::from(cfg)
    } else {
        dirs::cache_dir()
            .map(|x| x.join(name))
            .ok_or_else(|| Error::UpdateCacheError("cannot read cache file".to_owned()))?
    };

    create_file_if_not_exists(&path)?;
    Ok(path)
}

/// Parse a simple toml cache file from the format below
/// "Key"=score
/// i.e.
/// "Firefox"=42
/// "Chrome"=12
/// "Files"=50
/// # Errors
/// Returns an Error when the given file is not found or did not parse.
pub fn load_cache_file(cache_path: &PathBuf) -> Result<HashMap<String, i64>, Error> {
    let toml_content =
        fs::read_to_string(cache_path).map_err(|e| Error::UpdateCacheError(format!("{e}")))?;
    let parsed: toml::Value = toml_content
        .parse()
        .map_err(|_| Error::ParsingError("failed to parse cache".to_owned()))?;

    let mut result: HashMap<String, i64> = HashMap::new();
    if let toml::Value::Table(table) = parsed {
        for (key, val) in table {
            if let toml::Value::Integer(i) = val {
                result.insert(key, i);
            } else {
                log::warn!("Skipping key '{key}' because it's not an integer");
            }
        }
    }
    Ok(result)
}

/// Stores a cache file in the cache format. See `load_cache_file` for details.
/// # Errors
/// `Error::Parsing` if converting into toml was not possible
/// `Error::Io` if storing the file failed.
pub fn save_cache_file<S: BuildHasher>(
    path: &PathBuf,
    data: &HashMap<String, i64, S>,
) -> Result<(), Error> {
    // Convert the HashMap to TOML string
    let toml_string =
        toml::ser::to_string(&data).map_err(|e| Error::ParsingError(e.to_string()))?;
    fs::write(path, toml_string).map_err(|e| Error::Io(e.to_string()))?;
    Ok(())
}

/// Crates a new file if it does not exist yet.
/// # Errors
/// `Errors::Io` if creating the file failed
pub fn create_file_if_not_exists(path: &PathBuf) -> Result<(), Error> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path);

    match file {
        Ok(_) => Ok(()),

        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(Error::Io(e.to_string())),
    }
}

/// Check if the given dir entry is an executable
#[must_use]
pub fn is_executable(entry: &Path) -> bool {
    if let Ok(metadata) = entry.metadata() {
        let permissions = metadata.permissions();
        metadata.is_file() && (permissions.mode() & 0o111 != 0)
    } else {
        false
    }
}

/// Copy the given text into the clipboard.
/// # Errors
/// Will return an error if copying to the clipboard failed.
pub fn copy_to_clipboard(text: String, notify_body: Option<&str>) -> Result<(), Error> {
    let mut opts = wl_clipboard_rs::copy::Options::new();
    opts.clipboard(ClipboardType::Regular);
    opts.serve_requests(ServeRequests::Only(1));
    let result = opts.copy(Source::Bytes(text.into_bytes().into()), MimeType::Text);

    match result {
        Ok(()) => {
            let mut notification = Notification::new();
            notification.summary("Copied to clipboard");
            if let Some(notify_body) = notify_body {
                notification.body(notify_body);
            }

            notification.show().map_err(|e| Error::Io(e.to_string()))?;
            Ok(())
        }
        Err(e) => {
            Notification::new()
                .summary("Failed to copy to clipboard")
                .show()
                .map_err(|e| Error::Io(e.to_string()))?;
            Err(Error::Clipboard(e.to_string()))
        }
    }
}

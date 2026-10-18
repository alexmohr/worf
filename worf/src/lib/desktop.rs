use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    fs,
    hash::BuildHasher,
    io::{self, IsTerminal},
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

    let mut paths = Vec::<(usize, PathBuf)>::new();
    let start = Instant::now();

    // get files per prio, highest prio in homedir
    let mut prio = 0;
    if let Some(home) = dirs::home_dir() {
        paths.push((prio, home.join(".local/share/applications")));
        paths.push((
            prio,
            home.join(".local/share/flatpak/exports/share/applications"),
        ));
    }

    if let Ok(xdg_data_home) = env::var("XDG_DATA_HOME") {
        let x = PathBuf::from(&xdg_data_home);
        paths.push((prio, x.join("applications")));
        paths.push((prio, x.join("flatpak/exports/share/applications")));
    }

    prio += 1;
    if let Ok(xdg_data_dirs) = env::var("XDG_DATA_DIRS") {
        for dir in xdg_data_dirs.split(':') {
            paths.push((prio, PathBuf::from(dir).join("applications")));
        }
    }

    paths.push((prio, PathBuf::from("/usr/local/share/applications")));
    paths.push((
        prio,
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ));
    paths.push((prio, PathBuf::from("/usr/share/applications")));

    let files: Vec<(usize, PathBuf)> = paths
        .into_par_iter()
        .filter(|(_, dir)| dir.exists())
        .filter_map(|(prio, dir)| {
            find_files(&dir, &DESKTOP_RE).map(|v| {
                v.into_iter()
                    .map(move |file| (prio, file))
                    .collect::<Vec<_>>()
            })
        })
        .flatten()
        .collect();

    let mut file_by_prio: HashMap<String, (usize, PathBuf)> = HashMap::new();
    for (prio, path) in files {
        if let Some(name) = path.file_name().and_then(|x| x.to_str()) {
            match file_by_prio.get(name) {
                None => {
                    file_by_prio.insert(name.to_string(), (prio, path));
                }
                Some((existing_prio, _)) if prio < *existing_prio => {
                    // Smaller prio value = higher priority
                    file_by_prio.insert(name.to_string(), (prio, path));
                }
                _ => {} // keep existing
            }
        }
    }
    let parsed: Vec<DesktopFile> = file_by_prio
        .into_values()
        .collect::<Vec<_>>()
        .into_par_iter()
        .filter_map(|(_, desktop_file)| {
            fs::read_to_string(&desktop_file)
                .ok()
                .and_then(|content| freedesktop_file_parser::parse(&content).ok())
        })
        .collect();

    let mut by_name = HashMap::with_capacity(parsed.len());
    for desktop_file in parsed {
        by_name.insert(desktop_file.entry.name.default.clone(), desktop_file);
    }
    let p: Vec<_> = by_name.into_values().collect();
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

        cmd.args(env::args_os().skip(1));
        // Marks the child so it does not fork again.
        cmd.env(fork_env_var, "1");
        // Keep pipes and redirections (i.e. for dmenu mode),
        // but detach from the terminal so it can be closed.
        cmd.stdin(stdio_unless_terminal(&io::stdin()))
            .stdout(stdio_unless_terminal(&io::stdout()))
            .stderr(stdio_unless_terminal(&io::stderr()));

        detach_and_spawn(cmd).expect("Failed to fork to background");
        std::process::exit(0);
    }
}

/// Splits a command line like the `Exec` key of desktop entries into arguments.
///
/// Arguments are separated by whitespace. Double quoted parts may contain whitespace
/// and the escapes `\"`, `` \` ``, `\$` and `\\`. Single quoted parts are taken
/// literally. Quoted and unquoted parts next to each other form one argument,
/// i.e. `FOO="a b"` is `FOO=a b`.
///
/// Field codes (`%f`, `%U`, ...) are removed as worf does not pass files or URLs,
/// `%%` becomes a literal `%`.
#[must_use]
pub fn parse_exec(cmd: &str) -> Vec<String> {
    let mut args = Vec::new();
    // None until the current argument has any content, so `""` is an empty argument.
    let mut current: Option<String> = None;
    let mut chars = cmd.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if let Some(arg) = current.take() {
                    args.push(arg);
                }
            }
            '"' => {
                let arg = current.get_or_insert_with(String::new);
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.peek() {
                            Some(&escaped @ ('"' | '`' | '$' | '\\')) => {
                                arg.push(escaped);
                                chars.next();
                            }
                            _ => arg.push('\\'),
                        },
                        _ => arg.push(c),
                    }
                }
            }
            '\'' => {
                let arg = current.get_or_insert_with(String::new);
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    arg.push(c);
                }
            }
            '%' => {
                let arg = current.get_or_insert_with(String::new);
                match chars.next() {
                    // `%%` is a literal `%`, a trailing `%` is kept as well
                    Some('%') | None => arg.push('%'),
                    Some(code) if code.is_ascii_alphabetic() => {
                        // A field code that is a whole argument is removed with it.
                        if arg.is_empty() && chars.peek().is_none_or(|c| c.is_whitespace()) {
                            current = None;
                        }
                    }
                    Some(other) => {
                        arg.push('%');
                        arg.push(other);
                    }
                }
            }
            _ => current.get_or_insert_with(String::new).push(c),
        }
    }

    if let Some(arg) = current {
        args.push(arg);
    }
    args
}

/// Spawn a new process and forks it away from the current worf process
/// The command is split with [`parse_exec`], it is not run through a shell.
/// # Errors
/// * No action in menu item
/// * Cannot run command (i.e. not found)
pub fn spawn_fork(cmd: &str, working_dir: Option<&String>) -> Result<(), Error> {
    let mut parts = parse_exec(cmd).into_iter();
    let Some(exec) = parts.next() else {
        return Err(Error::MissingAction);
    };

    if let Some(dir) = working_dir {
        env::set_current_dir(dir)
            .map_err(|e| Error::RunFailed(format!("cannot set workdir {e}")))?;
    }

    start_forked(&expand_path(&exec).to_string_lossy(), parts)
}

fn stdio_unless_terminal(stream: &impl IsTerminal) -> Stdio {
    if stream.is_terminal() {
        Stdio::null()
    } else {
        Stdio::inherit()
    }
}

/// Spawns `exec` with the given arguments as a new session, detached from worf.
/// The arguments are passed verbatim, without splitting or expansion.
/// # Errors
/// Returns `Error::Io` if the process cannot be spawned.
pub(crate) fn start_forked<I, S>(exec: &str, args: I) -> Result<(), Error>
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
    detach_and_spawn(cmd)
}

fn detach_and_spawn(mut cmd: Command) -> Result<(), Error> {
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    cmd.spawn().map_err(|e| Error::Io(e.to_string()))?;
    Ok(())
}

/// Get the path of a given cache file.
/// Falls back to the default cache location if the configured
/// cache file cannot be created.
/// # Errors
/// Will return Error if the cache file cannot be created or not found.
pub fn cache_file_path(config: &Config, name: &str) -> Result<PathBuf, Error> {
    if let Some(cfg) = config.cache_file() {
        let path = PathBuf::from(cfg);
        match create_file_if_not_exists(&path) {
            Ok(()) => return Ok(path),
            Err(e) => log::error!(
                "Cannot use cache file {}: {e}, falling back to default",
                path.display()
            ),
        }
    }

    let path = dirs::cache_dir()
        .map(|x| x.join(name))
        .ok_or_else(|| Error::UpdateCacheError("cannot read cache file".to_owned()))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exec() {
        assert_eq!(parse_exec("firefox %u"), vec!["firefox"]);
        assert_eq!(
            parse_exec(r#""/opt/My App/app" --flag"#),
            vec!["/opt/My App/app", "--flag"]
        );
        assert_eq!(
            parse_exec(r#"env FOO="a b" app"#),
            vec!["env", "FOO=a b", "app"]
        );
        assert_eq!(
            parse_exec(r#"sh -c "echo \"hi\" \$HOME \\ done""#),
            vec!["sh", "-c", r#"echo "hi" $HOME \ done"#]
        );
        assert_eq!(parse_exec("printf 100%%"), vec!["printf", "100%"]);
        assert_eq!(parse_exec("app --file=%f"), vec!["app", "--file="]);
        assert_eq!(parse_exec("app '' 'a b'"), vec!["app", "", "a b"]);
        assert_eq!(parse_exec("  "), Vec::<String>::new());
    }
}

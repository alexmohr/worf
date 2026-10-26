use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::OsStr,
    fs,
    hash::BuildHasher,
    io::{self, IsTerminal},
    os::unix::{fs::PermissionsExt, prelude::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::LazyLock,
    thread,
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

/// Finds all files matching `file_name` in `folder` and its subdirectories.
/// Returns the desktop file id (the path relative to `folder` with `/` replaced by `-`,
/// as defined by the desktop entry spec) and the path of each file.
/// Symlinks are followed but not resolved, so the id is the one of the link.
fn find_files(folder: &Path, file_name: &Regex) -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, prefix: &str, file_name: &Regex, depth: usize) -> Vec<(String, PathBuf)> {
        // Guards against symlink loops.
        const MAX_DEPTH: usize = 8;
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };

        let mut files = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    files.extend(walk(
                        &path,
                        &format!("{prefix}{name}-"),
                        file_name,
                        depth + 1,
                    ));
                }
            } else if file_name.is_match(name) {
                files.push((format!("{prefix}{name}"), path));
            }
        }
        files
    }

    walk(folder, "", file_name, 0)
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
    find_desktop_files_with_paths()
        .into_iter()
        .map(|(_, desktop_file)| desktop_file)
        .collect()
}

/// Like [`find_desktop_files`], but also returns the path of each desktop file.
/// # Panics
///
/// When it cannot parse the internal regex
#[must_use]
pub fn find_desktop_files_with_paths() -> Vec<(PathBuf, DesktopFile)> {
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

    // Relative paths are invalid according to the XDG base directory spec and would
    // be resolved against the current directory, so they are ignored (as are empty ones).
    let absolute_dir = |dir: &str| Some(PathBuf::from(dir)).filter(|p| p.is_absolute());

    if let Some(x) = env::var("XDG_DATA_HOME")
        .ok()
        .and_then(|d| absolute_dir(&d))
    {
        paths.push((prio, x.join("applications")));
        paths.push((prio, x.join("flatpak/exports/share/applications")));
    }

    // Earlier entries in XDG_DATA_DIRS take precedence over later ones.
    if let Ok(xdg_data_dirs) = env::var("XDG_DATA_DIRS") {
        for dir in xdg_data_dirs.split(':').filter_map(absolute_dir) {
            prio += 1;
            paths.push((prio, dir.join("applications")));
        }
    }

    prio += 1;
    paths.push((prio, PathBuf::from("/usr/local/share/applications")));
    paths.push((
        prio,
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ));
    paths.push((prio, PathBuf::from("/usr/share/applications")));

    let files: Vec<(usize, String, PathBuf)> = paths
        .into_par_iter()
        .filter(|(_, dir)| dir.exists())
        .flat_map_iter(|(prio, dir)| {
            find_files(&dir, &DESKTOP_RE)
                .into_iter()
                .map(move |(id, file)| (prio, id, file))
        })
        .collect();

    // Files with the same desktop file id override each other, the highest priority
    // (smallest value) wins.
    let mut file_by_prio: HashMap<String, (usize, PathBuf)> = HashMap::new();
    for (prio, id, path) in files {
        match file_by_prio.get(&id) {
            Some((existing_prio, _)) if *existing_prio <= prio => {}
            _ => {
                file_by_prio.insert(id, (prio, path));
            }
        }
    }
    // Sorted, so the result does not depend on the order of the hash map.
    let mut files: Vec<(usize, PathBuf)> = file_by_prio.into_values().collect();
    files.sort();
    let parsed: Vec<(PathBuf, DesktopFile)> = files
        .into_par_iter()
        .filter_map(|(_, path)| {
            let content = fs::read_to_string(&path)
                .inspect_err(|e| log::warn!("Cannot read desktop file {}: {e}", path.display()))
                .ok()?;
            parse_desktop_file(&content)
                .inspect_err(|e| log::warn!("Cannot parse desktop file {}: {e}", path.display()))
                .ok()
                .map(|desktop_file| (path, desktop_file))
        })
        .collect();

    // Different files can describe the same application (i.e. a package and a local copy).
    // Only entries with the same name and command are duplicates, different applications
    // may share a name. The first, highest priority entry is kept. Hidden entries are
    // not duplicates of visible ones, otherwise they would shadow them (i.e. chrome ships
    // a hidden com.google.Chrome.desktop next to the visible google-chrome.desktop).
    let mut seen = HashSet::with_capacity(parsed.len());
    let p: Vec<_> = parsed
        .into_iter()
        .filter(|(_, desktop_file)| {
            let exec = match &desktop_file.entry.entry_type {
                EntryType::Application(app) => app.exec.clone(),
                _ => None,
            };
            let hidden = desktop_file.entry.no_display.unwrap_or(false)
                || desktop_file.entry.hidden.unwrap_or(false);
            seen.insert((desktop_file.entry.name.default.clone(), exec, hidden))
        })
        .collect();
    log::debug!("Found {} desktop files in {:?}", p.len(), start.elapsed());
    p
}

/// Parses a desktop file. The parser rejects files with localized `Icon[..]` keys or
/// repeated keys, which exist in the wild (i.e. nextcloud, chrome). In that case the
/// offending lines are dropped and parsing is retried.
fn parse_desktop_file(content: &str) -> Result<DesktopFile, freedesktop_file_parser::ParseError> {
    freedesktop_file_parser::parse(content).or_else(|err| {
        let mut seen = HashSet::new();
        let mut group = "";
        let sanitized = content
            .lines()
            .filter(|line| {
                let line = line.trim();
                if line.starts_with('[') {
                    group = line;
                    return true;
                }
                let Some((key, _)) = line.split_once('=') else {
                    return true;
                };
                let key = key.trim();
                !key.starts_with("Icon[") && (line.starts_with('#') || seen.insert((group, key)))
            })
            .collect::<Vec<_>>()
            .join("\n");
        freedesktop_file_parser::parse(&sanitized).map_err(|_| err)
    })
}

/// Return all possible locales based on the users preferences
#[must_use]
pub fn get_locale_variants() -> Vec<String> {
    // Empty variables count as unset, like in glibc.
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|var| env::var(var).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "C".to_owned());
    locale_variants(&locale)
}

/// Locale keys to look up for `locale` (i.e. `de_DE.UTF-8@euro`), most specific first,
/// as defined by the desktop entry spec: `lang_COUNTRY@MODIFIER`, `lang_COUNTRY`,
/// `lang@MODIFIER`, `lang`. The case is kept, desktop files use keys like `pt_BR`.
fn locale_variants(locale: &str) -> Vec<String> {
    let (locale, modifier) = match locale.split_once('@') {
        Some((locale, modifier)) => (locale, Some(modifier)),
        None => (locale, None),
    };
    // The encoding is not part of the key.
    let locale = locale.split('.').next().unwrap_or(locale);
    let (lang, country) = match locale.split_once('_') {
        Some((lang, country)) => (lang, Some(country)),
        None => (locale, None),
    };

    let mut variants = vec![];
    if let Some(country) = country {
        if let Some(modifier) = modifier {
            variants.push(format!("{lang}_{country}@{modifier}"));
        }
        variants.push(format!("{lang}_{country}"));
    }
    if let Some(modifier) = modifier {
        variants.push(format!("{lang}@{modifier}"));
    }
    variants.push(lang.to_owned());
    variants
}

/// Lookup a value from a hashmap with respect to current locale.
/// Returns `fallback` if there is no value for any of the locale variants.
#[must_use]
pub fn lookup_name_with_locale<S: BuildHasher>(
    locale_variants: &[String],
    variants: &HashMap<String, String, S>,
    fallback: &str,
) -> String {
    locale_variants
        .iter()
        .find_map(|local| variants.get(local))
        .map_or_else(|| fallback.to_owned(), ToOwned::to_owned)
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

/// Values for the field codes of an `Exec` line that refer to the desktop entry itself.
pub struct ExecFields<'a> {
    /// The `Icon` key, used for `%i`
    pub icon: Option<&'a str>,
    /// The translated `Name`, used for `%c`
    pub name: &'a str,
    /// Path of the desktop file, used for `%k`
    pub desktop_file: Option<&'a Path>,
}

/// Expands the field codes `%i`, `%c` and `%k` of an `Exec` line, as defined by the desktop
/// entry spec. The values are quoted, so the result can be split with [`parse_exec`].
/// `%i` becomes `--icon <icon>`, or nothing if there is no icon.
/// Other field codes are kept, [`parse_exec`] removes them.
#[must_use]
pub fn expand_field_codes(exec: &str, fields: &ExecFields) -> String {
    fn quote(value: &str) -> String {
        let mut quoted = String::with_capacity(value.len() + 2);
        quoted.push('"');
        for c in value.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                quoted.push('\\');
            }
            quoted.push(c);
        }
        quoted.push('"');
        quoted
    }

    let mut result = String::with_capacity(exec.len());
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('i') => {
                if let Some(icon) = fields.icon.filter(|i| !i.is_empty()) {
                    result.push_str("--icon ");
                    result.push_str(&quote(icon));
                }
            }
            Some('c') => result.push_str(&quote(fields.name)),
            Some('k') => {
                if let Some(path) = fields.desktop_file {
                    result.push_str(&quote(&path.to_string_lossy()));
                }
            }
            Some(other) => {
                result.push('%');
                result.push(other);
            }
            None => result.push('%'),
        }
    }
    result
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

    let mut cmd = Command::new(expand_path(&exec));
    cmd.args(parts);
    if let Some(dir) = working_dir {
        // Only for the child, worf itself keeps its working directory.
        cmd.current_dir(expand_path(dir));
    }
    start_forked_cmd(cmd)
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

    let mut child = cmd.spawn().map_err(|e| Error::Io(e.to_string()))?;
    // Reap the child when it exits, so it does not stay a zombie while worf keeps running.
    // The thread does not keep worf alive, it ends with the process.
    thread::spawn(move || child.wait());
    Ok(())
}

/// Get the path of a given cache file.
/// Falls back to the default cache location if the configured
/// cache file cannot be created.
/// # Errors
/// Will return Error if the cache file cannot be created or not found.
pub fn cache_file_path(config: &Config, name: &str) -> Result<PathBuf, Error> {
    if let Some(cfg) = config.cache_file() {
        let path = expand_path(&cfg);
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

    // Write to a temporary file and rename it, so a crash or a concurrent worf
    // never leaves a truncated cache behind.
    let mut tmp_name = path.file_name().unwrap_or_default().to_owned();
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp_path = path.with_file_name(tmp_name);
    fs::write(&tmp_path, toml_string)
        .and_then(|()| fs::rename(&tmp_path, path))
        .map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            Error::Io(e.to_string())
        })
}

/// Crates a new file and its parent directories if it does not exist yet.
/// # Errors
/// `Errors::Io` if creating the file failed
pub fn create_file_if_not_exists(path: &PathBuf) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::Io(e.to_string()))?;
    }
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

    // The notification is only informational, failing to show it (i.e. because no
    // notification daemon runs) must not turn a successful copy into an error.
    let mut notification = Notification::new();
    match result {
        Ok(()) => {
            notification.summary("Copied to clipboard");
            if let Some(notify_body) = notify_body {
                notification.body(notify_body);
            }
        }
        Err(_) => {
            notification.summary("Failed to copy to clipboard");
        }
    }
    if let Err(e) = notification.show() {
        log::warn!("Cannot show notification: {e}");
    }

    result.map_err(|e| Error::Clipboard(e.to_string()))
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

    #[test]
    fn test_find_files_uses_desktop_file_ids() {
        let dir = env::temp_dir().join(format!("worf-test-ids-{}", std::process::id()));
        fs::create_dir_all(dir.join("kde")).unwrap();
        fs::write(dir.join("a.desktop"), "").unwrap();
        fs::write(dir.join("kde").join("b.desktop"), "").unwrap();
        fs::write(dir.join("readme.txt"), "").unwrap();

        let re = Regex::new(r"\.desktop$").unwrap();
        let mut ids: Vec<String> = find_files(&dir, &re)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec!["a.desktop", "kde-b.desktop"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_locale_variants() {
        assert_eq!(
            locale_variants("de_DE.UTF-8@euro"),
            vec!["de_DE@euro", "de_DE", "de@euro", "de"]
        );
        assert_eq!(locale_variants("pt_BR.UTF-8"), vec!["pt_BR", "pt"]);
        assert_eq!(locale_variants("fr"), vec!["fr"]);
    }

    #[test]
    fn test_expand_field_codes() {
        let path = PathBuf::from("/usr/share/applications/my app.desktop");
        let fields = ExecFields {
            icon: Some("my-icon"),
            name: "My \"App\"",
            desktop_file: Some(&path),
        };
        let expanded = expand_field_codes("app %i --name=%c %k %U 100%%", &fields);
        assert_eq!(
            parse_exec(&expanded),
            vec![
                "app",
                "--icon",
                "my-icon",
                "--name=My \"App\"",
                "/usr/share/applications/my app.desktop",
                "100%",
            ]
        );

        let no_icon = ExecFields {
            icon: None,
            name: "x",
            desktop_file: None,
        };
        assert_eq!(
            parse_exec(&expand_field_codes("app %i %k", &no_icon)),
            vec!["app"]
        );
    }

    #[test]
    fn test_cache_file_round_trip() {
        let dir = env::temp_dir().join(format!("worf-test-{}", std::process::id()));
        let path = dir.join("nested").join("cache");
        create_file_if_not_exists(&path).expect("create failed");

        let data = HashMap::from([("Firefox".to_owned(), 42), ("My App".to_owned(), 1)]);
        save_cache_file(&path, &data).expect("save failed");
        assert_eq!(load_cache_file(&path).expect("load failed"), data);

        let leftovers = fs::read_dir(path.parent().unwrap()).unwrap().count();
        assert_eq!(leftovers, 1, "temporary file was not removed");
        fs::remove_dir_all(dir).unwrap();
    }
}

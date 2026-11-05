
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppEntry {
    pub name: String,
    pub target: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub icon: Option<PathBuf>,
    pub is_game: bool,
    pub on_desktop: bool,
    pub hidden: bool,
    pub file: Option<PathBuf>,
}

impl AppEntry {
    fn plain(path: &Path) -> Self {
        AppEntry {
            name: display_name(path),
            target: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            working_dir: None,
            icon: None,
            is_game: false,
            on_desktop: false,
            hidden: false,
            file: Some(path.to_path_buf()),
        }
    }
}

pub fn from_file(path: &Path) -> AppEntry {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let parsed = match ext.as_str() {
        "desktop" => parse_desktop(path),
        "url" => parse_url_shortcut(path),
        _ => None,
    };
    parsed.unwrap_or_else(|| AppEntry::plain(path))
}

pub fn from_command(cmd: &str) -> Option<AppEntry> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }
    if crate::launcher::is_uri(cmd) {
        return Some(AppEntry { name: cmd.to_owned(), target: cmd.to_owned(), file: None, ..AppEntry::plain(Path::new(cmd)) });
    }
    let mut argv = split_command(cmd);
    let target = argv.remove(0);
    if argv.is_empty() && Path::new(&target).is_file() {
        return Some(from_file(Path::new(&target)));
    }
    let name_source = argv.iter().rev().chain([&target]).find(|a| Path::new(a).is_file()).unwrap_or(&target);
    Some(AppEntry { name: display_name(Path::new(name_source)), target, args: argv, file: None, ..AppEntry::plain(Path::new(cmd)) })
}

pub fn installed() -> Vec<AppEntry> {
    let mut seen = HashSet::new();
    let mut apps: Vec<AppEntry> = scan_roots()
        .into_iter()
        .flat_map(|(dir, on_desktop)| {
            let mut files = Vec::new();
            collect_files(&dir, 4, &mut files);
            files.into_iter().filter_map(move |f| entry_for_scan(&f, on_desktop))
        })
        .filter(|a| !a.hidden && seen.insert((a.name.clone(), a.target.clone(), a.args.clone())))
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

fn collect_files(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        let is_bundle = path.extension().is_some_and(|e| e == "app");
        if path.is_dir() && !is_bundle {
            if depth > 0 {
                collect_files(&path, depth - 1, out);
            }
        } else {
            out.push(path);
        }
    }
}

fn display_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().replace(['_', '-'], " "))
        .unwrap_or_else(|| "New game".into())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn scan_roots() -> Vec<(PathBuf, bool)> {
    let mut roots: Vec<(PathBuf, bool)> = xdg_data_dirs().into_iter().map(|d| (d.join("applications"), false)).collect();
    if let Some(desktop) = desktop_dir() {
        roots.push((desktop, true));
    }
    roots
}

#[cfg(all(unix, not(target_os = "macos")))]
fn entry_for_scan(path: &Path, on_desktop: bool) -> Option<AppEntry> {
    if path.extension()? != "desktop" {
        return None;
    }
    let mut entry = parse_desktop(path)?;
    entry.on_desktop = on_desktop;
    Some(entry)
}

#[cfg(windows)]
fn scan_roots() -> Vec<(PathBuf, bool)> {
    let mut roots = Vec::new();
    for var in ["APPDATA", "ProgramData"] {
        if let Some(base) = std::env::var_os(var) {
            roots.push((PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs"), false));
        }
    }
    if let Some(desktop) = desktop_dir() {
        roots.push((desktop, true));
    }
    if let Some(public) = std::env::var_os("PUBLIC") {
        roots.push((PathBuf::from(public).join("Desktop"), true));
    }
    roots
}

#[cfg(windows)]
fn entry_for_scan(path: &Path, on_desktop: bool) -> Option<AppEntry> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mut entry = match ext.as_str() {
        "url" => parse_url_shortcut(path)?,
        "lnk" => AppEntry::plain(path),
        _ => return None,
    };
    let lower = entry.name.to_lowercase();
    if lower.starts_with("uninstall") || lower.contains("uninstall ") {
        return None;
    }
    if ext == "lnk" {
        let folders = path.parent().map(|p| p.to_string_lossy().to_lowercase()).unwrap_or_default();
        entry.is_game = ["game", "steam", "epic", "gog", "heroic", "ubisoft", "battle.net", "ea app"]
            .iter()
            .any(|k| folders.contains(k));
    }
    entry.on_desktop = on_desktop;
    Some(entry)
}

#[cfg(target_os = "macos")]
fn scan_roots() -> Vec<(PathBuf, bool)> {
    let mut roots = vec![(PathBuf::from("/Applications"), false)];
    if let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()) {
        roots.push((home.join("Applications"), false));
    }
    roots
}

#[cfg(target_os = "macos")]
fn entry_for_scan(path: &Path, _on_desktop: bool) -> Option<AppEntry> {
    (path.extension()? == "app").then(|| AppEntry::plain(path))
}

#[cfg(not(target_os = "macos"))]
fn desktop_dir() -> Option<PathBuf> {
    directories::UserDirs::new().and_then(|u| u.desktop_dir().map(Path::to_path_buf))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn xdg_data_dirs() -> Vec<PathBuf> {
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()).unwrap_or_default();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());

    let mut dirs = vec![data_home.clone(), data_home.join("flatpak/exports/share")];
    dirs.extend(data_dirs.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs.push("/var/lib/flatpak/exports/share".into());
    dirs.push("/var/lib/snapd/desktop".into());
    let mut seen = HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

pub fn parse_desktop(path: &Path) -> Option<AppEntry> {
    let text = fs::read_to_string(path).ok()?;
    let keys = ini_group(&text, "Desktop Entry");
    let flag = |k: &str| keys.get(k).is_some_and(|v| v.eq_ignore_ascii_case("true"));

    let (target, args) = match keys.get("Type").map(String::as_str).unwrap_or("Application") {
        "Application" => {
            let mut argv = desktop_exec_args(keys.get("Exec")?);
            if argv.is_empty() {
                return None;
            }
            let program = argv.remove(0);
            (program, argv)
        }
        "Link" => (keys.get("URL")?.clone(), Vec::new()),
        _ => return None,
    };

    Some(AppEntry {
        name: localized(&keys, "Name").cloned().unwrap_or_else(|| display_name(path)),
        target,
        args,
        working_dir: keys.get("Path").filter(|p| !p.is_empty()).map(PathBuf::from),
        icon: keys.get("Icon").and_then(|i| resolve_icon(i)),
        is_game: keys.get("Categories").is_some_and(|c| c.split(';').any(|c| c == "Game")),
        on_desktop: false,
        hidden: flag("NoDisplay") || flag("Hidden"),
        file: Some(path.to_path_buf()),
    })
}

fn parse_url_shortcut(path: &Path) -> Option<AppEntry> {
    let text = fs::read_to_string(path).ok()?;
    let url = ini_group(&text, "InternetShortcut").remove("URL")?;
    let lower = url.to_lowercase();
    Some(AppEntry {
        is_game: ["steam://", "com.epicgames", "heroic://", "goggalaxy://", "uplay://"]
            .iter()
            .any(|s| lower.starts_with(s)),
        target: url,
        ..AppEntry::plain(path)
    })
}

fn ini_group(text: &str, group: &str) -> HashMap<String, String> {
    let mut in_group = false;
    let mut keys = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_group = line == format!("[{group}]");
        } else if in_group
            && !line.starts_with('#')
            && let Some((k, v)) = line.split_once('=')
        {
            keys.entry(k.trim().to_owned()).or_insert_with(|| unescape_value(v.trim()));
        }
    }
    keys
}

fn localized<'a>(keys: &'a HashMap<String, String>, key: &str) -> Option<&'a String> {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .unwrap_or_default();
    localized_for(keys, key, &locale)
}

fn localized_for<'a>(keys: &'a HashMap<String, String>, key: &str, locale: &str) -> Option<&'a String> {
    let locale = locale.split(['.', '@']).next().unwrap_or("");
    let lang = locale.split('_').next().unwrap_or("");
    [locale, lang]
        .iter()
        .filter(|l| !l.is_empty())
        .find_map(|l| keys.get(&format!("{key}[{l}]")))
        .or_else(|| keys.get(key))
}

fn unescape_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn desktop_exec_args(exec: &str) -> Vec<String> {
    split_command(exec)
        .into_iter()
        .filter_map(|arg| {
            if arg.len() == 2 && arg.starts_with('%') && arg != "%%" {
                return None;
            }
            let mut out = String::new();
            let mut chars = arg.chars();
            while let Some(c) = chars.next() {
                if c == '%' {
                    if let Some('%') = chars.next() {
                        out.push('%');
                    }
                } else {
                    out.push(c);
                }
            }
            Some(out)
        })
        .collect()
}

pub fn split_command(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('"'), '\\') if chars.peek().is_some_and(|n| "\"`$\\".contains(*n)) => {
                cur.push(chars.next().unwrap());
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if started || !cur.is_empty() {
        args.push(cur);
    }
    args
}

pub fn join_command(target: &str, args: &[String]) -> String {
    std::iter::once(target)
        .chain(args.iter().map(String::as_str))
        .map(|a| {
            if !a.is_empty() && !a.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
                return a.to_owned();
            }
            let mut q = String::from('"');
            let mut chars = a.chars().peekable();
            while let Some(c) = chars.next() {
                let escapes_next = c == '\\' && chars.peek().is_none_or(|n| "\"`$\\".contains(*n));
                if c == '"' || c == '`' || c == '$' || escapes_next {
                    q.push('\\');
                }
                q.push(c);
            }
            q.push('"');
            q
        })
        .collect::<Vec<_>>()
        .join(" ")
}

const ICON_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "svg"];

fn resolve_icon(icon: &str) -> Option<PathBuf> {
    let is_image = |p: &Path| {
        p.is_file()
            && p.extension().and_then(|e| e.to_str()).is_some_and(|e| ICON_EXTS.contains(&e.to_ascii_lowercase().as_str()))
    };
    let path = Path::new(icon);
    if path.is_absolute() {
        return is_image(path).then(|| path.to_path_buf());
    }
    theme_icon(icon, &is_image)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn theme_icon(name: &str, is_image: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    const SIZES: &[&str] = &["512x512", "256x256", "scalable", "128x128", "96x96", "64x64", "48x48"];
    let data_dirs = xdg_data_dirs();
    for size in SIZES {
        for base in &data_dirs {
            for ext in ["png", "svg"] {
                let p = base.join(format!("icons/hicolor/{size}/apps/{name}.{ext}"));
                if is_image(&p) {
                    return Some(p);
                }
            }
        }
    }
    data_dirs
        .iter()
        .flat_map(|d| ["png", "svg"].map(|ext| d.join(format!("pixmaps/{name}.{ext}"))))
        .find(|p| is_image(p))
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn theme_icon(_name: &str, _is_image: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heroic_desktop_entry() {
        let dir = std::env::temp_dir().join(format!("yasaff-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Hades.desktop");
        fs::write(
            &file,
            "[Desktop Entry]\nName=Hades\nName[pl]=Hades PL\nExec=xdg-open 'heroic://launch?appName=Min&runner=legendary'\n\
             Terminal=false\nType=Application\nIcon=/nonexistent/icon.jpg\nCategories=Game;\n\n[Desktop Action x]\nName=Other\n",
        )
        .unwrap();
        let e = from_file(&file);
        fs::remove_dir_all(&dir).unwrap();
        assert!(e.name.starts_with("Hades"));
        assert_eq!(e.target, "xdg-open");
        assert_eq!(e.args, ["heroic://launch?appName=Min&runner=legendary"]);
        assert!(e.is_game);
        assert_eq!(e.icon, None);
    }

    #[test]
    fn localized_name() {
        let keys = ini_group("[Desktop Entry]\nName=Kombo Breiker: Breaking Inside\nName[en_US]=KB Breaking Inside\n", "Desktop Entry");
        assert_eq!(localized_for(&keys, "Name", "en_US.UTF-8").unwrap(), "KB Breaking Inside");
        assert_eq!(localized_for(&keys, "Name", "de_DE.UTF-8").unwrap(), "Kombo Breiker: Breaking Inside");
    }

    #[test]
    fn exec_field_codes_and_quotes() {
        assert_eq!(
            desktop_exec_args(r#"env WINEPREFIX="/home/u/My Games" wine "C:\\\\Game\\\\run.exe" %U --pct=100%%"#),
            ["env", "WINEPREFIX=/home/u/My Games", "wine", r"C:\\Game\\run.exe", "--pct=100%"]
        );
        assert_eq!(desktop_exec_args("steam steam://rungameid/570 %u"), ["steam", "steam://rungameid/570"]);
    }

    #[test]
    fn command_round_trip() {
        let args = vec!["--mode".to_owned(), "My Save".to_owned(), String::new()];
        let joined = join_command(r"C:\Program Files\Game\game.exe", &args);
        assert_eq!(joined, r#""C:\Program Files\Game\game.exe" --mode "My Save" """#);
        let back = split_command(&joined);
        assert_eq!(back[0], r"C:\Program Files\Game\game.exe");
        assert_eq!(&back[1..], &args[..]);
    }
}


use crate::library::{Game, Library};
use crate::settings::Settings;
use crate::stores::Store;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const DEFAULT: &str = "DefaultUser";
const STATE: &str = "profiles.json";
const MARKER: &str = "yasaffProfile.json";
const PATH_FILES: [&str; 2] = ["library.json", "userSettings.json"];
const SKIPPED: [&str; 1] = ["cache"];

static ACTIVE: OnceLock<(String, PathBuf)> = OnceLock::new();

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct State {
    active: String,
}

#[derive(Serialize, Deserialize)]
struct Marker {
    name: String,
    dir: String,
    version: String,
}

pub struct Imported {
    pub name: String,
    pub missing: Vec<String>,
}

pub(crate) fn root() -> PathBuf {
    directories::ProjectDirs::from("", "", "yasaff")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("yasaff-data"))
}

fn profiles_dir() -> PathBuf {
    root().join("profiles")
}

fn profile_dir(name: &str) -> PathBuf {
    profiles_dir().join(name)
}

pub fn dir() -> PathBuf {
    ACTIVE.get_or_init(start).1.clone()
}

pub fn active() -> String {
    ACTIVE.get_or_init(start).0.clone()
}

fn start() -> (String, PathBuf) {
    if let Err(e) = migrate_legacy() {
        eprintln!("could not move old data into {DEFAULT}: {e:#}");
    }
    let state: State = fs::read_to_string(root().join(STATE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let name = Some(state.active).filter(|n| valid_name(n).is_ok() && profile_dir(n).is_dir()).or_else(|| list().into_iter().next());
    let name = name.unwrap_or_else(|| DEFAULT.to_owned());
    let dir = profile_dir(&name);
    let _ = fs::create_dir_all(&dir);
    let _ = write_state(&name);
    (name, dir)
}

fn migrate_legacy() -> Result<()> {
    let root = root();
    if profiles_dir().exists() || !root.is_dir() {
        return Ok(());
    }
    let entries: Vec<_> = fs::read_dir(&root)?.flatten().filter(|e| e.file_name() != STATE).collect();
    if entries.is_empty() {
        return Ok(());
    }
    let dest = profile_dir(DEFAULT);
    fs::create_dir_all(&dest)?;
    for entry in entries {
        fs::rename(entry.path(), dest.join(entry.file_name())).with_context(|| format!("moving {}", entry.path().display()))?;
    }
    rewrite_paths(&dest, &root.to_string_lossy())
}

fn write_state(name: &str) -> Result<()> {
    fs::create_dir_all(root())?;
    fs::write(root().join(STATE), serde_json::to_string_pretty(&State { active: name.to_owned() })?)?;
    Ok(())
}

pub fn list() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(profiles_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| valid_name(n).is_ok())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn exists(name: &str) -> bool {
    list().iter().any(|n| n.eq_ignore_ascii_case(name))
}

pub fn valid_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!(tr!("the name is empty"));
    }
    if name != name.trim() || name.starts_with('.') || name.chars().count() > 64 {
        bail!(tr!("\"{name}\" can't be used as a profile name", name = name));
    }
    if name.chars().any(|c| c.is_control() || "/\\:*?\"<>|".contains(c)) {
        bail!(tr!("a profile name can't contain / \\ : * ? \" < > |"));
    }
    Ok(())
}

pub fn unique_name(name: &str) -> String {
    if !exists(name) {
        return name.to_owned();
    }
    let base = match name.rsplit_once('_') {
        Some((base, n)) if !base.is_empty() && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => base,
        _ => name,
    };
    let prefix = format!("{}_", base.to_lowercase());
    let highest = list()
        .iter()
        .filter_map(|n| n.to_lowercase().strip_prefix(&prefix).and_then(|n| n.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("{base}_{}", highest + 1)
}

pub fn switch(name: &str) -> Result<()> {
    ensure!(profile_dir(name).is_dir(), "{name} doesn't exist");
    write_state(name)
}

pub fn create(name: &str) -> Result<()> {
    valid_name(name)?;
    if exists(name) {
        bail!(tr!("a profile named {name} already exists", name = name));
    }
    fs::create_dir_all(profile_dir(name))?;
    Ok(())
}

pub fn rename(new: &str) -> Result<()> {
    valid_name(new)?;
    let old = active();
    if new == old {
        return Ok(());
    }
    if !new.eq_ignore_ascii_case(&old) && exists(new) {
        bail!(tr!("a profile named {name} already exists", name = new));
    }
    let (from, to) = (profile_dir(&old), profile_dir(new));
    fs::rename(&from, &to).with_context(|| format!("renaming {}", from.display()))?;
    rewrite_paths(&to, &from.to_string_lossy())?;
    write_state(new)
}

pub fn reset_current() -> Result<()> {
    let dir = profile_dir(&active());
    remove(&dir)?;
    fs::create_dir_all(&dir)?;
    Ok(())
}

pub fn reset_all() -> Result<()> {
    remove(&profiles_dir())?;
    fs::create_dir_all(profile_dir(DEFAULT))?;
    write_state(DEFAULT)
}

fn remove(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let trash = root().join(format!(".trash-{}", uuid::Uuid::new_v4()));
    fs::rename(dir, &trash).with_context(|| tr!("{folder} is in use; close the apps started from YASAFF and try again", folder = dir.display()))?;
    fs::remove_dir_all(&trash).with_context(|| format!("deleting {}", trash.display()))
}

fn rewrite_paths(dir: &Path, old_dir: &str) -> Result<()> {
    let old = old_dir.trim_end_matches(['/', '\\']);
    if old.is_empty() {
        return Ok(());
    }
    for file in PATH_FILES.map(|f| dir.join(f)) {
        let Ok(text) = fs::read_to_string(&file) else { continue };
        let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        if rewrite_value(&mut json, old, dir) {
            fs::write(&file, serde_json::to_string_pretty(&json)?).with_context(|| format!("writing {}", file.display()))?;
        }
    }
    Ok(())
}

fn rewrite_value(value: &mut serde_json::Value, old: &str, dir: &Path) -> bool {
    match value {
        serde_json::Value::String(s) => {
            let Some(rest) = s.strip_prefix(old).filter(|r| r.starts_with(['/', '\\'])) else { return false };
            let path = rest.split(['/', '\\']).filter(|p| !p.is_empty()).fold(dir.to_path_buf(), |p, part| p.join(part));
            *s = path.to_string_lossy().into_owned();
            true
        }
        serde_json::Value::Array(items) => items.iter_mut().fold(false, |changed, v| rewrite_value(v, old, dir) | changed),
        serde_json::Value::Object(map) => map.values_mut().fold(false, |changed, v| rewrite_value(v, old, dir) | changed),
        _ => false,
    }
}

pub fn export(dest: &Path) -> Result<()> {
    use zip::write::SimpleFileOptions;

    let (name, dir) = (active(), dir());
    let partial = dest.with_extension("zip.partial");
    let written = (|| -> Result<()> {
        let file = fs::File::create(&partial).with_context(|| format!("creating {}", partial.display()))?;
        let mut zip = zip::ZipWriter::new(io::BufWriter::new(file));
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
        let marker = Marker { name, dir: dir.to_string_lossy().into_owned(), version: env!("CARGO_PKG_VERSION").to_owned() };
        zip.start_file(MARKER, options)?;
        serde_json::to_writer_pretty(&mut zip, &marker)?;
        let mut dirs = vec![dir.clone()];
        while let Some(current) = dirs.pop() {
            for entry in fs::read_dir(&current).with_context(|| format!("reading {}", current.display()))?.flatten() {
                let path = entry.path();
                let rel = zip_name(&dir, &path);
                let kind = entry.file_type()?;
                if (current == dir && SKIPPED.contains(&rel.as_str())) || rel.starts_with(".download-") || rel.starts_with(".staging-") {
                    continue;
                }
                if kind.is_dir() {
                    zip.add_directory(&rel, options)?;
                    dirs.push(path);
                } else if kind.is_file() && rel != MARKER {
                    #[cfg(unix)]
                    let options = {
                        use std::os::unix::fs::PermissionsExt;
                        options.unix_permissions(entry.metadata()?.permissions().mode())
                    };
                    zip.start_file(&rel, options)?;
                    io::copy(&mut fs::File::open(&path).with_context(|| format!("reading {}", path.display()))?, &mut zip)?;
                }
            }
        }
        zip.finish()?;
        Ok(())
    })();
    match written {
        Ok(()) => fs::rename(&partial, dest).with_context(|| format!("saving {}", dest.display())),
        Err(e) => {
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn zip_name(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

pub fn import(zip_path: &Path) -> Result<Imported> {
    fs::create_dir_all(profiles_dir())?;
    let staging = profiles_dir().join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        crate::appstore::unpack(zip_path, &staging, MARKER).map_err(|_| anyhow::anyhow!(tr!("this is not an exported YASAFF profile")))?;
        let marker: Marker = serde_json::from_str(&fs::read_to_string(staging.join(MARKER))?).context(tr!("this is not an exported YASAFF profile"))?;
        fs::remove_file(staging.join(MARKER))?;
        let wanted = if valid_name(&marker.name).is_ok() { marker.name.clone() } else { DEFAULT.to_owned() };
        let name = unique_name(&wanted);
        let dir = profile_dir(&name);
        fs::rename(&staging, &dir).with_context(|| format!("creating {}", dir.display()))?;
        rewrite_paths(&dir, &marker.dir)?;
        Ok(Imported { missing: missing(&dir), name })
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

fn missing(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let library: Library = fs::read_to_string(dir.join("library.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    for game in &library.games {
        out.extend(missing_in_game(game));
    }
    let settings: Settings = fs::read_to_string(dir.join("userSettings.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    for store in Store::ALL {
        if settings.store(store).enabled && settings.store_command(store).is_none() {
            out.push(tr!("{store}: not installed", store = store.name()));
        }
    }
    if let Some(backup) = settings.backup_dir.as_ref().filter(|d| !d.is_dir()) {
        out.push(tr!("Save backups folder: {path}", path = backup.display()));
    }
    out
}

fn missing_in_game(game: &Game) -> Vec<String> {
    let mut out = Vec::new();
    let name = &game.name;
    let target = game.target.trim();
    if looks_like_path(target) && !Path::new(target).exists() {
        out.push(tr!("{game}: game file {path}", game = name, path = target));
    }
    for arg in game.args.iter().filter(|a| looks_like_path(a) && !Path::new(a.as_str()).exists()) {
        out.push(tr!("{game}: game file {path}", game = name, path = arg));
    }
    if let Some(dir) = game.working_dir.as_ref().filter(|d| !d.is_dir()) {
        out.push(tr!("{game}: working folder {path}", game = name, path = dir.display()));
    }
    if let Some(dir) = game.save_dir.as_ref().filter(|d| !d.is_dir()) {
        out.push(tr!("{game}: save folder {path}", game = name, path = dir.display()));
    }
    let assets = [&game.cover, &game.background, &game.spine, &game.top, &game.back, &game.side, &game.bottom, &game.booklet, &game.model, &game.video];
    for path in assets.into_iter().flatten().filter(|p| !p.exists()) {
        out.push(tr!("{game}: image or video {path}", game = name, path = path.display()));
    }
    out
}

fn looks_like_path(s: &str) -> bool {
    let b = s.as_bytes();
    let drive = b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
    !crate::launcher::is_uri(s) && (Path::new(s).is_absolute() || s.starts_with('/') || drive)
}

pub fn exe() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok())
}

pub fn restart() -> ! {
    let exe = exe();
    let mut args = std::env::args().skip(1).peekable();
    let mut kept = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--disc" {
            args.next();
        } else {
            kept.push(arg);
        }
    }
    if let Some(exe) = exe
        && let Err(e) = std::process::Command::new(&exe).args(&kept).spawn()
    {
        eprintln!("could not restart {}: {e}", exe.display());
    }
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_paths_under_the_old_folder() {
        let dir = Path::new("/new/profiles/Bob");
        let mut json = serde_json::json!({
            "games": [{ "cover": "C:\\Users\\a\\yasaff\\profiles\\Ann\\assets\\x.png", "target": "C:\\Games\\a.exe" }],
            "cursor": "cursor.png",
            "other": "C:\\Users\\a\\yasaff\\profiles\\Ann2\\x",
        });
        assert!(rewrite_value(&mut json, "C:\\Users\\a\\yasaff\\profiles\\Ann", dir));
        assert_eq!(json["games"][0]["cover"], dir.join("assets").join("x.png").to_string_lossy().as_ref());
        assert_eq!(json["games"][0]["target"], "C:\\Games\\a.exe");
        assert_eq!(json["other"], "C:\\Users\\a\\yasaff\\profiles\\Ann2\\x");
    }

    #[test]
    fn checks_only_paths() {
        assert!(looks_like_path("/games/a.sh"));
        assert!(looks_like_path("D:\\Games\\a.exe"));
        assert!(!looks_like_path("steam://rungameid/570"));
        assert!(!looks_like_path("wine"));
    }

    #[test]
    fn names() {
        assert!(valid_name("DefaultUser_2").is_ok());
        assert!(valid_name("").is_err());
        assert!(valid_name("a/b").is_err());
        assert!(valid_name("..").is_err());
        assert!(valid_name(" x").is_err());
    }
}

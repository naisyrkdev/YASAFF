//! Keeping a game's saves on the USB stick or SD card it was started from.
//!
//! Each game gets a catalog on the device, `YasaffSaves/<game>/`:
//!
//! ```text
//! YasaffSaves/Hollow Knight/
//!     save/                      the newest save: one entry per tracked file or folder
//!         paths.json             where each entry came from on the PC
//!     backup/
//!         2026-10-07 183012/     the newest saves, timestamped; the oldest are pruned
//! ```
//!
//! With a disaster recovery count of N the device holds N + 1 copies: N in `backup` and the
//! current one in `save`. Each copy is written to a `.partial` folder and renamed into place, so
//! a pulled-out device never leaves a half-written save where a whole one used to be.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const ROOT: &str = "YasaffSaves";
const CURRENT: &str = "save";
const BACKUPS: &str = "backup";
const PARTIAL: &str = ".partial";
const INDEX: &str = "paths.json";
pub const MAX_RECOVERY: u32 = 20;

/// A game's choices for its saves on a launch device, kept in the library.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct DeviceSaves {
    /// Copy the saves to the device the game was started from when it closes.
    pub keep_on_device: bool,
    /// Timestamped backups kept next to the current save.
    pub recovery_count: u32,
    /// Save files and folders (folders with everything in them); empty uses the save folder.
    pub paths: Vec<PathBuf>,
}

impl DeviceSaves {
    pub fn is_default(&self) -> bool {
        *self == DeviceSaves::default()
    }
}

/// The game's catalog on the device mounted at `device_root`.
pub fn catalog(device_root: &Path, game_name: &str) -> PathBuf {
    device_root.join(ROOT).join(safe_name(game_name))
}

/// Creates the catalog with its backup folder; an existing one is left as it is.
pub fn create_catalog(catalog: &Path) -> Result<()> {
    fs::create_dir_all(catalog.join(BACKUPS)).with_context(|| format!("creating {}", catalog.display()))
}

fn safe_name(name: &str) -> String {
    let name: String = name.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    let name = name.trim().trim_end_matches('.').to_owned();
    if name.is_empty() { "Game".to_owned() } else { name }
}

#[derive(Serialize, Deserialize)]
struct Index {
    game: String,
    saved: String,
    entries: BTreeMap<String, PathBuf>,
}

/// Copies `paths` into the game's catalog: first a timestamped copy in `backup` (when
/// `recovery_count` > 0), then the current one in `save`; backups beyond `recovery_count` are
/// removed. `action` hears what is being done. Returns the catalog.
pub fn store(game_name: &str, paths: &[PathBuf], catalog: &Path, recovery_count: u32, mut action: impl FnMut(String)) -> Result<PathBuf> {
    let present: Vec<&PathBuf> = paths.iter().filter(|p| p.exists()).collect();
    if present.is_empty() {
        bail!(tr!("none of the save files exist yet"));
    }
    action(tr!("Preparing {folder}", folder = catalog.display()));
    create_catalog(catalog)?;
    let entries = entry_names(&present);
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S").to_string();
    let index = Index { game: game_name.to_owned(), saved: stamp.clone(), entries: entries.iter().map(|(n, p)| (n.clone(), (*p).clone())).collect() };

    let backups = catalog.join(BACKUPS);
    if recovery_count > 0 {
        let mut name = stamp.clone();
        let mut n = 2;
        while backups.join(&name).exists() {
            name = format!("{stamp} ({n})");
            n += 1;
        }
        action(tr!("Backing up to backup/{name}", name = name));
        write_copy(&entries, &index, &backups.join(&name), &mut action)?;
    }
    action(tr!("Writing the current save").to_owned());
    write_copy(&entries, &index, &catalog.join(CURRENT), &mut action)?;
    prune(&backups, recovery_count, &mut action)?;
    sync_dir(catalog);
    Ok(catalog.to_path_buf())
}

/// Names each tracked path gets in the catalog: its own name, made unique.
fn entry_names<'a>(paths: &[&'a PathBuf]) -> Vec<(String, &'a PathBuf)> {
    let mut taken: Vec<String> = Vec::new();
    let mut entries = Vec::new();
    for path in paths {
        let base = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "save".into());
        let base = safe_name(&base);
        let mut name = base.clone();
        let mut n = 2;
        while taken.iter().any(|t| t.eq_ignore_ascii_case(&name)) || name.eq_ignore_ascii_case(INDEX) {
            name = format!("{base} ({n})");
            n += 1;
        }
        taken.push(name.clone());
        entries.push((name, *path));
    }
    entries
}

/// Writes a complete copy to `dest`, replacing what is there only once the copy is whole.
fn write_copy(entries: &[(String, &PathBuf)], index: &Index, dest: &Path, action: &mut impl FnMut(String)) -> Result<()> {
    let partial = with_suffix(dest, PARTIAL);
    remove(&partial)?;
    fs::create_dir_all(&partial).with_context(|| format!("creating {}", partial.display()))?;
    let result = (|| -> Result<()> {
        for (name, source) in entries {
            copy_tree(source, &partial.join(name), action)?;
        }
        let json = serde_json::to_string_pretty(index)?;
        write_synced(&partial.join(INDEX), json.as_bytes())?;
        sync_dir(&partial);
        Ok(())
    })();
    if let Err(e) = result {
        let _ = remove(&partial);
        return Err(e);
    }
    let old = with_suffix(dest, ".old");
    remove(&old)?;
    if dest.exists() {
        fs::rename(dest, &old).with_context(|| format!("replacing {}", dest.display()))?;
    }
    fs::rename(&partial, dest).with_context(|| format!("saving {}", dest.display()))?;
    remove(&old)?;
    sync_dir(dest.parent().unwrap_or(dest));
    Ok(())
}

fn copy_tree(source: &Path, dest: &Path, action: &mut impl FnMut(String)) -> Result<()> {
    let meta = fs::metadata(source).with_context(|| format!("reading {}", source.display()))?;
    if meta.is_file() {
        action(tr!("Copying {file}", file = source.display()));
        let mut from = fs::File::open(source).with_context(|| format!("reading {}", source.display()))?;
        let mut to = fs::File::create(dest).with_context(|| format!("writing {}", dest.display()))?;
        std::io::copy(&mut from, &mut to).with_context(|| format!("copying {}", source.display()))?;
        return to.sync_all().with_context(|| format!("writing {}", dest.display()));
    }
    fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
    let mut children: Vec<_> = fs::read_dir(source).with_context(|| format!("reading {}", source.display()))?.flatten().collect();
    children.sort_by_key(|e| e.file_name());
    for child in children {
        let kind = child.file_type()?;
        let path = child.path();
        // Linked folders are skipped: they can lead outside the save, or back into it.
        if kind.is_dir() || kind.is_file() || (kind.is_symlink() && path.is_file()) {
            copy_tree(&path, &dest.join(child.file_name()), action)?;
        }
    }
    Ok(())
}

/// Writes `data` and waits until it is on the device, so it survives the device being pulled out.
fn write_synced(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = fs::File::create(path).with_context(|| format!("writing {}", path.display()))?;
    file.write_all(data).with_context(|| format!("writing {}", path.display()))?;
    file.sync_all().with_context(|| format!("writing {}", path.display()))
}

fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Keeps the `keep` newest backups (their names are timestamps, so they sort by age).
fn prune(backups: &Path, keep: u32, action: &mut impl FnMut(String)) -> Result<()> {
    let mut found: Vec<PathBuf> = fs::read_dir(backups)
        .with_context(|| format!("reading {}", backups.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    let (partial, mut whole): (Vec<_>, Vec<_>) = found.drain(..).partition(|p| p.to_string_lossy().ends_with(PARTIAL));
    for p in partial {
        remove(&p)?;
    }
    whole.sort();
    let extra = whole.len().saturating_sub(keep as usize);
    for old in whole.into_iter().take(extra) {
        action(tr!("Removing old backup {name}", name = old.file_name().unwrap_or_default().to_string_lossy()));
        remove(&old)?;
    }
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

fn remove(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(_) => return Ok(()),
    }
    .with_context(|| format!("removing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("yasaff-device-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn stores_files_and_nested_folders() {
        let root = tmp("store");
        let pc = root.join("pc");
        fs::create_dir_all(pc.join("Saves/slot1/deep")).unwrap();
        fs::write(pc.join("Saves/slot1/deep/a.sav"), "a").unwrap();
        fs::write(pc.join("settings.ini"), "s").unwrap();
        fs::create_dir_all(pc.join("other")).unwrap();
        fs::write(pc.join("other/settings.ini"), "o").unwrap();
        let paths = vec![pc.join("Saves"), pc.join("settings.ini"), pc.join("other/settings.ini"), pc.join("missing.sav")];

        let catalog = catalog(&root.join("usb"), "Game: Name?");
        assert!(catalog.ends_with("YasaffSaves/Game_ Name_"));
        store("Game", &paths, &catalog, 2, |_| {}).unwrap();
        let save = catalog.join(CURRENT);
        assert_eq!(names(&save), ["Saves", "paths.json", "settings.ini", "settings.ini (2)"]);
        assert_eq!(fs::read_to_string(save.join("Saves/slot1/deep/a.sav")).unwrap(), "a");
        assert_eq!(fs::read_to_string(save.join("settings.ini (2)")).unwrap(), "o");
        let index: Index = serde_json::from_str(&fs::read_to_string(save.join(INDEX)).unwrap()).unwrap();
        assert_eq!(index.entries["Saves"], pc.join("Saves"));
        assert_eq!(names(&catalog.join(BACKUPS)).len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn keeps_count_plus_one_copies() {
        let root = tmp("rotate");
        let save = root.join("save.dat");
        let catalog = root.join("usb").join(ROOT).join("Game");
        fs::create_dir_all(catalog.join(BACKUPS)).unwrap();
        for old in ["2020-01-01 000000", "2021-01-01 000000", "2022-01-01 000000", "2023-01-01 000000.partial"] {
            fs::create_dir_all(catalog.join(BACKUPS).join(old)).unwrap();
        }
        fs::write(&save, "new").unwrap();
        store("Game", std::slice::from_ref(&save), &catalog, 2, |_| {}).unwrap();
        let backups = names(&catalog.join(BACKUPS));
        assert_eq!(backups.len(), 2, "{backups:?}");
        assert_eq!(backups[0], "2022-01-01 000000");
        assert_eq!(fs::read_to_string(catalog.join(BACKUPS).join(&backups[1]).join("save.dat")).unwrap(), "new");
        assert_eq!(fs::read_to_string(catalog.join(CURRENT).join("save.dat")).unwrap(), "new");

        store("Game", std::slice::from_ref(&save), &catalog, 0, |_| {}).unwrap();
        assert!(names(&catalog.join(BACKUPS)).is_empty(), "a count of 0 keeps only the current save");
        assert_eq!(names(&catalog), [BACKUPS, CURRENT]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn nothing_to_store_leaves_the_device_alone() {
        let root = tmp("empty");
        let catalog = root.join("usb").join(ROOT).join("Game");
        assert!(store("Game", &[root.join("nope")], &catalog, 1, |_| {}).is_err());
        assert!(!catalog.exists());
        let _ = fs::remove_dir_all(&root);
    }
}

use crate::appstore::{self, Manifest};
use crate::library::Library;
use crate::theme::Theme;
use anyhow::{Context as _, Result};
use egui::Context;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;

const DIR: &str = "initial_games";
const DONE: &str = "initialContent.json";
const BUNDLED: [(&str, &[u8]); 3] = [
    ("omaro-the-thief.zip", include_bytes!("../initial_content/dist/omaro-the-thief.zip")),
    ("cards-are-gone.zip", include_bytes!("../initial_content/dist/cards-are-gone.zip")),
    ("super-army-3d.zip", include_bytes!("../initial_content/dist/super-army-3d.zip")),
];
const THEMES_DIR: &str = "initial_themes";
const THEMES_DONE: &str = "initialThemes.json";
/// Theme packages, made in their own repositories (see `initial_content/README.md`).
const BUNDLED_THEMES: [(&str, &[u8]); 1] = [("double_axis_theme.zip", include_bytes!("../initial_content/custom_themes/double_axis_theme.zip"))];

pub enum Step {
    Total(usize),
    Installed(Box<Manifest>),
    Failed(String),
}
pub fn wanted(library: &Library) -> bool {
    if marker().exists() {
        return false;
    }
    if !library.games.is_empty() {
        mark_done();
        return false;
    }
    true
}

pub fn mark_done() {
    if let Err(e) = fs::write(marker(), "{ \"imported\": true }\n") {
        log::warn!("could not write {}: {e}", marker().display());
    }
}

fn marker() -> PathBuf {
    crate::profiles::dir().join(DONE)
}

pub fn start(ctx: Context) -> mpsc::Receiver<Step> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let send = |step| {
            let sent = tx.send(step).is_ok();
            ctx.request_repaint();
            sent
        };
        let packages = match packages(DIR, &BUNDLED) {
            Ok(p) => p,
            Err(e) => {
                send(Step::Failed(format!("{e:#}")));
                return;
            }
        };
        send(Step::Total(packages.len()));
        for zip in packages {
            let step = match appstore::install_package(&zip, &[]) {
                Ok(manifest) => Step::Installed(Box::new(manifest)),
                Err(e) => Step::Failed(format!("{}: {e:#}", zip.file_name().unwrap_or_default().to_string_lossy())),
            };
            if !send(step) {
                return;
            }
        }
    });
    rx
}
/// Installs every theme package in `<YASAFF data>/initial_themes/` into a new profile, once. A profile
/// that already has themes is from before bundled themes and keeps the ones it has. Returns warnings.
pub fn install_themes() -> Vec<String> {
    let done = crate::profiles::dir().join(THEMES_DONE);
    if done.exists() {
        return Vec::new();
    }
    let mut warnings = Vec::new();
    let has_themes = fs::read_dir(Theme::themes_dir()).into_iter().flatten().flatten().any(|e| e.path().join("themeSettings.json").is_file());
    if !has_themes {
        match packages(THEMES_DIR, &BUNDLED_THEMES) {
            Ok(zips) => {
                for zip in zips {
                    if let Err(e) = Theme::import(&zip) {
                        let file = zip.file_name().unwrap_or_default().to_string_lossy().into_owned();
                        warnings.push(tr!("Could not install the theme {file}: {error}", file = file, error = format!("{e:#}")));
                    }
                }
            }
            Err(e) => warnings.push(tr!("Could not install the theme {file}: {error}", file = THEMES_DIR, error = format!("{e:#}"))),
        }
    }
    if let Err(e) = fs::create_dir_all(crate::profiles::dir()).and_then(|()| fs::write(&done, "{ \"imported\": true }\n")) {
        log::warn!("could not write {}: {e}", done.display());
    }
    warnings
}

/// Writes the `bundled` zips to `<YASAFF data>/<dir>/` and lists every .zip there.
fn packages(dir: &str, bundled: &[(&str, &[u8])]) -> Result<Vec<PathBuf>> {
    let dir = crate::profiles::root().join(dir);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    for &(name, bytes) in bundled {
        let path = dir.join(name);
        if fs::read(&path).is_ok_and(|on_disk| on_disk == bytes) {
            continue;
        }
        let partial = path.with_extension("zip.partial");
        fs::write(&partial, bytes).with_context(|| format!("writing {}", partial.display()))?;
        fs::rename(&partial, &path).with_context(|| format!("writing {}", path.display()))?;
    }
    let mut zips: Vec<PathBuf> = fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")))
        .collect();
    zips.sort();
    Ok(zips)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_games_install_with_all_their_images() {
        let root = std::env::temp_dir().join(format!("yasaff-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        for (name, bytes) in BUNDLED {
            let zip = root.join(name);
            fs::write(&zip, bytes).unwrap();
            let manifest = appstore::install_package_in(&root, &zip, &[]).unwrap();
            assert_eq!(manifest.kind, "game");
            assert_eq!(manifest.builtin.as_deref(), Some(appstore::MOCK_GAME));
            let dir = root.join("apps").join(&manifest.id);
            let images = manifest.artwork.assets().into_iter().filter_map(|(_, p)| p).chain(&manifest.artwork.logo).chain(&manifest.icon);
            for image in images {
                assert!(dir.join(image).is_file(), "{name}: {image} is missing");
            }
            assert_eq!(manifest.artwork.assets().iter().filter(|(_, p)| p.is_some()).count(), 5, "{name}");
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn bundled_themes_are_theme_packages() {
        let root = std::env::temp_dir().join(format!("yasaff-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        for (name, bytes) in BUNDLED_THEMES {
            let zip = root.join(name);
            fs::write(&zip, bytes).unwrap();
            let dir = root.join(name.trim_end_matches(".zip"));
            appstore::unpack(&zip, &dir, "themeSettings.json").unwrap();
            let settings: crate::theme::ThemeSettings = serde_json::from_str(&fs::read_to_string(dir.join("themeSettings.json")).unwrap()).unwrap();
            if name == "double_axis_theme.zip" {
                assert_eq!(settings.kind, crate::theme::ThemeKind::DoubleAxis);
            }
        }
        fs::remove_dir_all(&root).unwrap();
    }

    /// `dist/*.zip` are build outputs of `initial_content/build.py`; fail when they no longer match
    /// the game folders they are packed from.
    #[test]
    fn bundled_zips_match_their_sources() {
        use std::io::Read;
        let games = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("initial_content/games");
        let mut folders: Vec<String> = fs::read_dir(&games).unwrap().flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        let mut bundled: Vec<String> = BUNDLED.iter().map(|(name, _)| name.trim_end_matches(".zip").to_owned()).collect();
        folders.sort();
        bundled.sort();
        assert_eq!(folders, bundled, "every game folder is bundled; run initial_content/build.py and update BUNDLED");
        for (name, bytes) in BUNDLED {
            let dir = games.join(name.trim_end_matches(".zip"));
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
            let mut packed = Vec::new();
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).unwrap();
                let mut data = Vec::new();
                entry.read_to_end(&mut data).unwrap();
                let path = entry.name().to_owned();
                assert_eq!(fs::read(dir.join(&path)).ok(), Some(data), "{name}: {path} is out of date; run initial_content/build.py");
                packed.push(path);
            }
            let on_disk = fs::read_dir(&dir).unwrap().flatten().filter(|e| e.path().is_file()).count();
            assert_eq!(packed.len(), on_disk, "{name}: files were added to {}; run initial_content/build.py", dir.display());
        }
    }
}

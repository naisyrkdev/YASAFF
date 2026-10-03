use crate::appstore::{self, Manifest};
use crate::library::Library;
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
        eprintln!("could not write {}: {e}", marker().display());
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
        let packages = match packages() {
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
fn packages() -> Result<Vec<PathBuf>> {
    let dir = crate::profiles::root().join(DIR);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    for (name, bytes) in BUNDLED {
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
}

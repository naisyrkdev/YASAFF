
use anyhow::{Context as _, Result, bail};
use egui::Context;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use crate::physical::{ExternalId, ids};

pub const DONE_BADGE: Duration = Duration::from_secs(3);

const STEAM_COVERS: [&str; 3] = ["library_600x900_2x.jpg", "library_600x900.jpg", "library_capsule.jpg"];
const STEAM_HERO: &str = "library_hero.jpg";

pub struct Request {
    pub game_id: String,
    pub ids: Vec<ExternalId>,
}

pub struct Artwork {
    pub game_id: String,
    pub cover: Option<PathBuf>,
    pub background: Option<PathBuf>,
    pub downloads: Vec<PathBuf>,
    pub source: String,
}

#[derive(Clone)]
pub enum Status {
    Queued,
    Fetching,
    Done(Instant, String),
    Failed(String),
}

impl Status {
    pub fn is_busy(&self) -> bool {
        matches!(self, Status::Queued | Status::Fetching)
    }

    pub fn describe(&self) -> String {
        match self {
            Status::Queued => tr!("Waiting to fetch artwork…").into(),
            Status::Fetching => tr!("Looking for artwork…").into(),
            Status::Done(_, source) => tr!("Artwork from {source}", source = source),
            Status::Failed(e) => tr!("Artwork: {error}", error = e),
        }
    }
}

enum Update {
    Started(String),
    Finished(String, Result<Artwork, String>),
}

pub struct Fetcher {
    ctx: Context,
    jobs: Option<mpsc::Sender<Request>>,
    updates_tx: mpsc::Sender<Update>,
    updates: mpsc::Receiver<Update>,
    status: HashMap<String, Status>,
}

impl Fetcher {
    pub fn new(ctx: Context) -> Self {
        let (updates_tx, updates) = mpsc::channel();
        Fetcher { ctx, jobs: None, updates_tx, updates, status: HashMap::new() }
    }

    pub fn status(&self, game_id: &str) -> Option<&Status> {
        self.status.get(game_id)
    }

    pub fn fetch(&mut self, request: Request) {
        if self.status.get(&request.game_id).is_some_and(Status::is_busy) {
            return;
        }
        if request.ids.is_empty() {
            let msg = tr!("not a Steam, Epic, GOG or Amazon game, so there's nowhere to look").to_owned();
            self.status.insert(request.game_id, Status::Failed(msg));
            return;
        }
        self.status.insert(request.game_id.clone(), Status::Queued);
        let jobs = self.jobs.get_or_insert_with(|| spawn_worker(self.ctx.clone(), self.updates_tx.clone()));
        let _ = jobs.send(request);
    }

    pub fn poll(&mut self) -> Vec<Artwork> {
        let mut done = Vec::new();
        while let Ok(update) = self.updates.try_recv() {
            match update {
                Update::Started(id) => {
                    self.status.insert(id, Status::Fetching);
                }
                Update::Finished(id, Ok(art)) => {
                    self.status.insert(id, Status::Done(Instant::now(), art.source.clone()));
                    done.push(art);
                }
                Update::Finished(id, Err(e)) => {
                    log::warn!("artwork for {id}: {e}");
                    self.status.insert(id, Status::Failed(e));
                }
            }
        }
        done
    }
}

fn spawn_worker(ctx: Context, updates: mpsc::Sender<Update>) -> mpsc::Sender<Request> {
    let (tx, jobs) = mpsc::channel::<Request>();
    std::thread::spawn(move || {
        for request in jobs {
            let id = request.game_id.clone();
            let _ = updates.send(Update::Started(id.clone()));
            ctx.request_repaint();
            let result = artwork(&request).map_err(|e| format!("{e:#}"));
            if updates.send(Update::Finished(id, result)).is_err() {
                return;
            }
            ctx.request_repaint();
        }
    });
    tx
}

pub struct Found {
    pub cover: Option<PathBuf>,
    pub background: Option<PathBuf>,
    pub source: &'static str,
}

pub fn local(external: &[ExternalId]) -> Option<Found> {
    external
        .iter()
        .filter_map(|id| match id.store.as_str() {
            ids::STEAM => Some(steam(&id.id)),
            ids::EPIC => heroic(&id.id, "legendary_library.json", "library"),
            ids::GOG => heroic(&id.id, "gog_library.json", "games"),
            ids::AMAZON => heroic(&id.id, "nile_library.json", "library"),
            _ => None,
        })
        .find(|f| f.cover.is_some() || f.background.is_some())
}

fn artwork(request: &Request) -> Result<Artwork> {
    let Some(found) = local(&request.ids) else { bail!(tr!("none on this computer yet; open the game once in its launcher, or set it by hand")) };

    let dir = crate::library::Library::data_dir().join("downloads");
    let mut downloads = Vec::new();
    let mut save = |bytes: Vec<u8>, kind: &str| -> Result<Option<PathBuf>> {
        let ext = image::guess_format(&bytes).ok().and_then(|f| f.extensions_str().first().copied()).unwrap_or("jpg");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}-{kind}.{ext}", request.game_id));
        fs::write(&path, bytes).with_context(|| format!("saving {}", path.display()))?;
        downloads.push(path.clone());
        Ok(Some(path))
    };
    let mut resolve = |image: Option<PathBuf>, kind: &str| -> Result<Option<PathBuf>> {
        match image {
            None => Ok(None),
            Some(path) if path.extension().is_some() => Ok(Some(path)),
            Some(path) => save(fs::read(&path)?, kind),
        }
    };
    let cover = resolve(found.cover, "cover")?;
    let background = resolve(found.background, "background")?;
    let source = tr!("{source} (on this computer)", source = found.source);
    Ok(Artwork { game_id: request.game_id.clone(), cover, background, downloads, source })
}

fn steam(appid: &str) -> Found {
    let dirs: Vec<PathBuf> = crate::scan::steam_roots().into_iter().map(|r| r.join("appcache/librarycache").join(appid)).collect();
    let cover = STEAM_COVERS.iter().find_map(|name| dirs.iter().find_map(|d| find_file(d, name)));
    let background = dirs.iter().find_map(|d| find_file(d, STEAM_HERO));
    Found { cover, background, source: "Steam" }
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    fs::read_dir(dir).ok()?.flatten().map(|e| e.path().join(name)).find(|p| p.is_file())
}

fn heroic(app: &str, file: &str, list_key: &str) -> Option<Found> {
    for config in crate::scan::heroic_config_dirs() {
        let Some(json) = crate::scan::read_json(&config.join("store_cache").join(file)) else { continue };
        let Some(game) = json[list_key].as_array().into_iter().flatten().find(|g| g["app_name"].as_str() == Some(app)) else { continue };
        let cached = |keys: &[&str]| {
            keys.iter()
                .filter_map(|k| game[*k].as_str().filter(|u| u.starts_with("http")))
                .map(|url| config.join("images-cache").join(sha256_hex(url)))
                .find(|p| p.is_file())
        };
        let cover = cached(&["art_square", "art_cover"]);
        let background = cached(&["art_background", "art_cover"]);
        return Some(Found { cover, background, source: "Heroic" });
    }
    None
}

fn sha256_hex(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn hashes_like_heroic() {
        assert_eq!(super::sha256_hex("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}


use crate::apps::AppEntry;
use crate::object3d::ModelUp;
use crate::theme::BoxOverrides;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::physical::{ExternalId, GameIds, ids};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub target: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    #[serde(flatten)]
    pub assets: Assets,
    #[serde(default, skip_serializing_if = "ModelUp::is_default")]
    pub model_up: ModelUp,
    #[serde(default)]
    pub box_style: BoxOverrides,
    #[serde(default)]
    pub last_played: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bubble_place: Option<String>,
}

impl Game {
    pub fn external_ids(&self) -> Vec<ExternalId> {
        let path = Path::new(self.target.trim());
        if path.extension().is_some_and(|e| e == "desktop")
            && let Some(entry) = crate::apps::parse_desktop(path)
        {
            return ids::from_command(&entry.target, &entry.args);
        }
        ids::from_command(&self.target, &self.args)
    }

    pub fn asset(&self, kind: AssetKind) -> Option<&Path> {
        self.assets.paths.get(&kind).map(PathBuf::as_path)
    }

    pub fn placement(&self, kind: AssetKind) -> CoverPlacement {
        self.assets.placements.get(&kind.placement_key()).copied().unwrap_or_default()
    }

    pub fn placement_mut(&mut self, kind: AssetKind) -> &mut CoverPlacement {
        self.assets.placements.entry(kind.placement_key()).or_default()
    }
}

/// A game's artwork files and how each image is placed on its surface.
///
/// Stored in `library.json` as the flat `cover`, `cover_placement`, … fields of earlier versions
/// (see `AssetFields`), so libraries stay readable in both directions.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(from = "AssetFields", into = "AssetFields")]
pub struct Assets {
    paths: BTreeMap<AssetKind, PathBuf>,
    /// Keyed by `AssetKind::placement_key`; a missing entry is the default placement.
    placements: BTreeMap<AssetKind, CoverPlacement>,
}

impl Assets {
    pub fn paths(&self) -> impl Iterator<Item = (AssetKind, &Path)> {
        self.paths.iter().map(|(k, p)| (*k, p.as_path()))
    }
}

#[derive(Serialize, Deserialize, Default)]
struct AssetFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cover: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    spine: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    top: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    back: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    side: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bottom: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    booklet: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    video: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    background: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "CoverPlacement::is_default")]
    cover_placement: CoverPlacement,
    #[serde(default, skip_serializing_if = "CoverPlacement::is_default")]
    spine_placement: CoverPlacement,
    #[serde(default, skip_serializing_if = "CoverPlacement::is_default")]
    top_placement: CoverPlacement,
    #[serde(default, skip_serializing_if = "CoverPlacement::is_default")]
    back_placement: CoverPlacement,
    #[serde(default, skip_serializing_if = "CoverPlacement::is_default")]
    side_placement: CoverPlacement,
    #[serde(default, skip_serializing_if = "CoverPlacement::is_default")]
    bottom_placement: CoverPlacement,
    #[serde(default, skip_serializing_if = "CoverPlacement::is_default")]
    booklet_placement: CoverPlacement,
}

impl AssetFields {
    fn path(&mut self, kind: AssetKind) -> &mut Option<PathBuf> {
        match kind {
            AssetKind::Cover => &mut self.cover,
            AssetKind::Background => &mut self.background,
            AssetKind::Spine => &mut self.spine,
            AssetKind::Top => &mut self.top,
            AssetKind::Back => &mut self.back,
            AssetKind::Side => &mut self.side,
            AssetKind::Bottom => &mut self.bottom,
            AssetKind::Booklet => &mut self.booklet,
            AssetKind::Model => &mut self.model,
            AssetKind::Video => &mut self.video,
        }
    }

    fn placement(&mut self, kind: AssetKind) -> &mut CoverPlacement {
        match kind.placement_key() {
            AssetKind::Spine => &mut self.spine_placement,
            AssetKind::Top => &mut self.top_placement,
            AssetKind::Back => &mut self.back_placement,
            AssetKind::Side => &mut self.side_placement,
            AssetKind::Bottom => &mut self.bottom_placement,
            AssetKind::Booklet => &mut self.booklet_placement,
            _ => &mut self.cover_placement,
        }
    }
}

impl From<AssetFields> for Assets {
    fn from(mut fields: AssetFields) -> Self {
        let mut assets = Assets::default();
        for kind in AssetKind::ALL {
            if let Some(path) = fields.path(kind).take() {
                assets.paths.insert(kind, path);
            }
            let placement = *fields.placement(kind);
            if !placement.is_default() {
                assets.placements.insert(kind.placement_key(), placement);
            }
        }
        assets
    }
}

impl From<Assets> for AssetFields {
    fn from(assets: Assets) -> Self {
        let mut fields = AssetFields::default();
        for (kind, path) in assets.paths {
            *fields.path(kind) = Some(path);
        }
        for (kind, placement) in assets.placements {
            *fields.placement(kind) = placement;
        }
        fields
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct CoverPlacement {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

impl Default for CoverPlacement {
    fn default() -> Self {
        Self { zoom: 1.0, x: 0.5, y: 0.5 }
    }
}

impl CoverPlacement {
    pub const ZOOM_RANGE: (f32, f32) = (0.5, 3.0);

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum AssetKind {
    Cover,
    Background,
    Spine,
    Top,
    Back,
    Side,
    Bottom,
    Booklet,
    Model,
    Video,
}

impl AssetKind {
    pub const ALL: [AssetKind; 10] = [
        AssetKind::Cover,
        AssetKind::Background,
        AssetKind::Spine,
        AssetKind::Top,
        AssetKind::Back,
        AssetKind::Side,
        AssetKind::Bottom,
        AssetKind::Booklet,
        AssetKind::Model,
        AssetKind::Video,
    ];

    /// The surface whose placement this asset uses: the box faces have their own, everything
    /// else (background, model, video) shares the cover's.
    fn placement_key(self) -> AssetKind {
        match self {
            AssetKind::Spine | AssetKind::Top | AssetKind::Back | AssetKind::Side | AssetKind::Bottom | AssetKind::Booklet => self,
            _ => AssetKind::Cover,
        }
    }

    fn slug(self) -> &'static str {
        match self {
            AssetKind::Cover => "cover",
            AssetKind::Background => "background",
            AssetKind::Spine => "spine",
            AssetKind::Top => "top",
            AssetKind::Back => "back",
            AssetKind::Side => "side",
            AssetKind::Bottom => "bottom",
            AssetKind::Booklet => "booklet",
            AssetKind::Model => "model",
            AssetKind::Video => "video",
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Library {
    pub games: Vec<Game>,
    #[serde(skip)]
    data_dir: PathBuf,
}

impl Library {
    pub fn data_dir() -> PathBuf {
        crate::profiles::dir()
    }

    fn file(&self) -> PathBuf {
        self.data_dir.join("library.json")
    }

    fn assets_dir(&self) -> PathBuf {
        self.data_dir.join("assets")
    }

    pub fn load() -> (Self, Option<String>) {
        let data_dir = Self::data_dir();
        let file = data_dir.join("library.json");
        let (mut lib, warning) = match Self::read(&file) {
            Ok(lib) => (lib, None),
            Err(e) => {
                let backup = file.with_extension(format!("json.broken-{}", now_secs()));
                let _ = fs::rename(&file, &backup);
                (Library::default(), Some(format!("{e:#} (moved to {})", backup.display())))
            }
        };
        lib.data_dir = data_dir;
        lib.sort();
        (lib, warning)
    }

    fn read(file: &Path) -> Result<Self> {
        if !file.exists() {
            return Ok(Library::default());
        }
        let text = fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", file.display()))
    }

    pub fn save(&self) -> Result<()> {
        crate::fsutil::write_atomic(&self.file(), serde_json::to_string_pretty(self)?)
    }

    fn sort(&mut self) {
        self.games.sort_by(|a, b| {
            b.last_played
                .cmp(&a.last_played)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }

    pub fn add(&mut self, entry: &AppEntry) -> usize {
        let id = new_id();
        self.games.push(Game {
            id: id.clone(),
            name: entry.name.clone(),
            target: entry.target.clone(),
            args: entry.args.clone(),
            working_dir: entry.working_dir.clone(),
            assets: Assets::default(),
            model_up: ModelUp::default(),
            box_style: BoxOverrides::default(),
            last_played: None,
            title_font: None,
            app_id: None,
            save_dir: None,
            bubble_place: None,
        });
        self.sort();
        self.games.iter().position(|g| g.id == id).unwrap()
    }

    pub fn position(&self, id: &str) -> Option<usize> {
        self.games.iter().position(|g| g.id == id)
    }

    pub fn find(&self, target: &str, args: &[String]) -> Option<usize> {
        self.games.iter().position(|g| g.target == target && g.args == args)
    }

    pub fn disc_ids(&self) -> Vec<GameIds> {
        self.games.iter().map(|g| GameIds { id: g.id.clone(), external: g.external_ids() }).collect()
    }

    pub fn set_asset(&mut self, index: usize, kind: AssetKind, src: &Path) -> Result<()> {
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        let dest = self.new_asset_path(index, kind, &ext)?;
        fs::copy(src, &dest).with_context(|| format!("copying {}", src.display()))?;
        self.replace_asset(index, kind, dest);
        Ok(())
    }

    pub fn set_model(&mut self, index: usize, src: &Path) -> Result<()> {
        let mesh = crate::object3d::import(src).with_context(|| format!("reading {}", src.display()))?;
        let dest = self.new_asset_path(index, AssetKind::Model, "ply")?;
        crate::object3d::formats::write_ply(&mesh, &dest)?;
        self.replace_asset(index, AssetKind::Model, dest);
        Ok(())
    }

    fn new_asset_path(&self, index: usize, kind: AssetKind, ext: &str) -> Result<PathBuf> {
        let assets = self.assets_dir();
        fs::create_dir_all(&assets)?;
        let millis = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        Ok(assets.join(format!("{}-{}-{millis}.{ext}", self.games[index].id, kind.slug())))
    }

    fn replace_asset(&mut self, index: usize, kind: AssetKind, dest: PathBuf) {
        let assets = self.assets_dir();
        if let Some(old) = self.games[index].assets.paths.insert(kind, dest) {
            remove_if_owned(&assets, &old);
        }
    }

    pub fn clear_asset(&mut self, index: usize, kind: AssetKind) {
        let assets = self.assets_dir();
        if let Some(old) = self.games[index].assets.paths.remove(&kind) {
            remove_if_owned(&assets, &old);
        }
    }

    pub fn remove(&mut self, index: usize) {
        let assets = self.assets_dir();
        let game = self.games.remove(index);
        for (_, path) in game.assets.paths() {
            remove_if_owned(&assets, path);
        }
    }

    pub fn mark_played(&mut self, index: usize) -> usize {
        let id = self.games[index].id.clone();
        self.games[index].last_played = Some(now_secs());
        self.sort();
        self.games.iter().position(|g| g.id == id).unwrap_or(0)
    }
}

fn remove_if_owned(assets: &Path, path: &Path) {
    if path.starts_with(assets) {
        let _ = fs::remove_file(path);
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_keep_the_flat_library_format() {
        let json = r#"{"id":"g","name":"G","target":"/g","cover":"/a/c.png","spine":"/a/s.png","video":"/a/v.mp4",
            "cover_placement":{"zoom":2.0,"x":0.1,"y":0.5},"spine_placement":{"zoom":1.5,"x":0.5,"y":0.5}}"#;
        let game: Game = serde_json::from_str(json).unwrap();
        assert_eq!(game.asset(AssetKind::Cover), Some(Path::new("/a/c.png")));
        assert_eq!(game.asset(AssetKind::Video), Some(Path::new("/a/v.mp4")));
        assert_eq!(game.asset(AssetKind::Top), None);
        assert_eq!(game.placement(AssetKind::Cover).zoom, 2.0);
        assert_eq!(game.placement(AssetKind::Background).zoom, 2.0, "the background shares the cover's placement");
        assert_eq!(game.placement(AssetKind::Spine).zoom, 1.5);
        assert!(game.placement(AssetKind::Top).is_default());

        let saved: serde_json::Value = serde_json::to_value(&game).unwrap();
        assert_eq!(saved["cover"], "/a/c.png");
        assert_eq!(saved["spine_placement"]["zoom"], 1.5);
        assert!(saved.get("top").is_none() && saved.get("top_placement").is_none());
        let again: Game = serde_json::from_value(saved).unwrap();
        assert_eq!(again.assets, game.assets);
    }
}

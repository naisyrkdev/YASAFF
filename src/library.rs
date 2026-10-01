
use crate::apps::AppEntry;
use crate::object3d::ModelUp;
use crate::theme::BoxOverrides;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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
    #[serde(default)]
    pub cover: Option<PathBuf>,
    #[serde(default)]
    pub spine: Option<PathBuf>,
    #[serde(default)]
    pub top: Option<PathBuf>,
    #[serde(default)]
    pub back: Option<PathBuf>,
    #[serde(default)]
    pub model: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "ModelUp::is_default")]
    pub model_up: ModelUp,
    #[serde(default)]
    pub box_style: BoxOverrides,
    #[serde(default, skip_serializing_if = "CoverPlacement::is_default")]
    pub cover_placement: CoverPlacement,
    #[serde(default)]
    pub background: Option<PathBuf>,
    #[serde(default)]
    pub last_played: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_dir: Option<PathBuf>,
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

    fn asset_mut(&mut self, kind: AssetKind) -> &mut Option<PathBuf> {
        match kind {
            AssetKind::Cover => &mut self.cover,
            AssetKind::Background => &mut self.background,
            AssetKind::Spine => &mut self.spine,
            AssetKind::Top => &mut self.top,
            AssetKind::Back => &mut self.back,
            AssetKind::Model => &mut self.model,
        }
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

#[derive(Clone, Copy, Debug)]
pub enum AssetKind {
    Cover,
    Background,
    Spine,
    Top,
    Back,
    Model,
}

impl AssetKind {
    fn slug(self) -> &'static str {
        match self {
            AssetKind::Cover => "cover",
            AssetKind::Background => "background",
            AssetKind::Spine => "spine",
            AssetKind::Top => "top",
            AssetKind::Back => "back",
            AssetKind::Model => "model",
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
        directories::ProjectDirs::from("", "", "yasaff")
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("yasaff-data"))
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
        fs::create_dir_all(&self.data_dir)?;
        let tmp = self.file().with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(&tmp, self.file())?;
        Ok(())
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
            cover: None,
            background: None,
            spine: None,
            top: None,
            back: None,
            model: None,
            model_up: ModelUp::default(),
            box_style: BoxOverrides::default(),
            cover_placement: CoverPlacement::default(),
            last_played: None,
            title_font: None,
            app_id: None,
            save_dir: None,
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
        if let Some(old) = self.games[index].asset_mut(kind).replace(dest) {
            remove_if_owned(&assets, &old);
        }
    }

    pub fn clear_asset(&mut self, index: usize, kind: AssetKind) {
        let assets = self.assets_dir();
        if let Some(old) = self.games[index].asset_mut(kind).take() {
            remove_if_owned(&assets, &old);
        }
    }

    pub fn remove(&mut self, index: usize) {
        let assets = self.assets_dir();
        let game = self.games.remove(index);
        for path in [game.cover, game.background, game.spine, game.top, game.back, game.model].into_iter().flatten() {
            remove_if_owned(&assets, &path);
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

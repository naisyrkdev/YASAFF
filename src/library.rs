
use crate::apps::AppEntry;
use crate::device_saves::DeviceSaves;
use crate::object3d::ModelUp;
use crate::theme::{BoxOverrides, CoverShape, TileStyle};
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
    /// The cover placement kept for each tile style, so every tile shape keeps its own crop.
    #[serde(default, skip_serializing_if = "CoverShapes::is_empty")]
    pub cover_shapes: CoverShapes,
    /// The slot this game always takes in the row and grid views, whatever else moves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_slot: Option<usize>,
    /// Keeping the saves on the USB stick or SD card the game is started from.
    #[serde(default, skip_serializing_if = "DeviceSaves::is_default")]
    pub device_saves: DeviceSaves,
}

/// Cover placements per tile style, and cover images per `CoverShape`. The active ones live in
/// `Assets` (what painting reads); this keeps a placement for every style the cover was shown in,
/// and the image of every other shape that has one of its own.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CoverShapes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current: Option<TileStyle>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    saved: BTreeMap<TileStyle, CoverPlacement>,
    /// Never holds the shape of `current`: its image is the `Cover` asset.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    images: BTreeMap<CoverShape, PathBuf>,
}

impl CoverShapes {
    fn is_empty(&self) -> bool {
        self.current.is_none() && self.saved.is_empty() && self.images.is_empty()
    }
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

    /// Whether the game has a cover image, for any tile shape.
    pub fn has_cover(&self) -> bool {
        self.asset(AssetKind::Cover).is_some() || !self.cover_shapes.images.is_empty()
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

    /// Makes the cover image and placement the ones kept for `style`. The placement of the style
    /// shown before is stored first; a style seen for the first time starts as a copy of it, unless
    /// its shape brings a different image. Returns whether anything changed.
    pub fn use_cover_shape(&mut self, style: TileStyle) -> bool {
        let shapes = &mut self.cover_shapes;
        if shapes.current == Some(style) {
            return false;
        }
        let mut active = self.assets.placements.get(&AssetKind::Cover).copied().unwrap_or_default();
        if shapes.is_empty() && active.is_default() {
            return false;
        }
        if let Some(previous) = shapes.current {
            shapes.saved.insert(previous, active);
            let (from, to) = (previous.cover_shape(), style.cover_shape());
            if from != to
                && let Some(image) = shapes.images.remove(&to)
            {
                if let Some(shown) = self.assets.paths.insert(AssetKind::Cover, image) {
                    shapes.images.insert(from, shown);
                }
                active = CoverPlacement::default();
            }
        }
        let next = *shapes.saved.entry(style).or_insert(active);
        shapes.current = Some(style);
        if next.is_default() {
            self.assets.placements.remove(&AssetKind::Cover);
        } else {
            self.assets.placements.insert(AssetKind::Cover, next);
        }
        true
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
    idle_video: Option<PathBuf>,
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
            AssetKind::IdleVideo => &mut self.idle_video,
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
    IdleVideo,
}

impl AssetKind {
    pub const ALL: [AssetKind; 11] = [
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
        AssetKind::IdleVideo,
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
            AssetKind::IdleVideo => "idle-video",
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
        lib.arrange();
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

    /// Puts every game with a fixed slot back on it; the other games fill the free slots in the
    /// order they are in. A slot past the end of the library (games were removed) falls back to the
    /// nearest free one, and the game returns to it once the library is long enough again.
    fn arrange(&mut self) {
        let n = self.games.len();
        let (mut fixed, free): (Vec<Game>, Vec<Game>) = std::mem::take(&mut self.games).into_iter().partition(|g| g.fixed_slot.is_some());
        fixed.sort_by_key(|g| g.fixed_slot);
        let mut slots: Vec<Option<Game>> = (0..n).map(|_| None).collect();
        for game in fixed {
            let want = game.fixed_slot.unwrap_or(0).min(n - 1);
            let slot = (want..n).chain((0..want).rev()).find(|&j| slots[j].is_none()).unwrap_or(want);
            slots[slot] = Some(game);
        }
        let mut free = free.into_iter();
        self.games = slots.into_iter().filter_map(|slot| slot.or_else(|| free.next())).collect();
    }

    /// Makes the game at `index` keep its current slot, or lets it move again.
    pub fn set_fixed(&mut self, index: usize, fixed: bool) {
        if fixed {
            for game in self.games.iter_mut().filter(|g| g.fixed_slot == Some(index)) {
                game.fixed_slot = None;
            }
        }
        self.games[index].fixed_slot = fixed.then_some(index);
    }

    /// Swaps two games' slots. A game with a fixed slot keeps the one it was moved to.
    pub fn swap(&mut self, a: usize, b: usize) {
        self.games.swap(a, b);
        for i in [a, b] {
            if self.games[i].fixed_slot.is_some() {
                self.games[i].fixed_slot = Some(i);
            }
        }
        self.arrange();
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
            cover_shapes: CoverShapes::default(),
            fixed_slot: None,
            device_saves: DeviceSaves::default(),
        });
        self.arrange();
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
        let dest = self.copy_asset(index, kind.slug(), src)?;
        self.replace_asset(index, kind, dest);
        Ok(())
    }

    /// Gives the game a cover image for each listed shape, `style` being the tile style shown
    /// now: its shape's image becomes the cover, the others are kept until their shape is shown.
    pub fn set_shape_covers(&mut self, index: usize, style: TileStyle, covers: &[(CoverShape, PathBuf)]) -> Result<()> {
        let shown = style.cover_shape();
        if let Some((_, src)) = covers.iter().find(|(shape, _)| *shape == shown) {
            self.set_asset(index, AssetKind::Cover, src)?;
        }
        for (shape, src) in covers.iter().filter(|(shape, _)| *shape != shown) {
            let dest = self.copy_asset(index, &format!("cover-{}", shape.slug()), src)?;
            if let Some(old) = self.games[index].cover_shapes.images.insert(*shape, dest) {
                remove_if_owned(&self.assets_dir(), &old);
            }
        }
        let shapes = &mut self.games[index].cover_shapes;
        if shapes.current != Some(style) {
            shapes.saved.clear();
            shapes.current = Some(style);
        }
        Ok(())
    }

    fn copy_asset(&self, index: usize, slug: &str, src: &Path) -> Result<PathBuf> {
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        let dest = self.new_asset_path(index, slug, &ext)?;
        fs::copy(src, &dest).with_context(|| format!("copying {}", src.display()))?;
        Ok(dest)
    }

    pub fn set_model(&mut self, index: usize, src: &Path) -> Result<()> {
        let mesh = crate::object3d::import(src).with_context(|| format!("reading {}", src.display()))?;
        let dest = self.new_asset_path(index, AssetKind::Model.slug(), "ply")?;
        crate::object3d::formats::write_ply(&mesh, &dest)?;
        self.replace_asset(index, AssetKind::Model, dest);
        Ok(())
    }

    fn new_asset_path(&self, index: usize, slug: &str, ext: &str) -> Result<PathBuf> {
        let assets = self.assets_dir();
        fs::create_dir_all(&assets)?;
        let millis = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        // A file set within the same millisecond must not overwrite (and then delete) the one it replaces.
        let path = |n: u128| assets.join(format!("{}-{slug}-{n}.{ext}", self.games[index].id));
        Ok((millis..).map(path).find(|p| !p.exists()).unwrap())
    }

    fn replace_asset(&mut self, index: usize, kind: AssetKind, dest: PathBuf) {
        let assets = self.assets_dir();
        let game = &mut self.games[index];
        if kind == AssetKind::Cover {
            // A new image starts centered, and the crops made for the old one are dropped. Shapes
            // with an image of their own keep it and their crops.
            game.assets.placements.remove(&AssetKind::Cover);
            let CoverShapes { saved, images, .. } = &mut game.cover_shapes;
            if images.is_empty() {
                game.cover_shapes = CoverShapes::default();
            } else {
                saved.retain(|style, _| images.contains_key(&style.cover_shape()));
            }
        }
        if let Some(old) = game.assets.paths.insert(kind, dest) {
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
        for path in game.assets.paths().map(|(_, p)| p).chain(game.cover_shapes.images.values().map(PathBuf::as_path)) {
            remove_if_owned(&assets, path);
        }
        self.arrange();
    }

    /// Records that the game was started. With `to_front` it moves to the first slot no game is
    /// fixed to; a game with a fixed slot stays where it is. Returns the game's new index.
    pub fn mark_played(&mut self, index: usize, to_front: bool) -> usize {
        let id = self.games[index].id.clone();
        self.games[index].last_played = Some(now_secs());
        if to_front && self.games[index].fixed_slot.is_none() {
            let game = self.games.remove(index);
            self.games.insert(0, game);
            self.arrange();
        }
        self.position(&id).unwrap_or(0)
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
    fn each_tile_style_keeps_its_own_cover_placement() {
        let mut game: Game = serde_json::from_str(r#"{"id":"g","name":"G","target":"/g"}"#).unwrap();
        assert!(!game.use_cover_shape(TileStyle::Flat), "an untouched cover needs nothing stored");
        let square = CoverPlacement { zoom: 2.0, x: 0.2, y: 0.5 };
        *game.placement_mut(AssetKind::Cover) = square;
        assert!(game.use_cover_shape(TileStyle::Flat));

        assert!(game.use_cover_shape(TileStyle::Cover));
        assert_eq!(game.placement(AssetKind::Cover), square, "a new style starts as a copy of the previous one");
        let rect = CoverPlacement { zoom: 1.5, x: 0.5, y: 0.8 };
        *game.placement_mut(AssetKind::Cover) = rect;

        game.use_cover_shape(TileStyle::Flat);
        assert_eq!(game.placement(AssetKind::Cover), square, "going back restores that style's placement");
        game.use_cover_shape(TileStyle::Cover);
        assert_eq!(game.placement(AssetKind::Cover), rect);

        let json = serde_json::to_string(&game).unwrap();
        let mut reloaded: Game = serde_json::from_str(&json).unwrap();
        reloaded.use_cover_shape(TileStyle::Flat);
        assert_eq!(reloaded.placement(AssetKind::Cover), square);
    }

    #[test]
    fn each_cover_shape_keeps_its_own_image() {
        let root = std::env::temp_dir().join(format!("yasaff-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file = |name: &str| {
            let path = root.join(name);
            fs::write(&path, name).unwrap();
            path
        };
        let (square, tall, picked) = (file("square.png"), file("tall.png"), file("picked.png"));
        let mut lib = library(&["g"]);
        lib.data_dir = root.join("data");
        let shown = |lib: &Library| fs::read_to_string(lib.games[0].asset(AssetKind::Cover).unwrap()).unwrap();

        lib.set_shape_covers(0, TileStyle::Flat, &[(CoverShape::Square, square), (CoverShape::Tall, tall)]).unwrap();
        assert_eq!(shown(&lib), "square.png", "the shape shown when importing gets its image");
        assert!(!lib.games[0].use_cover_shape(TileStyle::Flat));
        assert!(lib.games[0].use_cover_shape(TileStyle::Cover));
        assert_eq!(shown(&lib), "tall.png");
        lib.games[0].use_cover_shape(TileStyle::Model);
        assert_eq!(shown(&lib), "tall.png", "boxes share the tall image");
        let crop = CoverPlacement { zoom: 2.0, x: 0.3, y: 0.5 };
        *lib.games[0].placement_mut(AssetKind::Cover) = crop;

        lib.games[0].use_cover_shape(TileStyle::Disc);
        assert_eq!(shown(&lib), "square.png");
        assert!(lib.games[0].placement(AssetKind::Cover).is_default(), "the tall crop doesn't carry over to the square image");
        lib.set_asset(0, AssetKind::Cover, &picked).unwrap();
        assert_eq!(shown(&lib), "picked.png");

        let json = serde_json::to_string(&lib.games[0]).unwrap();
        lib.games[0] = serde_json::from_str(&json).unwrap();
        lib.games[0].use_cover_shape(TileStyle::Model);
        assert_eq!(shown(&lib), "tall.png", "picking a square cover leaves the tall one alone");
        assert_eq!(lib.games[0].placement(AssetKind::Cover), crop);
        lib.games[0].use_cover_shape(TileStyle::Flat);
        assert_eq!(shown(&lib), "picked.png", "and is kept for the square tiles");

        lib.remove(0);
        assert_eq!(fs::read_dir(root.join("data/assets")).unwrap().count(), 0, "removing the game deletes every shape's image");
        fs::remove_dir_all(&root).unwrap();
    }

    fn library(names: &[&str]) -> Library {
        let games = names.iter().map(|n| serde_json::from_str(&format!(r#"{{"id":"{n}","name":"{n}","target":"/{n}"}}"#)).unwrap()).collect();
        Library { games, data_dir: PathBuf::new() }
    }

    fn order(lib: &Library) -> String {
        lib.games.iter().map(|g| g.id.as_str()).collect()
    }

    #[test]
    fn played_games_move_to_the_front_around_fixed_slots() {
        let mut lib = library(&["a", "b", "c", "d", "e"]);
        lib.set_fixed(0, true);
        lib.set_fixed(2, true);
        assert_eq!(lib.mark_played(4, true), 1, "slot 0 is fixed, so the first free slot is 1");
        assert_eq!(order(&lib), "aecbd");
        assert_eq!(lib.mark_played(2, true), 2, "a game with a fixed slot stays put");
        assert_eq!(lib.mark_played(3, false), 3, "without to_front nothing moves");
        assert_eq!(order(&lib), "aecbd");
    }

    #[test]
    fn fixed_slots_survive_removals_and_swaps() {
        let mut lib = library(&["a", "b", "c", "d", "e"]);
        lib.set_fixed(2, true);
        lib.games.remove(0);
        lib.arrange();
        assert_eq!(order(&lib), "bdce", "c keeps slot 2 when a game before it goes");
        lib.games.truncate(2);
        lib.arrange();
        assert_eq!(order(&lib), "bd", "c was removed with the rest");

        let mut lib = library(&["a", "b", "c", "d"]);
        lib.set_fixed(3, true);
        lib.games.remove(0);
        lib.arrange();
        assert_eq!(order(&lib), "bcd", "a slot past the end falls back to the last one");
        lib.games.push(library(&["x"]).games.remove(0));
        lib.arrange();
        assert_eq!(order(&lib), "bcxd", "and returns once there is room again");

        let mut lib = library(&["a", "b", "c", "d"]);
        lib.set_fixed(1, true);
        lib.swap(1, 3);
        assert_eq!(order(&lib), "adcb", "a swapped game takes its new slot");
        assert_eq!(lib.games[3].fixed_slot, Some(3));
        assert_eq!(lib.games[1].fixed_slot, None);

        let json = serde_json::to_string(&lib.games[3]).unwrap();
        let reloaded: Game = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.fixed_slot, Some(3));
    }

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

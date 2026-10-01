
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
    /// Bubble placement saved by older versions; read once and replaced by `bubble_slot`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bubble_place: Option<String>,
    /// The bubble this game sits in, so adding or removing games leaves it where it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bubble_slot: Option<usize>,
    /// The cover image and placement kept for each tile style.
    #[serde(default, skip_serializing_if = "CoverShapes::is_empty")]
    pub cover_shapes: CoverShapes,
    /// Every image file the game keeps, used or not, so it can be picked again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gallery: Vec<PathBuf>,
    /// The slot this game always takes in the row and grid views, whatever else moves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_slot: Option<usize>,
    /// Keeping the saves on the USB stick or SD card the game is started from.
    #[serde(default, skip_serializing_if = "DeviceSaves::is_default")]
    pub device_saves: DeviceSaves,
    /// Names from `Library::categories`. Custom themes may group games by them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// One of `categories`; set whenever the game has any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_category: Option<String>,
    /// The theme picture was started from the cover once; it is not copied again after that.
    #[serde(default, alias = "axis_seeded", skip_serializing_if = "std::ops::Not::not")]
    pub picture_seeded: bool,
}

/// The cover image and placement of every tile style. The shown style's live in `Assets` (what
/// painting reads); this keeps them for the others, so each style has a cover of its own.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CoverShapes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current: Option<TileStyle>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    saved: BTreeMap<TileStyle, CoverPlacement>,
    /// Never holds `current`: its image is the `Cover` asset.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    styles: BTreeMap<TileStyle, PathBuf>,
    /// Images per shape, from before each style had its own; moved into `styles` when loaded.
    #[serde(default, skip_serializing)]
    images: BTreeMap<CoverShape, PathBuf>,
}

impl CoverShapes {
    fn is_empty(&self) -> bool {
        self.current.is_none() && self.saved.is_empty() && self.styles.is_empty()
    }

    fn shows(&self, style: TileStyle) -> bool {
        self.current.is_none_or(|c| c == style)
    }

    /// Gives every style of an image's shape that image, unless the style has its own.
    fn migrate(&mut self) {
        for (shape, image) in std::mem::take(&mut self.images) {
            for style in TileStyle::ALL.into_iter().filter(|s| s.cover_shape() == shape) {
                if !self.shows(style) {
                    self.styles.entry(style).or_insert_with(|| image.clone());
                }
            }
        }
    }
}

/// Where a game shows an image.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageSlot {
    /// The cover a tile style shows.
    Cover(TileStyle),
    /// Any other image: background, box faces, booklet. `AssetKind::Cover` is the shown style's cover.
    Asset(AssetKind),
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

    /// Whether the game has a cover image, for any tile style.
    pub fn has_cover(&self) -> bool {
        self.asset(AssetKind::Cover).is_some() || !self.cover_shapes.styles.is_empty()
    }

    /// The cover image `style` shows; `None` for a style that has none of its own yet.
    pub fn style_cover(&self, style: TileStyle) -> Option<&Path> {
        if self.cover_shapes.shows(style) {
            return self.asset(AssetKind::Cover);
        }
        self.cover_shapes.styles.get(&style).map(PathBuf::as_path)
    }

    pub fn style_placement(&self, style: TileStyle) -> CoverPlacement {
        if self.cover_shapes.shows(style) {
            return self.placement(AssetKind::Cover);
        }
        self.cover_shapes.saved.get(&style).copied().unwrap_or_default()
    }

    pub fn set_style_placement(&mut self, style: TileStyle, placement: CoverPlacement) {
        if self.cover_shapes.shows(style) {
            *self.placement_mut(AssetKind::Cover) = placement;
        } else {
            self.cover_shapes.saved.insert(style, placement);
        }
    }

    /// The image in `slot`.
    pub fn image(&self, slot: ImageSlot) -> Option<&Path> {
        match slot {
            ImageSlot::Cover(style) => self.style_cover(style),
            ImageSlot::Asset(kind) => self.asset(kind),
        }
    }

    /// Every place the image at `path` is shown, covers first.
    pub fn image_uses(&self, path: &Path) -> Vec<ImageSlot> {
        let covers = TileStyle::ALL.into_iter().filter(|s| s.shows_cover() && self.style_cover(*s) == Some(path)).map(ImageSlot::Cover);
        let others = AssetKind::IMAGES.into_iter().filter(|k| *k != AssetKind::Cover && self.asset(*k) == Some(path)).map(ImageSlot::Asset);
        covers.chain(others).collect()
    }

    fn uses_image(&self, path: &Path) -> bool {
        self.assets.paths.values().chain(self.cover_shapes.styles.values()).any(|p| p == path)
    }

    /// Puts every image the game shows in its gallery, for libraries from before it had one.
    fn adopt_images(&mut self) {
        self.cover_shapes.migrate();
        let shown = self.assets.paths.iter().filter(|(k, _)| k.is_image()).map(|(_, p)| p);
        let images: Vec<PathBuf> = shown.chain(self.cover_shapes.styles.values()).cloned().collect();
        for image in images {
            if !self.gallery.contains(&image) {
                self.gallery.push(image);
            }
        }
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

    /// Makes the cover image and placement the ones kept for `style`. The style shown before keeps
    /// its own; a style with no cover of its own starts as a copy of it. Returns whether anything
    /// worth saving changed.
    pub fn use_cover_shape(&mut self, style: TileStyle) -> bool {
        let shapes = &mut self.cover_shapes;
        if shapes.current == Some(style) {
            return false;
        }
        let image = self.assets.paths.get(&AssetKind::Cover).cloned();
        let placement = self.assets.placements.get(&AssetKind::Cover).copied().unwrap_or_default();
        let Some(previous) = shapes.current.replace(style) else {
            return image.is_some() || !placement.is_default();
        };
        match &image {
            Some(image) => shapes.styles.insert(previous, image.clone()),
            None => shapes.styles.remove(&previous),
        };
        shapes.saved.insert(previous, placement);
        let kept = shapes.saved.remove(&style);
        let (image, placement) = match shapes.styles.remove(&style) {
            Some(own) => (Some(own), kept.unwrap_or_default()),
            None => (image, kept.unwrap_or(placement)),
        };
        match image {
            Some(image) => self.assets.paths.insert(AssetKind::Cover, image),
            None => self.assets.paths.remove(&AssetKind::Cover),
        };
        if placement.is_default() {
            self.assets.placements.remove(&AssetKind::Cover);
        } else {
            self.assets.placements.insert(AssetKind::Cover, placement);
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
    #[serde(default, alias = "axis", skip_serializing_if = "Option::is_none")]
    theme_picture: Option<PathBuf>,
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
    #[serde(default, alias = "axis_placement", skip_serializing_if = "CoverPlacement::is_default")]
    theme_picture_placement: CoverPlacement,
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
            AssetKind::ThemePicture => &mut self.theme_picture,
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
            AssetKind::ThemePicture => &mut self.theme_picture_placement,
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
    /// The game's picture in a custom theme that asks for one.
    ThemePicture,
}

impl AssetKind {
    /// The kinds that are pictures, and so go in the gallery.
    pub const IMAGES: [AssetKind; 9] = [
        AssetKind::Cover,
        AssetKind::Background,
        AssetKind::Spine,
        AssetKind::Top,
        AssetKind::Back,
        AssetKind::Side,
        AssetKind::Bottom,
        AssetKind::Booklet,
        AssetKind::ThemePicture,
    ];

    pub fn is_image(self) -> bool {
        Self::IMAGES.contains(&self)
    }

    pub const ALL: [AssetKind; 12] = [
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
        AssetKind::ThemePicture,
    ];

    /// The surface whose placement this asset uses: the box faces and the theme picture have
    /// their own, everything else (background, model, video) shares the cover's.
    fn placement_key(self) -> AssetKind {
        match self {
            AssetKind::Spine | AssetKind::Top | AssetKind::Back | AssetKind::Side | AssetKind::Bottom | AssetKind::Booklet | AssetKind::ThemePicture => self,
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
            AssetKind::ThemePicture => "theme-picture",
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Library {
    pub games: Vec<Game>,
    /// Category names, in the order custom themes show them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(skip)]
    data_dir: PathBuf,
    /// `library.json` could be neither read nor moved aside; saving would destroy it.
    #[serde(skip)]
    keep_file: bool,
    /// A replaced image leaves the gallery (and the disk) when nothing else shows it, instead of
    /// being kept for the user to manage.
    #[serde(skip)]
    pub prune_gallery: bool,
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
                match fs::rename(&file, &backup) {
                    Ok(()) => (Library::default(), Some(format!("{e:#} (moved to {})", backup.display()))),
                    Err(_) => (Library { keep_file: true, ..Library::default() }, Some(format!("{e:#} (left as it is; changes won't be saved)"))),
                }
            }
        };
        lib.data_dir = data_dir;
        lib.arrange();
        lib.tidy_categories();
        for game in &mut lib.games {
            game.adopt_images();
        }
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
        anyhow::ensure!(!self.keep_file, "{} could not be read at startup and is left as it is", self.file().display());
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
            bubble_slot: None,
            cover_shapes: CoverShapes::default(),
            gallery: Vec::new(),
            fixed_slot: None,
            device_saves: DeviceSaves::default(),
            categories: Vec::new(),
            main_category: None,
            picture_seeded: false,
        });
        self.arrange();
        self.games.iter().position(|g| g.id == id).unwrap()
    }

    /// Drops category names the library doesn't have, and makes sure a game with categories has a
    /// main one among them.
    fn tidy_categories(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.categories.retain(|c| !c.trim().is_empty() && seen.insert(c.to_lowercase()));
        let known = self.categories.clone();
        for game in &mut self.games {
            game.categories.retain(|c| known.contains(c));
            game.categories.dedup();
            if game.main_category.as_ref().is_none_or(|m| !game.categories.contains(m)) {
                game.main_category = game.categories.first().cloned();
            }
        }
    }

    /// The category named `name`, ignoring case.
    pub fn category(&self, name: &str) -> Option<usize> {
        let name = name.trim().to_lowercase();
        self.categories.iter().position(|c| c.to_lowercase() == name)
    }

    pub fn add_category(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "a category needs a name");
        anyhow::ensure!(self.category(name).is_none(), "there is already a category named {name}");
        self.categories.push(name.to_owned());
        Ok(())
    }

    pub fn rename_category(&mut self, index: usize, name: &str) -> Result<()> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "a category needs a name");
        anyhow::ensure!(self.category(name).is_none_or(|i| i == index), "there is already a category named {name}");
        let old = std::mem::replace(&mut self.categories[index], name.to_owned());
        for game in &mut self.games {
            for c in game.categories.iter_mut().chain(game.main_category.as_mut()).filter(|c| **c == old) {
                *c = name.to_owned();
            }
        }
        Ok(())
    }

    pub fn remove_category(&mut self, index: usize) {
        self.categories.remove(index);
        self.tidy_categories();
    }

    /// Moves a category one place to the left (`-1`) or right (`1`).
    pub fn move_category(&mut self, index: usize, delta: i32) -> usize {
        let to = (index as i32 + delta).clamp(0, self.categories.len() as i32 - 1) as usize;
        self.categories.swap(index, to);
        to
    }

    /// Puts the game in the category or takes it out. The first category becomes the main one,
    /// and taking out the main one makes the next the main one.
    pub fn toggle_category(&mut self, game: usize, category: usize) {
        let name = self.categories[category].clone();
        let game = &mut self.games[game];
        match game.categories.iter().position(|c| *c == name) {
            Some(i) => {
                game.categories.remove(i);
            }
            None => game.categories.push(name),
        }
        let known = &self.categories;
        game.categories.sort_by_key(|c| known.iter().position(|k| k == c));
        if game.main_category.as_ref().is_none_or(|m| !game.categories.contains(m)) {
            game.main_category = game.categories.first().cloned();
        }
    }

    /// Makes the category the game's main one, adding the game to it.
    pub fn set_main_category(&mut self, game: usize, category: usize) {
        let name = self.categories[category].clone();
        if !self.games[game].categories.contains(&name) {
            self.toggle_category(game, category);
        }
        self.games[game].main_category = Some(name);
    }

    /// Starts a game's theme picture as a copy of its square cover, or else its tall cover. Done
    /// once per game, so removing the picture leaves it without one. Returns whether anything changed.
    pub fn seed_theme_picture(&mut self, index: usize) -> bool {
        let game = &self.games[index];
        if game.picture_seeded || game.asset(AssetKind::ThemePicture).is_some() {
            return false;
        }
        let Some(src) = [TileStyle::Flat, TileStyle::Cover].into_iter().find_map(|s| game.style_cover(s)).map(Path::to_path_buf) else {
            return false;
        };
        match self.copy_asset(index, AssetKind::ThemePicture.slug(), &src) {
            Ok(dest) => self.set_image(index, ImageSlot::Asset(AssetKind::ThemePicture), dest),
            Err(e) => log::warn!("could not copy {} for the theme picture: {e:#}", src.display()),
        }
        self.games[index].picture_seeded = true;
        true
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
        self.import_image(index, ImageSlot::Asset(kind), src)
    }

    /// Copies `src` into the game's files and shows it in `slot`. Images also join the gallery.
    pub fn import_image(&mut self, index: usize, slot: ImageSlot, src: &Path) -> Result<()> {
        let slug = match slot {
            ImageSlot::Cover(_) => AssetKind::Cover.slug(),
            ImageSlot::Asset(kind) => kind.slug(),
        };
        let dest = self.copy_asset(index, slug, src)?;
        self.set_image(index, slot, dest);
        Ok(())
    }

    /// Shows `image`, a file the game already has or was just given, in `slot`. A new cover starts
    /// centered. The image it replaces stays in the gallery, unless `prune_gallery` is on and
    /// nothing else shows it.
    pub fn set_image(&mut self, index: usize, slot: ImageSlot, image: PathBuf) {
        let game = &mut self.games[index];
        let is_image = match slot {
            ImageSlot::Cover(_) => true,
            ImageSlot::Asset(kind) => kind.is_image(),
        };
        if is_image && !game.gallery.contains(&image) {
            game.gallery.push(image.clone());
        }
        let old = match slot {
            ImageSlot::Cover(style) if !game.cover_shapes.shows(style) => {
                game.cover_shapes.saved.remove(&style);
                game.cover_shapes.styles.insert(style, image)
            }
            ImageSlot::Cover(_) | ImageSlot::Asset(AssetKind::Cover) => {
                game.assets.placements.remove(&AssetKind::Cover);
                game.assets.paths.insert(AssetKind::Cover, image)
            }
            ImageSlot::Asset(kind) => game.assets.paths.insert(kind, image),
        };
        if let Some(old) = old {
            self.release(index, &old);
        }
    }

    /// Gives the game a cover image for each listed shape, `style` being the tile style shown
    /// now: every style of a shape gets that shape's image.
    pub fn set_shape_covers(&mut self, index: usize, style: TileStyle, covers: &[(CoverShape, PathBuf)]) -> Result<()> {
        self.games[index].use_cover_shape(style);
        for (shape, src) in covers {
            let dest = self.copy_asset(index, &format!("cover-{}", shape.slug()), src)?;
            for each in TileStyle::ALL.into_iter().filter(|s| s.cover_shape() == *shape) {
                self.set_image(index, ImageSlot::Cover(each), dest.clone());
            }
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
        self.set_image(index, ImageSlot::Asset(AssetKind::Model), dest);
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

    /// Called for a file the game no longer shows in one place. Images stay in the gallery unless
    /// `prune_gallery` is on; models and videos are deleted, they have no gallery.
    fn release(&mut self, index: usize, old: &Path) {
        let assets = self.assets_dir();
        let game = &mut self.games[index];
        if game.uses_image(old) || (!self.prune_gallery && game.gallery.iter().any(|p| p == old)) {
            return;
        }
        game.gallery.retain(|p| p != old);
        remove_if_owned(&assets, old);
    }

    pub fn clear_asset(&mut self, index: usize, kind: AssetKind) {
        if let Some(old) = self.games[index].assets.paths.remove(&kind) {
            self.release(index, &old);
        }
    }

    /// Deletes a gallery image nothing shows. Returns false, keeping it, when something does.
    pub fn remove_from_gallery(&mut self, index: usize, image: &Path) -> bool {
        let assets = self.assets_dir();
        let game = &mut self.games[index];
        if game.uses_image(image) {
            return false;
        }
        game.gallery.retain(|p| p != image);
        remove_if_owned(&assets, image);
        true
    }

    pub fn remove(&mut self, index: usize) {
        let assets = self.assets_dir();
        let game = self.games.remove(index);
        let shown = game.assets.paths().map(|(_, p)| p).chain(game.cover_shapes.styles.values().map(PathBuf::as_path));
        for path in shown.chain(game.gallery.iter().map(PathBuf::as_path)) {
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

    fn temp_library() -> (PathBuf, Library, impl Fn(&str) -> PathBuf) {
        let root = std::env::temp_dir().join(format!("yasaff-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let mut lib = library(&["g"]);
        lib.data_dir = root.join("data");
        let dir = root.clone();
        let file = move |name: &str| {
            let path = dir.join(name);
            fs::write(&path, name).unwrap();
            path
        };
        (root, lib, file)
    }

    fn shown(lib: &Library) -> String {
        fs::read_to_string(lib.games[0].asset(AssetKind::Cover).unwrap()).unwrap()
    }

    #[test]
    fn each_tile_style_keeps_its_own_cover() {
        let (root, mut lib, file) = temp_library();
        assert!(!lib.games[0].use_cover_shape(TileStyle::Flat), "an untouched cover needs nothing stored");
        lib.set_asset(0, AssetKind::Cover, &file("square.png")).unwrap();
        let square = CoverPlacement { zoom: 2.0, x: 0.2, y: 0.5 };
        *lib.games[0].placement_mut(AssetKind::Cover) = square;

        assert!(lib.games[0].use_cover_shape(TileStyle::Cover));
        assert_eq!(shown(&lib), "square.png", "a style without a cover of its own copies the one shown before");
        assert_eq!(lib.games[0].placement(AssetKind::Cover), square);
        let rect = CoverPlacement { zoom: 1.5, x: 0.5, y: 0.8 };
        *lib.games[0].placement_mut(AssetKind::Cover) = rect;

        lib.games[0].use_cover_shape(TileStyle::Flat);
        assert_eq!(lib.games[0].placement(AssetKind::Cover), square, "going back restores that style's placement");
        lib.set_asset(0, AssetKind::Cover, &file("new-square.png")).unwrap();
        assert_eq!(shown(&lib), "new-square.png");

        lib.games[0].use_cover_shape(TileStyle::Cover);
        assert_eq!(shown(&lib), "square.png", "a new square image leaves the Game covers style alone");
        assert_eq!(lib.games[0].placement(AssetKind::Cover), rect);

        let json = serde_json::to_string(&lib.games[0]).unwrap();
        lib.games[0] = serde_json::from_str(&json).unwrap();
        lib.games[0].use_cover_shape(TileStyle::Flat);
        assert_eq!(shown(&lib), "new-square.png");
        assert!(lib.games[0].placement(AssetKind::Cover).is_default(), "a new image starts centered");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_hidden_style_gets_its_own_cover() {
        let (root, mut lib, file) = temp_library();
        lib.games[0].use_cover_shape(TileStyle::Flat);
        lib.set_asset(0, AssetKind::Cover, &file("square.png")).unwrap();
        lib.import_image(0, ImageSlot::Cover(TileStyle::Disc), &file("disc.png")).unwrap();
        assert_eq!(shown(&lib), "square.png", "the shown style keeps its cover");
        let game = &lib.games[0];
        assert_eq!(fs::read_to_string(game.style_cover(TileStyle::Disc).unwrap()).unwrap(), "disc.png");
        assert_eq!(game.style_cover(TileStyle::Crt), None, "a style never set has no cover of its own");
        lib.games[0].use_cover_shape(TileStyle::Disc);
        assert_eq!(shown(&lib), "disc.png");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn store_covers_go_to_every_style_of_their_shape() {
        let (root, mut lib, file) = temp_library();
        lib.set_shape_covers(0, TileStyle::Flat, &[(CoverShape::Square, file("square.png")), (CoverShape::Tall, file("tall.png"))]).unwrap();
        assert_eq!(shown(&lib), "square.png", "the shape shown when importing gets its image");
        lib.games[0].use_cover_shape(TileStyle::Model);
        assert_eq!(shown(&lib), "tall.png");
        lib.games[0].use_cover_shape(TileStyle::Case);
        assert_eq!(shown(&lib), "tall.png");
        lib.games[0].use_cover_shape(TileStyle::Disc);
        assert_eq!(shown(&lib), "square.png");
        assert_eq!(lib.games[0].gallery.len(), 2);
        lib.remove(0);
        assert_eq!(fs::read_dir(root.join("data/assets")).unwrap().count(), 0, "removing the game deletes every image");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_gallery_keeps_replaced_images_unless_pruned() {
        let (root, mut lib, file) = temp_library();
        lib.games[0].use_cover_shape(TileStyle::Flat);
        lib.set_asset(0, AssetKind::Cover, &file("one.png")).unwrap();
        lib.set_asset(0, AssetKind::Cover, &file("two.png")).unwrap();
        assert_eq!(lib.games[0].gallery.len(), 2, "kept for picking again");
        let one = lib.games[0].gallery[0].clone();
        assert!(lib.games[0].image_uses(&one).is_empty());
        lib.set_image(0, ImageSlot::Asset(AssetKind::Background), one.clone());
        assert_eq!(lib.games[0].image_uses(&one), [ImageSlot::Asset(AssetKind::Background)]);
        assert!(!lib.remove_from_gallery(0, &one), "an image in use stays");
        lib.clear_asset(0, AssetKind::Background);
        assert!(one.exists(), "clearing keeps it in the gallery");
        assert!(lib.remove_from_gallery(0, &one));
        assert!(!one.exists());

        lib.prune_gallery = true;
        let two = lib.games[0].asset(AssetKind::Cover).unwrap().to_path_buf();
        lib.games[0].use_cover_shape(TileStyle::Cover);
        lib.set_asset(0, AssetKind::Cover, &file("three.png")).unwrap();
        assert!(two.exists(), "the square cards still show it");
        lib.games[0].use_cover_shape(TileStyle::Flat);
        lib.set_asset(0, AssetKind::Cover, &file("four.png")).unwrap();
        assert!(!two.exists(), "pruned once nothing shows it");
        assert_eq!(lib.games[0].gallery.len(), 2);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn shape_images_of_old_libraries_move_to_their_styles() {
        let json = r#"{"id":"g","name":"G","target":"/g","cover":"/a/square.png",
            "cover_shapes":{"current":"flat","images":{"tall":"/a/tall.png"}}}"#;
        let mut game: Game = serde_json::from_str(json).unwrap();
        game.adopt_images();
        assert_eq!(game.style_cover(TileStyle::Model), Some(Path::new("/a/tall.png")));
        assert_eq!(game.style_cover(TileStyle::Disc), None, "square styles take the shown cover when switched to");
        assert_eq!(game.gallery, [PathBuf::from("/a/square.png"), PathBuf::from("/a/tall.png")]);
    }

    #[test]
    fn an_unreadable_library_is_never_overwritten() {
        let root = std::env::temp_dir().join(format!("yasaff-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("library.json"), "the user's library").unwrap();
        let lib = Library { data_dir: root.clone(), keep_file: true, ..library(&["a"]) };
        assert!(lib.save().is_err());
        assert_eq!(fs::read_to_string(root.join("library.json")).unwrap(), "the user's library");
        fs::remove_dir_all(&root).unwrap();
    }

    fn library(names: &[&str]) -> Library {
        let games = names.iter().map(|n| serde_json::from_str(&format!(r#"{{"id":"{n}","name":"{n}","target":"/{n}"}}"#)).unwrap()).collect();
        Library { games, ..Library::default() }
    }

    #[test]
    fn a_game_with_categories_always_has_a_main_one() {
        let mut lib = library(&["a"]);
        for name in ["Racing", "Kids"] {
            lib.add_category(name).unwrap();
        }
        assert!(lib.add_category("racing").is_err());
        let main = |lib: &Library| lib.games[0].main_category.clone();
        lib.toggle_category(0, 1);
        assert_eq!(main(&lib).as_deref(), Some("Kids"));
        lib.set_main_category(0, 0);
        assert_eq!((lib.games[0].categories.clone(), main(&lib).as_deref()), (vec!["Racing".to_owned(), "Kids".to_owned()], Some("Racing")));
        lib.rename_category(0, "Cars").unwrap();
        assert_eq!(main(&lib).as_deref(), Some("Cars"));
        lib.remove_category(0);
        assert_eq!((lib.games[0].categories.clone(), main(&lib).as_deref()), (vec!["Kids".to_owned()], Some("Kids")));
        lib.toggle_category(0, 0);
        assert_eq!((lib.games[0].categories.len(), main(&lib)), (0, None));
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


use crate::library::Library;
use anyhow::{Context, Result, bail};
use egui::Color32;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SETTINGS_FILE: &str = "themeSettings.json";
const IMAGES_DIR: &str = "images";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum ViewKind {
    #[default]
    Row,
    Grid,
}

impl ViewKind {
    pub const ALL: [Self; 2] = [Self::Row, Self::Grid];

    pub fn label(self) -> &'static str {
        match self {
            Self::Row => tr!("Row view"),
            Self::Grid => tr!("Grid view"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum TileStyle {
    #[default]
    Flat,
    Cover,
    #[serde(rename = "box")]
    FlatBox,
    Model,
    Object,
    Disc,
}

impl TileStyle {
    pub const ALL: [Self; 6] = [Self::Flat, Self::Cover, Self::FlatBox, Self::Model, Self::Object, Self::Disc];

    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => tr!("Square cards"),
            Self::Cover => tr!("Game covers"),
            Self::FlatBox => tr!("Flat 3D boxes"),
            Self::Model => tr!("3D model boxes"),
            Self::Object => tr!("3D objects"),
            Self::Disc => tr!("3D discs"),
        }
    }

    pub fn is_box(self) -> bool {
        matches!(self, Self::FlatBox | Self::Model)
    }

    pub fn aspect(self, [w, h, _]: [f32; 3]) -> f32 {
        match self {
            Self::Cover => w / h,
            _ => 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum BoxCase {
    BluRay,
    Dvd,
    BigBox,
    Custom,
}

impl BoxCase {
    pub const ALL: [Self; 4] = [Self::BluRay, Self::Dvd, Self::BigBox, Self::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Self::BluRay => tr!("Size 1"),
            Self::Dvd => tr!("Size 2"),
            Self::BigBox => tr!("Size 3"),
            Self::Custom => tr!("Custom size"),
        }
    }
}

pub const BOX_WIDTH_RANGE: (f32, f32) = (50.0, 400.0);
pub const BOX_HEIGHT_RANGE: (f32, f32) = (50.0, 400.0);
pub const BOX_DEPTH_RANGE: (f32, f32) = (5.0, 100.0);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum SpineLabel {
    Name,
    None,
}

impl SpineLabel {
    pub const ALL: [Self; 2] = [Self::Name, Self::None];

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => tr!("Game name"),
            Self::None => tr!("Nothing"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct BoxSettings {
    pub case: BoxCase,
    pub custom_size: [f32; 3],
    pub spine_label: SpineLabel,
    #[serde(with = "hex")]
    pub spine_color: Color32,
    #[serde(with = "hex")]
    pub spine_text_color: Color32,
    #[serde(with = "hex")]
    pub top_color: Color32,
}

impl Default for BoxSettings {
    fn default() -> Self {
        Self {
            case: BoxCase::BluRay,
            custom_size: [135.0, 190.0, 14.0],
            spine_label: SpineLabel::Name,
            spine_color: Color32::from_rgb(0x14, 0x14, 0x16),
            spine_text_color: Color32::WHITE,
            top_color: Color32::from_rgb(0x1f, 0x1f, 0x22),
        }
    }
}

impl BoxSettings {
    pub fn size_mm(&self) -> [f32; 3] {
        match self.case {
            BoxCase::BluRay => [135.0, 170.0, 14.0],
            BoxCase::Dvd => [135.0, 190.0, 14.0],
            BoxCase::BigBox => [190.0, 240.0, 45.0],
            BoxCase::Custom => {
                let [w, h, d] = self.custom_size;
                let clamp = |v: f32, (min, max): (f32, f32)| if v.is_finite() { v.clamp(min, max) } else { min };
                [clamp(w, BOX_WIDTH_RANGE), clamp(h, BOX_HEIGHT_RANGE), clamp(d, BOX_DEPTH_RANGE)]
            }
        }
    }

    pub fn with(&self, o: &BoxOverrides) -> BoxSettings {
        BoxSettings {
            case: self.case,
            custom_size: self.custom_size,
            spine_label: o.spine_label.unwrap_or(self.spine_label),
            spine_color: o.spine_color.unwrap_or(self.spine_color),
            spine_text_color: o.spine_text_color.unwrap_or(self.spine_text_color),
            top_color: o.top_color.unwrap_or(self.top_color),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ObjectSettings {
    #[serde(with = "hex")]
    pub color: Color32,
}

impl Default for ObjectSettings {
    fn default() -> Self {
        Self { color: Color32::from_rgb(0xd4, 0xda, 0xe6) }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum DiscKind {
    Cd,
    Dvd,
    BluRay,
}

impl DiscKind {
    pub const ALL: [Self; 3] = [Self::Cd, Self::Dvd, Self::BluRay];

    pub fn label(self) -> &'static str {
        match self {
            Self::Cd => "CD",
            Self::Dvd => "DVD",
            Self::BluRay => "Blu-ray",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum DiscSpin {
    Around,
    Flip,
    Flat,
}

impl DiscSpin {
    pub const ALL: [Self; 3] = [Self::Around, Self::Flip, Self::Flat];

    pub fn label(self) -> &'static str {
        match self {
            Self::Around => tr!("Around its center"),
            Self::Flip => tr!("Flipping over"),
            Self::Flat => tr!("Flat, like in a drive"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct DiscSettings {
    pub kind: DiscKind,
    pub angle: f32,
    pub spin: bool,
    pub spin_direction: DiscSpin,
    pub spin_speed: f32,
}

impl DiscSettings {
    pub const ANGLE_RANGE: (f32, f32) = (0.0, 85.0);
    pub const SPEED_RANGE: (f32, f32) = (1.0, 500.0);
    pub const FLAT_ANGLE: f32 = 70.0;

    pub fn lying_flat(&self) -> bool {
        self.spin && self.spin_direction == DiscSpin::Flat
    }

    pub fn shown_angle(&self) -> f32 {
        if self.lying_flat() { Self::FLAT_ANGLE } else { self.angle }
    }
}

impl Default for DiscSettings {
    fn default() -> Self {
        Self { kind: DiscKind::Dvd, angle: 0.0, spin: true, spin_direction: DiscSpin::Flip, spin_speed: 6.0 }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct BoxOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spine_label: Option<SpineLabel>,
    #[serde(with = "hex_opt", skip_serializing_if = "Option::is_none")]
    pub spine_color: Option<Color32>,
    #[serde(with = "hex_opt", skip_serializing_if = "Option::is_none")]
    pub spine_text_color: Option<Color32>,
    #[serde(with = "hex_opt", skip_serializing_if = "Option::is_none")]
    pub top_color: Option<Color32>,
    #[serde(with = "hex_opt", skip_serializing_if = "Option::is_none")]
    pub object_color: Option<Color32>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum GridAlign {
    Left,
    Center,
}

impl GridAlign {
    pub const ALL: [Self; 2] = [Self::Left, Self::Center];

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => tr!("Left"),
            Self::Center => tr!("Centered"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    Top,
    Middle,
    Bottom,
}

impl BarPosition {
    pub const ALL: [Self; 3] = [Self::Top, Self::Middle, Self::Bottom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => tr!("Top"),
            Self::Middle => tr!("Middle"),
            Self::Bottom => tr!("Bottom"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundKind {
    Color,
    Gradient,
    Image,
}

impl BackgroundKind {
    pub const ALL: [Self; 3] = [Self::Color, Self::Gradient, Self::Image];

    pub fn label(self) -> &'static str {
        match self {
            Self::Color => tr!("Solid color"),
            Self::Gradient => tr!("Gradient"),
            Self::Image => tr!("Image"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum GradientDirection {
    Vertical,
    Horizontal,
    Diagonal,
}

impl GradientDirection {
    pub const ALL: [Self; 3] = [Self::Vertical, Self::Horizontal, Self::Diagonal];

    pub fn label(self) -> &'static str {
        match self {
            Self::Vertical => tr!("Top to bottom"),
            Self::Horizontal => tr!("Left to right"),
            Self::Diagonal => tr!("Diagonal"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ImageFit {
    Fill,
    Fit,
    Stretch,
    Tile,
    Mirror,
    Center,
}

impl ImageFit {
    pub const ALL: [Self; 6] = [Self::Fill, Self::Fit, Self::Stretch, Self::Tile, Self::Mirror, Self::Center];

    pub fn label(self) -> &'static str {
        match self {
            Self::Fill => tr!("Extend (fill, crop)"),
            Self::Fit => tr!("Fit (whole image)"),
            Self::Stretch => tr!("Stretch"),
            Self::Tile => tr!("Repeat"),
            Self::Mirror => tr!("Repeat mirrored"),
            Self::Center => tr!("Center"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct TileSettings {
    pub rows: u32,
    pub size: f32,
    pub spacing: f32,
    pub enlarge_selected: bool,
    pub selected_scale: f32,
    pub bar_position: BarPosition,
}

impl Default for TileSettings {
    fn default() -> Self {
        Self { rows: 1, size: 180.0, spacing: 18.0, enlarge_selected: true, selected_scale: 1.65, bar_position: BarPosition::Top }
    }
}

impl TileSettings {
    pub const ROWS_RANGE: (u32, u32) = (1, 4);
    pub const SIZE_RANGE: (f32, f32) = (80.0, 400.0);
    pub const SPACING_RANGE: (f32, f32) = (0.0, 120.0);
    pub const SCALE_RANGE: (f32, f32) = (1.05, 2.5);
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct GridSettings {
    pub columns: u32,
    pub rows: u32,
    pub spacing: f32,
    pub enlarge_selected: bool,
    pub align: GridAlign,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self { columns: 4, rows: 3, spacing: 24.0, enlarge_selected: true, align: GridAlign::Center }
    }
}

impl GridSettings {
    pub const COLUMNS_RANGE: (u32, u32) = (2, 8);
    pub const ROWS_RANGE: (u32, u32) = (1, 5);
    pub const SPACING_RANGE: (f32, f32) = (0.0, 120.0);
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Background {
    pub game_art: bool,
    pub kind: BackgroundKind,
    #[serde(with = "hex")]
    pub color: Color32,
    #[serde(with = "hex")]
    pub gradient_from: Color32,
    #[serde(with = "hex")]
    pub gradient_to: Color32,
    pub gradient_direction: GradientDirection,
    pub image: Option<String>,
    pub image_fit: ImageFit,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            game_art: true,
            kind: BackgroundKind::Gradient,
            color: Color32::from_rgb(0x06, 0x12, 0x2c),
            gradient_from: Color32::from_rgb(0x07, 0x1b, 0x42),
            gradient_to: Color32::from_rgb(0x00, 0x02, 0x07),
            gradient_direction: GradientDirection::Vertical,
            image: None,
            image_fit: ImageFit::Fill,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StoreOrigin {
    pub id: String,
    pub version: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ThemeSettings {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<StoreOrigin>,
    pub view: ViewKind,
    pub tiles: TileSettings,
    pub grid: GridSettings,
    pub tile_style: TileStyle,
    pub boxes: BoxSettings,
    pub objects: ObjectSettings,
    pub discs: DiscSettings,
    pub background: Background,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            name: "Default".to_owned(),
            store: None,
            view: ViewKind::default(),
            tiles: TileSettings::default(),
            grid: GridSettings::default(),
            tile_style: TileStyle::default(),
            boxes: BoxSettings::default(),
            objects: ObjectSettings::default(),
            discs: DiscSettings::default(),
            background: Background::default(),
        }
    }
}

pub struct Theme {
    pub id: String,
    pub dir: PathBuf,
    pub settings: ThemeSettings,
}

impl Theme {
    pub fn themes_dir() -> PathBuf {
        Library::data_dir().join("themes")
    }

    pub fn load_all() -> (Vec<Theme>, Vec<String>) {
        let mut themes = Vec::new();
        let mut warnings = Vec::new();
        for entry in fs::read_dir(Self::themes_dir()).into_iter().flatten().flatten() {
            let dir = entry.path();
            if !dir.join(SETTINGS_FILE).is_file() {
                continue;
            }
            match read_settings(&dir) {
                Ok(settings) => {
                    let id = entry.file_name().to_string_lossy().into_owned();
                    themes.push(Theme { id, dir, settings });
                }
                Err(e) => warnings.push(tr!("Skipping theme: {error}", error = format!("{e:#}"))),
            }
        }
        if themes.is_empty() {
            match Theme::create(ThemeSettings::default()) {
                Ok(theme) => themes.push(theme),
                Err(e) => {
                    warnings.push(tr!("Could not create the default theme: {error}", error = format!("{e:#}")));
                    let id = uuid::Uuid::new_v4().to_string();
                    themes.push(Theme { dir: Self::themes_dir().join(&id), id, settings: ThemeSettings::default() });
                }
            }
        }
        sort(&mut themes);
        (themes, warnings)
    }

    pub fn create(settings: ThemeSettings) -> Result<Theme> {
        let id = uuid::Uuid::new_v4().to_string();
        let theme = Theme { dir: Self::themes_dir().join(&id), id, settings };
        fs::create_dir_all(theme.images_dir())?;
        theme.save()?;
        Ok(theme)
    }

    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(self.images_dir())?;
        let file = self.dir.join(SETTINGS_FILE);
        let tmp = file.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&self.settings)?)?;
        fs::rename(&tmp, &file)?;
        Ok(())
    }

    pub fn images_dir(&self) -> PathBuf {
        self.dir.join(IMAGES_DIR)
    }

    pub fn background_image(&self) -> Option<PathBuf> {
        let name = self.settings.background.image.as_deref()?;
        is_plain_file_name(name).then(|| self.images_dir().join(name))
    }

    pub fn set_background_image(&mut self, src: &Path) -> Result<()> {
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        let name = format!("background-{}.{ext}", now_secs());
        fs::create_dir_all(self.images_dir())?;
        fs::copy(src, self.images_dir().join(&name)).with_context(|| format!("copying {}", src.display()))?;
        if let Some(old) = self.background_image() {
            let _ = fs::remove_file(old);
        }
        self.settings.background.image = Some(name);
        self.settings.background.kind = BackgroundKind::Image;
        self.save()
    }

    pub fn duplicate(&self, name: String) -> Result<Theme> {
        let theme = Theme::create(ThemeSettings { name, store: None, ..self.settings.clone() })?;
        copy_images(&self.images_dir(), &theme.images_dir())?;
        Ok(theme)
    }

    pub fn delete(&self) -> Result<()> {
        if self.dir.parent() != Some(Self::themes_dir().as_path()) {
            bail!("{} is not a theme folder", self.dir.display());
        }
        fs::remove_dir_all(&self.dir).with_context(|| format!("deleting {}", self.dir.display()))
    }

    pub fn export(&self, parent: &Path) -> Result<PathBuf> {
        let base = safe_folder_name(&self.settings.name);
        let mut dest = parent.join(&base);
        for n in 2.. {
            if !dest.exists() {
                break;
            }
            dest = parent.join(format!("{base} ({n})"));
        }
        fs::create_dir_all(dest.join(IMAGES_DIR))?;
        fs::write(dest.join(SETTINGS_FILE), serde_json::to_string_pretty(&self.settings)?)?;
        copy_images(&self.images_dir(), &dest.join(IMAGES_DIR))?;
        Ok(dest)
    }

    pub fn import(src: &Path) -> Result<Theme> {
        if !src.join(SETTINGS_FILE).is_file() {
            bail!("{} doesn't contain a {SETTINGS_FILE}", src.display());
        }
        let settings = ThemeSettings { store: None, ..read_settings(src)? };
        let theme = Theme::create(settings)?;
        copy_images(&src.join(IMAGES_DIR), &theme.images_dir())?;
        Ok(theme)
    }

    pub fn install_from_store(src: &Path, origin: StoreOrigin) -> Result<Theme> {
        let mut settings = read_settings(src)?;
        settings.store = Some(origin);
        let theme = Theme::create(settings)?;
        copy_images(&src.join(IMAGES_DIR), &theme.images_dir())?;
        Ok(theme)
    }

    pub fn update_from_store(&mut self, src: &Path, origin: StoreOrigin) -> Result<()> {
        let mut settings = read_settings(src)?;
        settings.store = Some(origin);
        let fresh = self.dir.join(format!("{IMAGES_DIR}.new"));
        let _ = fs::remove_dir_all(&fresh);
        copy_images(&src.join(IMAGES_DIR), &fresh)?;
        fs::create_dir_all(&fresh)?;
        let old = self.dir.join(format!("{IMAGES_DIR}.old"));
        let _ = fs::remove_dir_all(&old);
        if self.images_dir().exists() {
            fs::rename(self.images_dir(), &old)?;
        }
        fs::rename(&fresh, self.images_dir())?;
        let _ = fs::remove_dir_all(&old);
        self.settings = settings;
        self.save()
    }
}

pub fn sort(themes: &mut [Theme]) {
    themes.sort_by_key(|t| t.settings.name.to_lowercase());
}

fn read_settings(dir: &Path) -> Result<ThemeSettings> {
    let file = dir.join(SETTINGS_FILE);
    let text = fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", file.display()))
}

fn copy_images(src: &Path, dest: &Path) -> Result<()> {
    let Ok(entries) = fs::read_dir(src) else { return Ok(()) };
    fs::create_dir_all(dest)?;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            fs::copy(entry.path(), dest.join(entry.file_name()))
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}

fn safe_folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_owned();
    if cleaned.is_empty() { "Theme".to_owned() } else { cleaned }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn to_hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
}

pub fn parse_hex(text: &str) -> Option<Color32> {
    let hex = text.trim().trim_start_matches('#');
    if !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        3 => {
            let v: Vec<u8> = hex.chars().map(|c| c.to_digit(16).map(|d| d as u8 * 17)).collect::<Option<_>>()?;
            Some(Color32::from_rgb(v[0], v[1], v[2]))
        }
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

mod hex {
    use egui::Color32;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(c: &Color32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::to_hex(*c))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Color32, D::Error> {
        let text = String::deserialize(d)?;
        super::parse_hex(&text).ok_or_else(|| D::Error::custom(format!("invalid color {text:?}")))
    }
}

mod hex_opt {
    use egui::Color32;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(c: &Option<Color32>, s: S) -> Result<S::Ok, S::Error> {
        match c {
            Some(c) => s.serialize_some(&super::to_hex(*c)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Color32>, D::Error> {
        match Option::<String>::deserialize(d)? {
            None => Ok(None),
            Some(text) => super::parse_hex(&text).map(Some).ok_or_else(|| D::Error::custom(format!("invalid color {text:?}"))),
        }
    }
}

pub const COLOR_PRESETS: &[Color32] = &[
    Color32::from_rgb(0x00, 0x00, 0x00),
    Color32::from_rgb(0x00, 0x02, 0x07),
    Color32::from_rgb(0x06, 0x12, 0x2c),
    Color32::from_rgb(0x07, 0x1b, 0x42),
    Color32::from_rgb(0x0b, 0x2a, 0x66),
    Color32::from_rgb(0x1b, 0x1f, 0x2a),
    Color32::from_rgb(0x2a, 0x2d, 0x34),
    Color32::from_rgb(0x2b, 0x10, 0x4a),
    Color32::from_rgb(0x4a, 0x10, 0x3c),
    Color32::from_rgb(0x5a, 0x0e, 0x16),
    Color32::from_rgb(0x4a, 0x26, 0x0a),
    Color32::from_rgb(0x0e, 0x3b, 0x1f),
    Color32::from_rgb(0x06, 0x3a, 0x40),
    Color32::from_rgb(0x1f, 0x5f, 0x8b),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        for c in [Color32::from_rgb(0x0b, 0x2a, 0x66), Color32::from_rgba_unmultiplied(1, 2, 3, 128)] {
            assert_eq!(parse_hex(&to_hex(c)), Some(c));
        }
        assert_eq!(parse_hex("#fff"), Some(Color32::WHITE));
        assert_eq!(parse_hex("12345"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
    }

    #[test]
    fn settings_json_uses_defaults_for_missing_fields() {
        let s: ThemeSettings = serde_json::from_str(r##"{"name":"X","background":{"kind":"color","color":"#102030"}}"##).unwrap();
        assert_eq!(s.name, "X");
        assert_eq!(s.background.kind, BackgroundKind::Color);
        assert_eq!(s.background.color, Color32::from_rgb(0x10, 0x20, 0x30));
        assert_eq!(s.tiles.size, TileSettings::default().size);
        assert_eq!(s.view, ViewKind::Row);
        assert_eq!(s.tiles.rows, 1);
        assert_eq!(s.grid, GridSettings::default());
    }

    #[test]
    fn flat_box_style_keeps_its_saved_name() {
        let s: ThemeSettings = serde_json::from_str(r#"{"tile_style":"box"}"#).unwrap();
        assert_eq!(s.tile_style, TileStyle::FlatBox);
    }

    #[test]
    fn disc_style_reads_with_default_disc_settings() {
        let s: ThemeSettings = serde_json::from_str(r#"{"tile_style":"disc","discs":{"kind":"bluray","angle":30}}"#).unwrap();
        assert_eq!(s.tile_style, TileStyle::Disc);
        assert_eq!(s.discs.kind, DiscKind::BluRay);
        assert_eq!(s.discs.angle, 30.0);
        assert_eq!(s.discs.spin_speed, DiscSettings::default().spin_speed);
    }

    #[test]
    fn box_overrides_round_trip() {
        let o = BoxOverrides { spine_color: Some(Color32::from_rgb(0x5a, 0x0e, 0x16)), ..Default::default() };
        let json = serde_json::to_string(&o).unwrap();
        assert_eq!(json, r##"{"spine_color":"#5a0e16"}"##);
        assert_eq!(serde_json::from_str::<BoxOverrides>(&json).unwrap(), o);
        let look = BoxSettings::default().with(&o);
        assert_eq!(look.spine_color, Color32::from_rgb(0x5a, 0x0e, 0x16));
        assert_eq!(look.top_color, BoxSettings::default().top_color);
    }

    #[test]
    fn store_origin_is_kept_only_when_set() {
        let plain = serde_json::to_string(&ThemeSettings::default()).unwrap();
        assert!(!plain.contains("\"store\""));
        let s: ThemeSettings = serde_json::from_str(r#"{"name":"N","store":{"id":"neon-nights","version":2}}"#).unwrap();
        assert_eq!(s.store, Some(StoreOrigin { id: "neon-nights".into(), version: 2 }));
    }

    #[test]
    fn image_names_stay_inside_the_theme() {
        assert!(is_plain_file_name("background-1.png"));
        assert!(!is_plain_file_name("../secret.png"));
        assert!(!is_plain_file_name("C:\\x.png"));
        assert!(!is_plain_file_name(".."));
    }
}

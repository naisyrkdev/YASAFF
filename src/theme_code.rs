//! A custom theme's code: `theme.wasm` in the theme's folder, a WebAssembly module that lays out
//! the home screen. It runs sandboxed in an interpreter. It gets no imports, so it can't reach
//! files, the network or anything else, only the JSON YASAFF hands it. See `docs/THEMES.md`.
//!
//! The module exports:
//! - `memory`;
//! - `yasaff_alloc(len: i32) -> i32`: room for `len` bytes, where YASAFF writes its input;
//! - `yasaff_describe(ptr: i32, len: i32) -> i64`: takes `{"language": …}` as JSON, gives back the
//!   theme's [`Manifest`] as JSON, its labels in that language if it has them;
//! - `yasaff_frame(ptr: i32, len: i32) -> i64`: takes a [`FrameIn`] as JSON, gives back a
//!   [`FrameOut`] as JSON.
//!
//! An `i64` result is the JSON's address in the upper 32 bits and its length in the lower 32.

use crate::theme::parse_hex;
use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use wasmi::{Config, Engine, Instance, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc};

pub const CODE_FILE: &str = "theme.wasm";
/// Instructions one call may run before it is stopped, so a theme stuck in a loop can't hang YASAFF.
const FUEL: u64 = 400_000_000;
const MAX_MEMORY: usize = 256 * 1024 * 1024;
const MAX_CODE: u64 = 32 * 1024 * 1024;

/// What a theme says about itself: the options it shows in Settings and the pictures it uses.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Manifest {
    pub options: Vec<OptionSpec>,
    /// A picture of each game, picked in its Options; none when the theme doesn't use one.
    pub game_picture: Option<GamePicture>,
}

impl Manifest {
    pub fn option(&self, key: &str) -> Option<&OptionSpec> {
        self.options.iter().find(|o| o.key == key)
    }

    /// The theme's pictures page, from its `pictures` option.
    pub fn pictures(&self) -> Option<&Pictures> {
        self.options.iter().find_map(|o| match &o.kind {
            OptionKind::Pictures(p) => Some(p),
            _ => None,
        })
    }

    /// Every option's value, the defaults filling in for missing or invalid ones; what the theme
    /// gets as `options`.
    pub fn values(&self, saved: &BTreeMap<String, Value>, fonts: &[String]) -> serde_json::Map<String, Value> {
        self.options.iter().filter(|o| !matches!(o.kind, OptionKind::Pictures(_))).map(|o| (o.key.clone(), o.value(saved, fonts))).collect()
    }

    /// A size in pixels at 1080p: fixed, or a number option's value.
    pub fn size(&self, size: &[Size; 2], saved: &BTreeMap<String, Value>) -> [f32; 2] {
        size.clone().map(|s| match s {
            Size::Fixed(v) => v,
            Size::Option(key) => self.option(&key).and_then(|o| o.value(saved, &[]).as_f64()).unwrap_or(100.0) as f32,
        })
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct OptionSpec {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// Starts a new section of the theme's options, under this heading.
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub section_description: Option<String>,
    /// A toggle that, when on, turns this option off.
    #[serde(default)]
    pub disabled_by: Option<String>,
    #[serde(flatten)]
    pub kind: OptionKind,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OptionKind {
    Number(NumberOption),
    Toggle {
        #[serde(default)]
        default: bool,
    },
    Color {
        default: String,
    },
    Choice {
        default: String,
        choices: Vec<Choice>,
    },
    /// An imported font file, or none for the game title font.
    Font,
    /// A row that opens the pictures page.
    Pictures(Pictures),
}

#[derive(Deserialize, Clone, Debug)]
pub struct NumberOption {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub default: f64,
    /// Shown after the value, e.g. `px` or `%`.
    #[serde(default)]
    pub unit: String,
    /// The value is shown multiplied by this, e.g. 100 for a 0–1 value shown in `%`.
    #[serde(default = "one")]
    pub display_scale: f64,
    /// Shown instead of the value when it is 0, e.g. "Off".
    #[serde(default)]
    pub zero_label: Option<String>,
}

fn one() -> f64 {
    1.0
}

impl NumberOption {
    pub fn clamp(&self, v: f64) -> f64 {
        let v = if v.is_finite() { v } else { self.default };
        (v.clamp(self.min.min(self.max), self.max.max(self.min)) * 1000.0).round() / 1000.0
    }

    /// The value as typed: scaled for display, without the unit.
    pub fn number(&self, v: f64) -> String {
        let shown = v * self.display_scale;
        if (shown - shown.round()).abs() < 1e-6 { format!("{shown:.0}") } else { format!("{shown:.2}").trim_end_matches('0').to_owned() }
    }

    pub fn label(&self, v: f64) -> String {
        match &self.zero_label {
            Some(zero) if v == 0.0 => zero.clone(),
            _ if self.unit.is_empty() => self.number(v),
            _ => format!("{} {}", self.number(v), self.unit),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

/// A page of pictures the theme shows in place of names: one for each category if
/// `categories` is set, and one for each of `slots`.
#[derive(Deserialize, Clone, Debug)]
pub struct Pictures {
    #[serde(default)]
    pub categories: bool,
    /// The box a category's picture is shown in.
    pub size: [Size; 2],
    #[serde(default)]
    pub slots: Vec<PictureSlot>,
    /// What shows without a picture, e.g. "The name is shown".
    #[serde(default)]
    pub empty: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PictureSlot {
    pub key: String,
    pub label: String,
    /// Starts a new section of the page, under this heading.
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub section_description: Option<String>,
    /// The box it is shown in, when not the one of `Pictures::size`.
    #[serde(default)]
    pub size: Option<[Size; 2]>,
    #[serde(default)]
    pub empty: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GamePicture {
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub section: String,
    #[serde(default)]
    pub section_description: String,
    pub size: [Size; 2],
    /// Each game starts with a copy of its cover, once.
    #[serde(default)]
    pub seed_from_cover: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Size {
    Fixed(f32),
    /// The value of the number option with this key.
    Option(String),
}

impl OptionSpec {
    /// The option's value: the saved one if valid, else the default.
    pub fn value(&self, saved: &BTreeMap<String, Value>, fonts: &[String]) -> Value {
        let saved = saved.get(&self.key);
        match &self.kind {
            OptionKind::Number(n) => n.clamp(saved.and_then(Value::as_f64).unwrap_or(n.default)).into(),
            OptionKind::Toggle { default } => saved.and_then(Value::as_bool).unwrap_or(*default).into(),
            OptionKind::Color { default } => {
                let color = saved.and_then(Value::as_str).and_then(parse_hex).or_else(|| parse_hex(default)).unwrap_or(egui::Color32::WHITE);
                crate::theme::to_hex(color).into()
            }
            OptionKind::Choice { default, choices } => {
                let valid = saved.and_then(Value::as_str).filter(|v| choices.iter().any(|c| c.value == *v));
                valid.unwrap_or(default).into()
            }
            OptionKind::Font => match saved.and_then(Value::as_str) {
                Some(file) if fonts.iter().any(|f| f == file) => file.into(),
                _ => Value::Null,
            },
            OptionKind::Pictures(_) => Value::Null,
        }
    }
}

/// What YASAFF gives the theme each frame. Positions and sizes are in screen points.
#[derive(Serialize)]
pub struct FrameIn<'a> {
    /// The whole window: `[x, y, width, height]`.
    pub screen: [f32; 4],
    /// The space between the top and bottom bars.
    pub area: [f32; 4],
    /// The window's height over 1080: multiply sizes given at 1080p by it.
    pub scale: f32,
    /// Seconds since the theme was loaded, and since the last frame.
    pub time: f64,
    pub dt: f32,
    /// Off while a menu or Settings covers the home screen; clicks aren't reported then.
    pub interactive: bool,
    /// The selected tile, by its index in `library.tiles`.
    pub selected: usize,
    /// Sent on the first frame and whenever it changes; keep it until the next one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<&'a Value>,
    /// Sent on the first frame and whenever they change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<&'a Value>,
    pub events: &'a [Event],
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    /// A direction pressed on the controller or keyboard: `left`, `right`, `up` or `down`.
    Nav(&'static str),
    /// One of the frame's `hits` was clicked, by its id.
    Click(u64),
}

/// What the theme gives back: what to draw, where clicks count, and what to select.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct FrameOut {
    pub draw: Vec<Shape>,
    pub hits: Vec<Hit>,
    /// Selects this tile; YASAFF plays the swap sound when that changes the selection.
    pub select: Option<usize>,
    /// Acts as a click on this tile: selects it, or starts it when already selected.
    pub click: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub struct Hit {
    pub rect: [f32; 4],
    pub id: u64,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    Rect {
        rect: [f32; 4],
        #[serde(default)]
        radius: f32,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        stroke: Option<Stroke>,
    },
    /// A soft halo around a box.
    Glow {
        rect: [f32; 4],
        #[serde(default)]
        radius: f32,
        color: String,
        strength: f32,
    },
    Circle {
        center: [f32; 2],
        radius: f32,
        fill: String,
    },
    Image {
        source: Source,
        rect: [f32; 4],
        #[serde(default = "opaque")]
        alpha: f32,
        #[serde(default)]
        radius: f32,
        /// A halo that follows the picture's outline.
        #[serde(default)]
        glow: Option<Glow>,
    },
    Text {
        text: String,
        pos: [f32; 2],
        #[serde(default)]
        align: [Align; 2],
        size: f32,
        #[serde(default)]
        font: Font,
        color: String,
        /// A halo around the letters; without one the text has a drop shadow.
        #[serde(default)]
        glow: Option<Glow>,
        #[serde(default = "yes")]
        shadow: bool,
    },
    /// Text centered in `rect` at the biggest size up to `max_size` where it fits without breaking a word.
    FitText {
        text: String,
        rect: [f32; 4],
        max_size: f32,
        #[serde(default)]
        font: Font,
        color: String,
        #[serde(default)]
        glow: Option<Glow>,
    },
}

fn opaque() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Debug)]
pub struct Stroke {
    pub width: f32,
    pub color: String,
}

#[derive(Deserialize, Debug)]
pub struct Glow {
    pub color: String,
    pub strength: f32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// A game's picture (`game_picture`), placed as the user set it.
    Tile(usize),
    /// A tile's icon: Discover, Add game and Settings have one.
    Icon(usize),
    /// One of the theme's pictures, by key: `category:<name>` or a slot's key.
    Picture(String),
    /// A file in the theme's `images/` folder.
    File(String),
}

#[derive(Deserialize, Debug, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Top,
    Bottom,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Font {
    /// The game title font from Settings › Fonts.
    #[default]
    Title,
    /// YASAFF's interface font.
    Body,
    /// An imported font file, e.g. a `font` option's value; the title font when it isn't imported.
    File(String),
    /// A game's own title font, else the title font.
    Tile(usize),
}

/// A loaded `theme.wasm`, ready to draw frames.
pub struct Runtime {
    store: Store<StoreLimits>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    frame: TypedFunc<(i32, i32), i64>,
    pub manifest: Manifest,
}

#[derive(Serialize)]
struct DescribeIn<'a> {
    /// `english`, `french`, `spanish` or `polish`.
    language: &'a Value,
}

impl Runtime {
    pub fn load(path: &Path, language: &Value) -> Result<Self> {
        let size = std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?.len();
        ensure!(size <= MAX_CODE, "{} is larger than 32 MB", path.display());
        let wasm = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, &wasm).map_err(|e| anyhow::anyhow!("{} is not a WebAssembly module: {e}", path.display()))?;
        if let Some(import) = module.imports().next() {
            bail!("{CODE_FILE} imports {}::{}, but a theme gets no imports; build it for wasm32-unknown-unknown without WASI", import.module(), import.name());
        }
        let mut store = Store::new(&engine, StoreLimitsBuilder::new().memory_size(MAX_MEMORY).build());
        store.limiter(|limits| limits);
        store.set_fuel(FUEL).map_err(wasm_error)?;
        let instance: Instance = Linker::new(&engine).instantiate_and_start(&mut store, &module).map_err(wasm_error)?;
        let memory = instance.get_memory(&store, "memory").context("theme.wasm doesn't export its memory")?;
        let alloc = instance.get_typed_func(&store, "yasaff_alloc").map_err(|e| anyhow::anyhow!("yasaff_alloc: {e}"))?;
        let frame = instance.get_typed_func(&store, "yasaff_frame").map_err(|e| anyhow::anyhow!("yasaff_frame: {e}"))?;
        let describe: TypedFunc<(i32, i32), i64> = instance.get_typed_func(&store, "yasaff_describe").map_err(|e| anyhow::anyhow!("yasaff_describe: {e}"))?;
        let mut runtime = Self { store, memory, alloc, frame, manifest: Manifest::default() };
        let text = runtime.call(&describe, &DescribeIn { language })?;
        runtime.manifest = serde_json::from_slice(&text).context("yasaff_describe gave back JSON YASAFF can't read")?;
        Ok(runtime)
    }

    pub fn frame(&mut self, input: &FrameIn) -> Result<FrameOut> {
        let text = self.call(&self.frame.clone(), input)?;
        serde_json::from_slice(&text).context("yasaff_frame gave back JSON YASAFF can't read")
    }

    /// Calls `func` with `input` as JSON and returns the JSON it gives back.
    fn call(&mut self, func: &TypedFunc<(i32, i32), i64>, input: &impl Serialize) -> Result<Vec<u8>> {
        let json = serde_json::to_vec(input)?;
        self.store.set_fuel(FUEL).map_err(wasm_error)?;
        let len = i32::try_from(json.len()).context("the input is too big")?;
        let ptr = self.alloc.call(&mut self.store, len).map_err(wasm_error)?;
        let start = usize::try_from(ptr).context("yasaff_alloc gave back a negative address")?;
        let data = self.memory.data_mut(&mut self.store);
        data.get_mut(start..start + json.len()).context("yasaff_alloc gave back room outside the memory")?.copy_from_slice(&json);
        let packed = func.call(&mut self.store, (ptr, len)).map_err(wasm_error)?;
        self.read(packed)
    }

    fn read(&self, packed: i64) -> Result<Vec<u8>> {
        let (ptr, len) = ((packed as u64 >> 32) as usize, (packed as u64 & 0xffff_ffff) as usize);
        let data = self.memory.data(&self.store);
        Ok(data.get(ptr..ptr + len).context("the theme gave back JSON outside its memory")?.to_vec())
    }
}

fn wasm_error(e: wasmi::Error) -> anyhow::Error {
    if e.as_trap_code() == Some(wasmi::TrapCode::OutOfFuel) {
        anyhow::anyhow!("the theme's code ran too long and was stopped")
    } else {
        anyhow::anyhow!("the theme's code failed: {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_fall_back_to_their_defaults() {
        let manifest: Manifest = serde_json::from_str(
            r##"{"options": [
                {"key": "w", "label": "W", "type": "number", "min": 10, "max": 20, "step": 1, "default": 15},
                {"key": "c", "label": "C", "type": "color", "default": "#ff0000"},
                {"key": "k", "label": "K", "type": "choice", "default": "a", "choices": [{"value": "a", "label": "A"}, {"value": "b", "label": "B"}]},
                {"key": "f", "label": "F", "type": "font"},
                {"key": "p", "label": "P", "type": "pictures", "categories": true, "size": [100, "w"]}
            ]}"##,
        )
        .unwrap();
        let saved: BTreeMap<String, Value> = serde_json::from_str(r#"{"w": 99, "c": "nope", "k": "b", "f": "gone.ttf"}"#).unwrap();
        let values = manifest.values(&saved, &["mine.ttf".to_owned()]);
        assert_eq!(values["w"], 20.0);
        assert_eq!(values["c"], "#ff0000");
        assert_eq!(values["k"], "b");
        assert_eq!(values["f"], Value::Null);
        assert!(!values.contains_key("p"));
        assert_eq!(manifest.size(&manifest.pictures().unwrap().size, &saved), [100.0, 20.0]);
    }

    #[test]
    fn shapes_read_with_their_defaults() {
        let out: FrameOut = serde_json::from_str(
            r##"{"draw": [{"type": "image", "source": {"tile": 3}, "rect": [0, 0, 10, 10]},
                         {"type": "text", "text": "Hi", "pos": [1, 2], "size": 20, "color": "#fff", "font": {"file": "a.ttf"}}],
                 "select": 2}"##,
        )
        .unwrap();
        assert!(matches!(out.draw[0], Shape::Image { source: Source::Tile(3), alpha, .. } if alpha == 1.0));
        assert!(matches!(&out.draw[1], Shape::Text { shadow: true, font: Font::File(f), .. } if f == "a.ttf"));
        assert_eq!(out.select, Some(2));
    }
}

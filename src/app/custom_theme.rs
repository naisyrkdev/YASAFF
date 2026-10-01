//! Custom themes: the home screen drawn by the theme's own code (`crate::theme_code`). YASAFF
//! hands the code the library and the input each frame and paints the shapes it gives back.

use super::*;
use crate::theme_code::{self, Align as Anchor, Event, Font, FrameIn, Manifest, Runtime, Source};
use egui::load::{ImagePoll, SizeHint};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, ColorImage, FontFamily, Galley, ImageSource};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::SystemTime;

/// The loaded code of the current custom theme, and what it was last sent.
pub(super) struct ThemeCode {
    /// The theme, the modification time of its code and the language the runtime was loaded for;
    /// a change reloads it.
    loaded: Option<(String, Option<SystemTime>, Value)>,
    runtime: Option<Runtime>,
    pub(super) error: Option<String>,
    events: Vec<Event>,
    library: Option<String>,
    options: Option<String>,
    started: Instant,
    last_frame: Instant,
}

impl Default for ThemeCode {
    fn default() -> Self {
        let now = Instant::now();
        Self { loaded: None, runtime: None, error: None, events: Vec::new(), library: None, options: None, started: now, last_frame: now }
    }
}

impl ThemeCode {
    pub(super) fn manifest(&self) -> Option<&Manifest> {
        self.runtime.as_ref().map(|r| &r.manifest)
    }
}

/// A picture on a custom theme's pictures page: a category's, or one of the slots the theme declares.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum PictureSlot {
    Category(usize),
    Slot(usize),
}

/// Keeps a theme's label for as long as YASAFF runs, so Settings can show it like its own text.
/// Each distinct label is kept once.
pub(super) fn intern(text: &str) -> &'static str {
    static KEPT: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    let kept = kept.get_or_insert_with(HashSet::new);
    if let Some(&s) = kept.get(text) {
        return s;
    }
    let s: &'static str = Box::leak(text.to_owned().into_boxed_str());
    kept.insert(s);
    s
}

impl App {
    pub(super) fn is_custom(&self) -> bool {
        self.theme().settings.kind == ThemeKind::Custom
    }

    pub(super) fn is_standard(&self) -> bool {
        self.theme().settings.kind == ThemeKind::Standard
    }

    /// Whether the custom theme's code is drawing the home screen; without it the row view stands in.
    pub(super) fn custom_running(&self) -> bool {
        self.is_custom() && self.code.runtime.is_some()
    }

    pub(super) fn theme_manifest(&self) -> Option<&Manifest> {
        self.code.manifest().filter(|_| self.is_custom())
    }

    /// Loads the current theme's code when the theme or its code changed.
    pub(super) fn sync_theme_code(&mut self) {
        if !self.is_custom() {
            if self.code.loaded.is_some() {
                self.code = ThemeCode::default();
            }
            return;
        }
        let path = self.theme().code_file();
        let language = json!(self.settings.language);
        let key = (self.theme().id.clone(), std::fs::metadata(&path).and_then(|m| m.modified()).ok(), language.clone());
        if self.code.loaded.as_ref() == Some(&key) {
            return;
        }
        let missing = key.1.is_none();
        self.code = ThemeCode { loaded: Some(key), ..ThemeCode::default() };
        let loaded = if missing {
            Err(tr!("This theme has no code ({file}). Import its package again.", file = theme_code::CODE_FILE))
        } else {
            Runtime::load(&path, &language).map_err(|e| tr!("The theme's code could not be loaded: {error}", error = format!("{e:#}")))
        };
        match loaded {
            Ok(runtime) => self.code.runtime = Some(runtime),
            Err(e) => {
                log::warn!("{}: {e}", self.theme().settings.name);
                self.code.error = Some(e);
            }
        }
    }

    /// Gives games their theme picture from their cover, the first time a theme asks for one.
    pub(super) fn sync_theme_pictures(&mut self) {
        if !self.theme_manifest().and_then(|m| m.game_picture.as_ref()).is_some_and(|p| p.seed_from_cover) {
            return;
        }
        let mut changed = false;
        for i in 0..self.library.games.len() {
            changed |= self.library.seed_theme_picture(i);
        }
        if changed {
            self.save();
        }
    }

    /// Passes a direction on to the theme's code, which moves the selection on its next frame.
    pub(super) fn navigate_custom(&mut self, action: Action) {
        let dir = match action {
            Action::Left => "left",
            Action::Right => "right",
            Action::Up => "up",
            Action::Down => "down",
            _ => return,
        };
        self.code.events.push(Event::Nav(dir));
    }

    /// What the theme knows about the library, as it gets it.
    fn theme_library(&self) -> Value {
        let tiles: Vec<Value> = (0..self.tile_count())
            .map(|i| match (self.library.games.get(i), self.special(i)) {
                (Some(game), _) => json!({
                    "kind": "game",
                    "name": game.name,
                    "status": self.game_status(game),
                    "categories": game.categories,
                    "main_category": game.main_category,
                    "picture": game.asset(AssetKind::ThemePicture).is_some(),
                    "running": self.is_active(i),
                }),
                (None, Some(special)) => json!({
                    "kind": special.key(),
                    "name": special.name(),
                    "status": special_description(special),
                    "icon": self.special_icon(special).is_some(),
                }),
                (None, None) => Value::Null,
            })
            .collect();
        let pictures: Vec<&String> = self.theme().settings.custom.images.keys().filter(|k| self.theme().picture(k).is_some()).collect();
        json!({
            "language": self.settings.language,
            "categories": self.library.categories,
            "pictures": pictures,
            "tiles": tiles,
        })
    }

    pub(super) fn draw_custom(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Some(manifest) = self.code.manifest() else { return };
        let options = Value::Object(manifest.values(&self.theme().settings.custom.values, &self.fonts));
        let library = self.theme_library();
        let (library_text, options_text) = (library.to_string(), options.to_string());
        let send_library = self.code.library.as_ref() != Some(&library_text);
        let send_options = self.code.options.as_ref() != Some(&options_text);
        let top = self.bar_rect(BarSlot::Top, screen, s).bottom();
        let bottom = self.bar_rect(BarSlot::Bottom, screen, s).top();
        let area = Rect::from_x_y_ranges(screen.x_range(), top..=bottom.max(top));
        let now = Instant::now();
        let events = std::mem::take(&mut self.code.events);
        let input = FrameIn {
            screen: [screen.left(), screen.top(), screen.width(), screen.height()],
            area: [area.left(), area.top(), area.width(), area.height()],
            scale: s,
            time: now.duration_since(self.code.started).as_secs_f64(),
            dt: now.duration_since(self.code.last_frame).as_secs_f32(),
            interactive: matches!(self.mode, Mode::Home),
            selected: self.selected,
            library: send_library.then_some(&library),
            options: send_options.then_some(&options),
            events: &events,
        };
        let Some(runtime) = self.code.runtime.as_mut() else { return };
        let out = match runtime.frame(&input) {
            Ok(out) => out,
            Err(e) => {
                let e = tr!("The theme's code stopped: {error}", error = format!("{e:#}"));
                log::warn!("{}: {e}", self.theme().settings.name);
                self.code.runtime = None;
                self.code.error = Some(e.clone());
                return self.notify(e);
            }
        };
        self.code.last_frame = now;
        if send_library {
            self.code.library = Some(library_text);
        }
        if send_options {
            self.code.options = Some(options_text);
        }

        let painter = ui.painter().clone();
        for shape in &out.draw {
            self.paint_shape(ui, &painter, shape, s);
        }
        if matches!(self.mode, Mode::Home) {
            for hit in &out.hits {
                if ui.interact(rect(hit.rect), Id::new(("theme-hit", hit.id)), Sense::click()).clicked() {
                    self.code.events.push(Event::Click(hit.id));
                }
            }
        }
        if !self.code.events.is_empty() {
            ui.ctx().request_repaint();
        }
        let count = self.tile_count();
        if let Some(i) = out.select.filter(|&i| i < count && i != self.selected) {
            self.selected = i;
            self.swap_sound();
        }
        if let Some(i) = out.click.filter(|&i| i < count && matches!(self.mode, Mode::Home)) {
            self.click_tile(i);
        }
    }

    /// Says why the custom theme isn't drawing, over the row view standing in for it.
    pub(super) fn draw_custom_error(&self, ui: &Ui, screen: Rect, s: f32) {
        let Some(error) = self.code.error.as_ref().filter(|_| self.is_custom()) else { return };
        let pos = pos2(screen.center().x, self.bar_rect(BarSlot::Top, screen, s).bottom() + 16.0 * s);
        shadow_text(ui.painter(), pos, Align2::CENTER_TOP, error, 20.0 * s, PAD_Y);
    }

    fn paint_shape(&self, ui: &Ui, painter: &Painter, shape: &theme_code::Shape, s: f32) {
        use theme_code::Shape as T;
        match shape {
            T::Rect { rect: r, radius, fill, stroke } => {
                let (r, radius) = (rect(*r), *radius as u8);
                if let Some(fill) = fill {
                    painter.rect_filled(r, radius, color(fill));
                }
                if let Some(stroke) = stroke {
                    painter.rect_stroke(r, radius, Stroke::new(stroke.width, color(&stroke.color)), StrokeKind::Inside);
                }
            }
            T::Glow { rect: r, radius, color: c, strength } => glow_box(painter, rect(*r), *radius as u8, color(c), *strength, s),
            T::Circle { center, radius, fill } => {
                painter.circle_filled(pos2(center[0], center[1]), *radius, color(fill));
            }
            T::Image { source, rect: r, alpha, radius, glow } => {
                let (r, radius) = (rect(*r), *radius as u8);
                if let Source::Icon(i) = source {
                    let Some(icon) = self.special(*i).and_then(|sp| self.special_icon(sp)) else { return };
                    if let (Some(glow), Some(uri)) = (glow, source_uri(&icon)) {
                        glow_outline(painter, uri, r, CoverPlacement::default(), color(&glow.color), glow.strength);
                    }
                    egui::Image::new(icon).tint(Color32::WHITE.gamma_multiply(*alpha)).paint_at(ui, r);
                    return;
                }
                let Some((path, placement)) = self.shape_image(source) else { return };
                if let Some(glow) = glow {
                    glow_outline(painter, &file_uri(&path), r, placement, color(&glow.color), glow.strength);
                }
                paint_image_placed(ui, &path, r, *alpha, radius, placement);
            }
            T::Text { text, pos, align, size, font, color: c, glow, shadow } => {
                let c = color(c);
                let galley = painter.layout_no_wrap(text.clone(), self.shape_font(font, *size), c);
                let anchor = Align2([align_of(align[0]), align_of(align[1])]);
                let at = anchor.anchor_size(pos2(pos[0], pos[1]), galley.size()).min;
                match glow {
                    Some(glow) => paint_glowing_text(painter, at, galley, c, Some((color(&glow.color), glow.strength)), s),
                    None if *shadow => paint_glowing_text(painter, at, galley, c, None, s),
                    None => painter.galley(at, galley, c),
                }
            }
            T::FitText { text, rect: r, max_size, font, color: c, glow } => {
                let (r, c) = (rect(*r), color(c));
                let family = self.shape_font(font, *max_size).family;
                let galley = fit_text(painter, text, &family, *max_size, r, c);
                let at = r.center() - galley.rect.center().to_vec2();
                paint_glowing_text(painter, at, galley, c, glow.as_ref().map(|g| (color(&g.color), g.strength)), s);
            }
        }
    }

    fn shape_image(&self, source: &Source) -> Option<(PathBuf, CoverPlacement)> {
        let theme = self.theme();
        match source {
            Source::Tile(i) => {
                let game = self.library.games.get(*i)?;
                Some((game.asset(AssetKind::ThemePicture)?.to_path_buf(), game.placement(AssetKind::ThemePicture)))
            }
            Source::Picture(key) => Some((theme.picture(key)?, theme.settings.custom.placement(key))),
            Source::File(name) => Some((theme.image_file(name)?, CoverPlacement::default())),
            Source::Icon(_) => None,
        }
    }

    fn shape_font(&self, font: &Font, size: f32) -> FontId {
        match font {
            Font::Title => FontId::new(size, fonts::title_family()),
            Font::Body => FontId::proportional(size),
            Font::File(file) if self.fonts.contains(file) => FontId::new(size, fonts::family(file)),
            Font::File(_) => FontId::new(size, fonts::title_family()),
            Font::Tile(i) => match self.library.games.get(*i) {
                Some(game) => self.game_title_font(game, size),
                None => FontId::new(size, fonts::title_family()),
            },
        }
    }

    /// The row that opens the pictures page: the theme's `pictures` option.
    pub(super) fn pictures_row(&self) -> SettingRow {
        let index = self.theme_manifest().and_then(|m| m.options.iter().position(|o| matches!(o.kind, theme_code::OptionKind::Pictures(_))));
        SettingRow::ThemeOption(index.unwrap_or(0))
    }

    /// The theme's pictures page: each category if it asks for them, then its own slots.
    pub(super) fn picture_slots(&self) -> Vec<PictureSlot> {
        let Some(pictures) = self.theme_manifest().and_then(Manifest::pictures) else { return Vec::new() };
        let categories = if pictures.categories { 0..self.library.categories.len() } else { 0..0 };
        categories.map(PictureSlot::Category).chain((0..pictures.slots.len()).map(PictureSlot::Slot)).collect()
    }

    pub(super) fn picture_key(&self, slot: PictureSlot) -> Option<String> {
        match slot {
            PictureSlot::Category(i) => self.library.categories.get(i).map(|c| theme::picture_key(c)),
            PictureSlot::Slot(i) => Some(self.theme_manifest()?.pictures()?.slots.get(i)?.key.clone()),
        }
    }

    pub(super) fn picture_name(&self, slot: PictureSlot) -> String {
        match slot {
            PictureSlot::Category(i) => self.library.categories.get(i).cloned().unwrap_or_default(),
            PictureSlot::Slot(i) => self.picture_spec(i).map(|p| p.label.clone()).unwrap_or_default(),
        }
    }

    pub(super) fn picture_spec(&self, i: usize) -> Option<&theme_code::PictureSlot> {
        self.theme_manifest()?.pictures()?.slots.get(i)
    }

    /// What shows without the picture, as its row says.
    pub(super) fn picture_empty(&self, slot: PictureSlot) -> &'static str {
        let pictures = self.theme_manifest().and_then(Manifest::pictures);
        let empty = match slot {
            PictureSlot::Slot(i) => self.picture_spec(i).and_then(|p| p.empty.as_deref()),
            PictureSlot::Category(_) => None,
        };
        match empty.or(pictures.map(|p| p.empty.as_str())).filter(|e| !e.is_empty()) {
            Some(text) => intern(text),
            None => tr!("None"),
        }
    }

    /// The width over the height of the box the picture is shown in.
    pub(super) fn picture_aspect(&self, slot: PictureSlot) -> f32 {
        let Some(manifest) = self.theme_manifest() else { return 1.0 };
        let Some(pictures) = manifest.pictures() else { return 1.0 };
        let size = match slot {
            PictureSlot::Slot(i) => self.picture_spec(i).and_then(|p| p.size.as_ref()).unwrap_or(&pictures.size),
            PictureSlot::Category(_) => &pictures.size,
        };
        let [w, h] = manifest.size(size, &self.theme().settings.custom.values);
        w / h.max(1.0)
    }

    /// The width over the height of a game's theme picture.
    pub(super) fn game_picture_aspect(&self) -> f32 {
        let Some(manifest) = self.theme_manifest() else { return 1.0 };
        let Some(picture) = manifest.game_picture.as_ref() else { return 1.0 };
        let [w, h] = manifest.size(&picture.size, &self.theme().settings.custom.values);
        w / h.max(1.0)
    }
}

fn rect([x, y, w, h]: [f32; 4]) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(w.max(0.0), h.max(0.0)))
}

fn color(text: &str) -> Color32 {
    theme::parse_hex(text).unwrap_or(Color32::WHITE)
}

fn align_of(align: Anchor) -> Align {
    match align {
        Anchor::Left | Anchor::Top => Align::Min,
        Anchor::Center => Align::Center,
        Anchor::Right | Anchor::Bottom => Align::Max,
    }
}

/// Lays `text` out centered and justified in `rect`, at the biggest size up to `max` where it fits
/// without breaking a word.
fn fit_text(painter: &Painter, text: &str, family: &FontFamily, max: f32, rect: Rect, color: Color32) -> Arc<Galley> {
    const MIN: f32 = 8.0;
    let layout = |size: f32| {
        let font = FontId::new(size, family.clone());
        let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font.clone(), color));
        job.wrap.max_width = rect.width();
        job.halign = Align::Center;
        job.justify = true;
        let galley = painter.layout_job(job);
        let widest_word = text.split_whitespace().map(|w| painter.layout_no_wrap(w.to_owned(), font.clone(), color).size().x).fold(0.0, f32::max);
        let fits = galley.size().y <= rect.height() && widest_word <= rect.width();
        (galley, fits)
    };
    let max = max.max(MIN);
    if let (galley, true) = layout(max) {
        return galley;
    }
    let (mut lo, mut hi) = (MIN, max);
    for _ in 0..10 {
        let mid = (lo + hi) / 2.0;
        if layout(mid).1 { lo = mid } else { hi = mid }
    }
    layout(lo).0
}

/// A soft halo of `color` around `rect`; `strength` goes from 0 (none) to 1.
fn glow_box(painter: &Painter, rect: Rect, radius: u8, color: Color32, strength: f32, s: f32) {
    if strength <= 0.01 {
        return;
    }
    let shadow = egui::epaint::Shadow {
        offset: [0, 0],
        blur: (36.0 * s * strength).clamp(1.0, 255.0) as u8,
        spread: (6.0 * s * strength).clamp(0.0, 255.0) as u8,
        color: color.gamma_multiply(0.9 * strength),
    };
    painter.add(shadow.as_shape(rect, radius));
}

/// How far an outline glow reaches past its picture, as a share of the picture's shorter side.
const GLOW_REACH: f32 = 0.3;

/// A soft halo of `color` around the opaque parts of the image at `uri`, placed in `rect` as
/// `placement` says; `strength` goes from 0 (none) to 1. Transparent parts, such as around a logo,
/// don't glow, so the halo follows the picture's outline instead of its box.
fn glow_outline(painter: &Painter, uri: &str, rect: Rect, placement: CoverPlacement, color: Color32, strength: f32) {
    if strength <= 0.01 || !rect.is_positive() {
        return;
    }
    let Some(mask) = outline_mask(painter.ctx(), uri, rect.size(), placement) else { return };
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.image(mask.id(), rect.expand(rect.size().min_elem() * GLOW_REACH), full, color.gamma_multiply(0.9 * strength));
}

/// The blurred shape of the image as it shows in a picture of `size`, with room around it for the
/// glow, as a white texture whose alpha is the glow's. It doesn't change with the picture's scale,
/// so one is made for each image, aspect and placement, and kept.
fn outline_mask(ctx: &Context, uri: &str, size: Vec2, placement: CoverPlacement) -> Option<TextureHandle> {
    const CELLS: f32 = 96.0;
    let milli = |v: f32| (v * 1000.0).round() as i32;
    let id = Id::new(("outline-glow", uri, milli(size.x / size.y), milli(placement.zoom), milli(placement.x), milli(placement.y)));
    if let Some(texture) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(texture);
    }
    let hint = SizeHint::Size { width: 256, height: 256, maintain_aspect_ratio: true };
    let Ok(ImagePoll::Ready { image }) = ctx.try_load_image(uri, hint) else { return None };
    let [iw, ih] = image.size;
    if iw == 0 || ih == 0 {
        return None;
    }
    // The picture measured in its shorter side, and the whole mask with the reach on each side.
    let unit = size / size.min_elem();
    let total = unit + Vec2::splat(2.0 * GLOW_REACH);
    let (gw, gh) = ((total.x / total.max_elem() * CELLS).ceil() as usize, (total.y / total.max_elem() * CELLS).ceil() as usize);
    let cell = vec2(total.x / gw as f32, total.y / gh as f32);
    let (part, uv) = box3d::place(vec2(iw as f32, ih as f32), size, placement);
    let mut alpha = vec![0.0f32; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let mut sum = 0.0;
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let p = pos2(((gx as f32 + sx) * cell.x - GLOW_REACH) / unit.x, ((gy as f32 + sy) * cell.y - GLOW_REACH) / unit.y);
                if !part.contains(p) {
                    continue;
                }
                let u = uv.min.x + (p.x - part.min.x) / part.width() * uv.width();
                let v = uv.min.y + (p.y - part.min.y) / part.height() * uv.height();
                let (px, py) = (((u * iw as f32) as usize).min(iw - 1), ((v * ih as f32) as usize).min(ih - 1));
                sum += image.pixels[py * iw + px].a() as f32 / 255.0;
            }
            alpha[gy * gw + gx] = sum / 4.0;
        }
    }
    let radius = (GLOW_REACH / cell.max_elem() / 2.5).round().max(1.0) as usize;
    for _ in 0..3 {
        box_blur(&mut alpha, gw, gh, radius);
    }
    // Doubled, so the glow is full at the picture's edge and fades out to nothing at its reach.
    let pixels = alpha.iter().map(|a| Color32::from_white_alpha(((a * 2.0).min(1.0) * 255.0) as u8)).collect();
    let texture = ctx.load_texture(format!("outline-glow:{uri}"), ColorImage::new([gw, gh], pixels), TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    Some(texture)
}

/// Averages each value with the ones within `r` of it, across and then down.
fn box_blur(values: &mut [f32], w: usize, h: usize, r: usize) {
    let mut pass = |len: usize, lines: usize, at: &dyn Fn(usize, usize) -> usize| {
        let mut line = vec![0.0; len];
        for l in 0..lines {
            for (k, v) in line.iter_mut().enumerate() {
                *v = values[at(l, k)];
            }
            for k in 0..len {
                let (from, to) = (k.saturating_sub(r), (k + r).min(len - 1));
                values[at(l, k)] = line[from..=to].iter().sum::<f32>() / (2 * r + 1) as f32;
            }
        }
    };
    pass(w, h, &|y, x| y * w + x);
    pass(h, w, &|x, y| y * w + x);
}

fn source_uri<'a>(source: &'a ImageSource) -> Option<&'a str> {
    match source {
        ImageSource::Uri(uri) | ImageSource::Bytes { uri, .. } => Some(uri),
        ImageSource::Texture(_) => None,
    }
}

/// Paints text with a drop shadow, and with `glow` (its color and strength from 0 to 1), a halo
/// around the letters.
fn paint_glowing_text(painter: &Painter, pos: Pos2, galley: Arc<Galley>, color: Color32, glow: Option<(Color32, f32)>, s: f32) {
    match glow.filter(|(_, strength)| *strength > 0.01) {
        Some((glow_color, strength)) => {
            for ring in 1..=4 {
                let r = ring as f32 * 1.6 * s * (0.5 + strength);
                let ring_color = glow_color.gamma_multiply(strength * 0.3 / ring as f32);
                for k in 0..12 {
                    let offset = Vec2::angled(k as f32 * std::f32::consts::TAU / 12.0) * r;
                    painter.galley_with_override_text_color(pos + offset, galley.clone(), ring_color);
                }
            }
        }
        None => painter.galley_with_override_text_color(pos + vec2(0.0, 2.0 * s), galley.clone(), Color32::from_black_alpha(160)),
    }
    painter.galley(pos, galley, color);
}

pub(super) fn special_description(special: Special) -> &'static str {
    match special {
        Special::Store => tr!("Apps, games and themes from YASAFF Discover, installed and updated right here"),
        Special::Add => tr!("Scan for installed games, or add an app, a .desktop/shortcut file, or a command or URL"),
        Special::Settings => tr!("Change theme, appearance, set discover server and more"),
    }
}

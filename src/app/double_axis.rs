//! The Double axis theme: categories side by side along the top, the games of the selected one
//! listed below it, as on a PlayStation 3. The last category, Main, holds Settings, Discover and
//! Add game.

use super::*;
use crate::theme::{AxisColumn, AxisPicture, TitleInfo, axis_image_key};
use egui::load::{ImagePoll, SizeHint};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, ColorImage, FontFamily, Galley, ImageSource, TextureHandle};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
enum ColumnKind {
    Category,
    Uncategorized,
    Main,
}

/// One column of the home screen: a category and the tiles in it.
pub(super) struct AxisCol {
    kind: ColumnKind,
    pub(super) name: String,
    pub(super) items: Vec<usize>,
}

impl AxisCol {
    pub(super) fn column(&self) -> AxisColumn<'_> {
        match self.kind {
            ColumnKind::Category => AxisColumn::Category(&self.name),
            ColumnKind::Uncategorized => AxisColumn::Uncategorized,
            ColumnKind::Main => AxisColumn::Main,
        }
    }

    fn key(&self) -> String {
        axis_image_key(self.column())
    }
}

/// The name of a built-in column, as the home screen and the category images page show it.
pub(super) fn builtin_column_name(column: AxisColumn) -> &'static str {
    match column {
        AxisColumn::Uncategorized => tr!("Games"),
        _ => tr!("Main"),
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

impl App {
    pub(super) fn is_double_axis(&self) -> bool {
        self.theme().settings.kind == ThemeKind::DoubleAxis
    }

    pub(super) fn is_standard(&self) -> bool {
        self.theme().settings.kind == ThemeKind::Standard
    }

    /// The columns with something in them, Main last.
    pub(super) fn axis_columns(&self) -> Vec<AxisCol> {
        let games = &self.library.games;
        let mut columns: Vec<AxisCol> = self
            .library
            .categories
            .iter()
            .map(|name| AxisCol {
                kind: ColumnKind::Category,
                name: name.clone(),
                items: (0..games.len()).filter(|&i| games[i].categories.contains(name)).collect(),
            })
            .collect();
        columns.push(AxisCol {
            kind: ColumnKind::Uncategorized,
            name: builtin_column_name(AxisColumn::Uncategorized).to_owned(),
            items: (0..games.len()).filter(|&i| games[i].categories.is_empty()).collect(),
        });
        columns.retain(|c| !c.items.is_empty());
        columns.push(AxisCol {
            kind: ColumnKind::Main,
            name: builtin_column_name(AxisColumn::Main).to_owned(),
            items: (games.len()..self.tile_count()).collect(),
        });
        columns
    }

    /// The column shown: the one last moved to while it holds the selected tile, else the first that does.
    fn axis_current(&self, columns: &[AxisCol]) -> usize {
        let holds = |c: &AxisCol| c.items.contains(&self.selected);
        let remembered = self.axis_column.as_ref().and_then(|key| columns.iter().position(|c| c.key() == *key && holds(c)));
        remembered.or_else(|| columns.iter().position(holds)).unwrap_or(0)
    }

    fn focus_axis_column(&mut self, columns: &[AxisCol], from: usize, to: usize) {
        if let Some(col) = columns.get(from) {
            self.axis_rows.insert(col.key(), self.selected);
        }
        let Some(col) = columns.get(to) else { return };
        let remembered = self.axis_rows.get(&col.key()).copied().filter(|i| col.items.contains(i));
        if let Some(i) = remembered.or(col.items.first().copied()) {
            self.selected = i;
        }
        self.axis_column = Some(col.key());
    }

    pub(super) fn navigate_axis(&mut self, action: Action) {
        let columns = self.axis_columns();
        let current = self.axis_current(&columns);
        let before = self.selected;
        match action {
            Action::Left if current > 0 => self.focus_axis_column(&columns, current, current - 1),
            Action::Right if current + 1 < columns.len() => self.focus_axis_column(&columns, current, current + 1),
            Action::Up | Action::Down => {
                let items = &columns[current].items;
                let row = items.iter().position(|&i| i == self.selected).unwrap_or(0);
                let row = if action == Action::Up { row.saturating_sub(1) } else { (row + 1).min(items.len() - 1) };
                self.selected = items[row];
                self.axis_column = Some(columns[current].key());
            }
            _ => {}
        }
        if self.selected != before {
            self.swap_sound();
        }
    }

    /// Gives games their Double axis image from their cover, the first time it is needed.
    pub(super) fn sync_axis_images(&mut self) {
        if !self.is_double_axis() {
            return;
        }
        let mut changed = false;
        for i in 0..self.library.games.len() {
            changed |= self.library.seed_axis_image(i);
        }
        if changed {
            self.save();
        }
    }

    fn axis_font(&self) -> FontFamily {
        match &self.theme().settings.double_axis.font {
            Some(file) if self.fonts.contains(file) => fonts::family(file),
            _ => fonts::title_family(),
        }
    }

    pub(super) fn draw_double_axis(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let columns = self.axis_columns();
        let current = self.axis_current(&columns);
        let axis = self.theme().settings.double_axis.clone();
        let [w, h] = axis.size().map(|v| v * s);
        let gap = 36.0 * s;
        let radius = (8.0 * s) as u8;
        let family = self.axis_font();
        let ctx = ui.ctx().clone();
        let interactive = matches!(self.mode, Mode::Home);
        let painter = ui.painter().clone();

        let top = self.bar_rect(BarSlot::Top, screen, s).bottom() + 40.0 * s;
        let anchor = screen.left() + (screen.width() * 0.18).max(110.0 * s);
        let scroll = ctx.animate_value_with_time(Id::new("axis-column"), current as f32, 0.2);
        let mut clicked_column = None;
        for (j, col) in columns.iter().enumerate() {
            let offset = j as f32 - scroll;
            let focus = (1.0 - offset.abs()).max(0.0);
            let scale = 0.75 + 0.25 * focus;
            let rect = Rect::from_min_size(pos2(anchor + offset * (w + gap), top), vec2(w, h));
            let rect = Rect::from_center_size(rect.center(), rect.size() * scale);
            if rect.right() < screen.left() || rect.left() > screen.right() {
                continue;
            }
            // Only the selected category glows; the others are just smaller, never faded.
            let glow = axis.glow() * focus;
            let text_color = axis.text_color;
            let image = self.theme().axis_image(col.column());
            match &image {
                Some(path) => {
                    let placement = axis.placement(col.column());
                    glow_outline(&painter, &file_uri(path), rect, placement, axis.glow_color, glow);
                    paint_image_placed(ui, path, rect, 1.0, radius, placement);
                }
                None if axis.border => glow_box(&painter, rect, radius, axis.glow_color, glow, s),
                None => {}
            }
            if axis.border {
                painter.rect_stroke(rect, radius, Stroke::new(2.0 * s, text_color), StrokeKind::Inside);
            }
            if image.is_none() {
                let inner = rect.shrink(10.0 * s * scale);
                let max = if axis.dynamic_font { inner.height() } else { axis.font_size() * s * scale };
                let galley = fit_text(&painter, &col.name, &family, max, inner, text_color);
                let pos = inner.center() - galley.rect.center().to_vec2();
                paint_glowing_text(&painter, pos, galley, text_color, (!axis.border).then_some((axis.glow_color, glow)), s);
            } else if j == current {
                let font = FontId::new(22.0 * s, family.clone());
                let galley = painter.layout_no_wrap(col.name.clone(), font, axis.text_color);
                let pos = rect.center_bottom() + vec2(-galley.size().x / 2.0, 10.0 * s);
                paint_glowing_text(&painter, pos, galley, axis.text_color, None, s);
            }
            if interactive && ui.interact(rect, Id::new(("axis-column", j)), Sense::click()).clicked() {
                clicked_column = Some(j);
            }
        }

        let clicked_item = self.draw_axis_items(ui, &columns[current], pos2(anchor, top), vec2(w, h), screen, s);

        if let Some(j) = clicked_column.filter(|&j| j != current) {
            self.focus_axis_column(&columns, current, j);
            self.swap_sound();
        } else if let Some(i) = clicked_item {
            self.axis_column = Some(columns[current].key());
            self.click_tile(i);
        }
    }

    /// The tiles of the selected column: the selected one just below the categories, the ones
    /// before it above them. Returns the clicked tile.
    fn draw_axis_items(&self, ui: &Ui, col: &AxisCol, origin: Pos2, size: Vec2, screen: Rect, s: f32) -> Option<usize> {
        let ctx = ui.ctx();
        let painter = ui.painter();
        let axis = &self.theme().settings.double_axis;
        let row = col.items.iter().position(|&i| i == self.selected).unwrap_or(0);
        let scroll = ctx.animate_value_with_time(Id::new(("axis-row", col.key())), row as f32, 0.18);
        let (small, big) = (64.0 * s, 104.0 * s);
        let step = small + 18.0 * s;
        let below = origin.y + size.y + if self.theme().axis_image(col.column()).is_some() { 64.0 * s } else { 30.0 * s };
        let above = origin.y - 24.0 * s - small;
        let interactive = matches!(self.mode, Mode::Home);
        let mut clicked = None;
        for (k, &i) in col.items.iter().enumerate() {
            let d = k as f32 - scroll;
            let focus = (1.0 - d.abs()).max(0.0);
            let selected = k == row;
            let y = if d >= 0.0 {
                below + d * step + (big - small) * d.min(1.0)
            } else if d > -1.0 {
                above - step + (below - above + step) * (d + 1.0)
            } else {
                above + (d + 1.0) * step
            };
            let item_h = small + (big - small) * focus;
            if y > screen.bottom() || y + item_h < screen.top() {
                continue;
            }
            let alpha = if d < -0.5 { 0.45 } else { 0.7 + 0.3 * focus };
            let glow = axis.glow() * focus;
            let picture = Rect::from_min_size(pos2(origin.x, y), vec2(item_h, item_h));
            let game = self.library.games.get(i);
            let image = game.and_then(|g| g.asset(AssetKind::Axis));
            // A Main entry shows the theme's picture for it, else its tile icon.
            let entry = if game.is_none() { self.special(i).map(|sp| AxisPicture::Entry(sp.key())) } else { None };
            let entry_image = entry.and_then(|e| self.theme().axis_image(e));
            let icon = if game.is_none() && entry_image.is_none() { self.special(i).and_then(|sp| self.special_icon(sp)) } else { None };
            let image = image.map(|path| (path, game.map(|g| g.placement(AssetKind::Axis)).unwrap_or_default()));
            let image = image.or(entry_image.as_deref().zip(entry.map(|e| axis.placement(e))));
            let has_picture = image.is_some() || icon.is_some();
            if let Some((path, placement)) = image {
                glow_outline(painter, &file_uri(path), picture, placement, axis.glow_color, glow);
                paint_image_placed(ui, path, picture, alpha, (6.0 * s) as u8, placement);
            } else if let Some(icon) = icon {
                let shown = picture.shrink(item_h * 0.18);
                if let Some(uri) = source_uri(&icon) {
                    glow_outline(painter, uri, shown, CoverPlacement::default(), axis.glow_color, glow);
                }
                egui::Image::new(icon).tint(Color32::WHITE.gamma_multiply(alpha)).paint_at(ui, shown);
            }
            if self.is_active(i) {
                painter.circle_filled(pos2(origin.x - 18.0 * s, y + item_h / 2.0), 6.0 * s, PAD_A);
            }
            // Without a picture the name is all there is, so it always shows.
            let show_title = !has_picture
                || match axis.title_info {
                    TitleInfo::Selected => selected,
                    TitleInfo::Always => true,
                    TitleInfo::Never => false,
                };
            if show_title {
                let x = if has_picture { picture.right() + 20.0 * s } else { origin.x };
                let name = match game {
                    Some(g) => g.name.clone(),
                    None => self.special(i).map_or("", Special::name).to_owned(),
                };
                let font_size = 26.0 * s + 14.0 * s * focus;
                let font = match game {
                    Some(g) => self.game_title_font(g, font_size),
                    None => FontId::proportional(font_size),
                };
                let color = axis.text_color.gamma_multiply(alpha);
                let with_status = selected && axis.title_info != TitleInfo::Never;
                let text_y = if with_status { y + item_h * 0.42 } else { y + item_h / 2.0 };
                let galley = painter.layout_no_wrap(name, font, color);
                let pos = pos2(x, text_y - galley.size().y / 2.0);
                paint_glowing_text(painter, pos, galley, color, (!has_picture).then_some((axis.glow_color, glow)), s);
                if with_status {
                    let sub = match game {
                        Some(g) => self.game_status(g),
                        None => self.special(i).map_or("", special_description).to_owned(),
                    };
                    let sub_color = axis.text_color.gamma_multiply(0.75 * ((focus - 0.5) * 2.0).max(0.0));
                    shadow_text(painter, pos2(x, y + item_h * 0.42 + font_size * 0.75), Align2::LEFT_TOP, &sub, 20.0 * s, sub_color);
                }
            }
            let hit = Rect::from_min_max(pos2(origin.x, y), pos2(screen.right() - 60.0 * s, y + item_h));
            if interactive && ui.interact(hit, Id::new(("axis-item", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }
        clicked
    }
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

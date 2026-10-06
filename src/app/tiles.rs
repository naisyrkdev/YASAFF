//! Painting one game tile in each style: flat art, box, disc, case, 3D object and 3D model.

use super::*;

pub(super) const SWAP_GLOW: Color32 = Color32::from_rgb(255, 210, 60);

pub(super) const RUNNING_GLOW: Color32 = Color32::from_rgb(70, 230, 90);

pub(super) struct BubbleLook {
    pub(super) empty: bool,
    pub(super) texture: Option<(TextureId, Vec2)>,
    pub(super) fill: Color32,
    pub(super) label: String,
    pub(super) icon: Option<egui::ImageSource<'static>>,
    pub(super) swapping: bool,
    pub(super) running: bool,
}

pub(super) fn disc_mesh(texture: TextureId, rings: usize, segments: usize, vertex: impl Fn(f32, f32) -> (Pos2, Pos2, Color32)) -> Mesh {
    let mut mesh = Mesh::with_texture(texture);
    let push = |mesh: &mut Mesh, rho: f32, theta: f32| {
        let (pos, uv, color) = vertex(rho, theta);
        mesh.vertices.push(egui::epaint::Vertex { pos, uv, color });
    };
    push(&mut mesh, 0.0, 0.0);
    for ring in 1..=rings {
        let rho = ring as f32 / rings as f32;
        for seg in 0..segments {
            push(&mut mesh, rho, seg as f32 / segments as f32 * std::f32::consts::TAU);
        }
    }
    let at = |ring: usize, seg: usize| (1 + (ring - 1) * segments + seg % segments) as u32;
    for seg in 0..segments {
        mesh.add_triangle(0, at(1, seg), at(1, seg + 1));
        for ring in 1..rings {
            mesh.add_triangle(at(ring, seg), at(ring + 1, seg), at(ring + 1, seg + 1));
            mesh.add_triangle(at(ring, seg), at(ring + 1, seg + 1), at(ring, seg + 1));
        }
    }
    mesh
}

pub(super) fn scale_color(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_premultiplied(f(c.r()), f(c.g()), f(c.b()), c.a())
}

/// Paints an icon (a white SVG) `size` square at `center`, tinted with `tint`.
pub(super) fn paint_icon(painter: &Painter, icon: egui::ImageSource<'static>, center: Pos2, size: f32, tint: Color32) {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    if let Ok(egui::load::TexturePoll::Ready { texture }) = egui::Image::new(icon).load_for_size(painter.ctx(), rect.size()) {
        painter.image(texture.id, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
    }
}


impl App {
    /// Paints a special tile's mark: its icon if it has one, else its glyph.
    fn paint_special_mark(&self, painter: &Painter, special: Option<Special>, center: Pos2, size: f32) {
        match special.and_then(|s| self.special_icon(s)) {
            Some(icon) => paint_icon(painter, icon, center, size, Color32::WHITE),
            None => {
                painter.text(center, Align2::CENTER_CENTER, special.map_or("", Special::glyph), FontId::proportional(size), Color32::WHITE);
            }
        }
    }
}

pub(super) fn paint_bubble(painter: &Painter, center: Pos2, r: f32, look: &BubbleLook, t: f32, focus: f32, s: f32) {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let white_uv = egui::epaint::WHITE_UV;
    if !look.empty {
        painter.circle_filled(center + vec2(0.0, r * 0.06 + 6.0 * s), r * 0.97, Color32::from_black_alpha(60));
    }
    let running_pulse = 0.5 + 0.5 * (t * 3.0).sin();
    if look.running {
        let reach = (r * 0.35).clamp(14.0 * s, 40.0 * s) * (0.8 + 0.4 * running_pulse);
        const RINGS: usize = 12;
        let step = reach / RINGS as f32;
        for k in 0..RINGS {
            let fade = (1.0 - k as f32 / RINGS as f32).powi(2);
            let glow = RUNNING_GLOW.gamma_multiply((0.3 + 0.5 * running_pulse) * fade);
            painter.circle_stroke(center, r + step * (k as f32 + 0.5), Stroke::new(step * 1.2, glow));
        }
    }

    let light = egui::emath::Vec2::new(-0.45, -0.55);
    let light_z = (1.0 - light.length_sq()).sqrt();
    let (texture, uv_half, base) = match look.texture {
        Some((id, size)) => (id, vec2((size.y / size.x).min(1.0), (size.x / size.y).min(1.0)) * 0.5, Color32::WHITE),
        None => (TextureId::default(), Vec2::ZERO, look.fill),
    };
    if !look.empty {
        painter.add(disc_mesh(texture, 10, 56, |rho, theta| {
        let dir = Vec2::angled(theta);
        let z = (1.0 - rho * rho).max(0.0).sqrt();
        let lit = 0.62 + 0.38 * (dir.dot(light) * rho + z * light_z).max(0.0);
        let rim = 1.0 - 0.3 * rho.powi(4);
        let uv = if look.texture.is_some() { pos2(0.5, 0.5) + dir * (rho.asin() / FRAC_PI_2) * uv_half * 2.0 } else { white_uv };
        (center + dir * rho * r, uv, scale_color(base, lit * rim))
        }));
    }
    if !look.empty && look.texture.is_none() {
        match &look.icon {
            Some(icon) => paint_icon(painter, icon.clone(), center, r * 0.6, Color32::WHITE),
            None if !look.label.is_empty() => shadow_text(painter, center, Align2::CENTER_CENTER, &look.label, r * 0.5, Color32::WHITE),
            None => {}
        }
    }

    painter.add(disc_mesh(TextureId::default(), 8, 56, |rho, theta| {
        let hue = (theta / TAU + t * 0.04 + rho * 0.35).rem_euclid(1.0);
        let strength = 0.03 + 0.5 * rho.powi(3);
        let rgb = egui::ecolor::Hsva::new(hue, 0.6, 1.0, 1.0).to_rgb();
        let c = |v: f32| (v * strength * 255.0) as u8;
        let color = Color32::from_rgba_premultiplied(c(rgb[0]), c(rgb[1]), c(rgb[2]), (strength * 90.0) as u8);
        (center + Vec2::angled(theta) * rho * r, white_uv, color)
    }));

    let highlight = |at: Vec2, radii: Vec2, angle: f32, alpha: f32| {
        let turn = egui::emath::Rot2::from_angle(angle);
        disc_mesh(TextureId::default(), 6, 32, move |rho, theta| {
            let fade = (1.0 - rho * rho).powi(2);
            let pos = center + at * r + turn * (Vec2::angled(theta) * rho * radii * r);
            (pos, white_uv, Color32::from_white_alpha((alpha * fade) as u8))
        })
    };
    painter.add(highlight(vec2(-0.36, -0.42), vec2(0.34, 0.19), -0.65, 200.0));
    painter.add(highlight(vec2(0.44, 0.47), vec2(0.11, 0.07), -0.65, 90.0));

    painter.circle_stroke(center, r, Stroke::new(1.4 * s, Color32::from_white_alpha(90)));
    painter.circle_stroke(center, r * 0.96, Stroke::new(1.0 * s, Color32::from_white_alpha(22)));
    if look.swapping {
        let pulse = 0.75 + 0.25 * (t * 4.0).sin();
        for k in 0..8 {
            let alpha = pulse * 0.5 * (1.0 - k as f32 / 8.0).powi(2);
            painter.circle_stroke(center, r + (2.0 + k as f32 * 3.0) * s, Stroke::new(4.0 * s, SWAP_GLOW.gamma_multiply(alpha)));
        }
        painter.circle_stroke(center, r, Stroke::new(3.0 * s, SWAP_GLOW.gamma_multiply(pulse)));
    }
    if look.running {
        painter.circle_stroke(center, r, Stroke::new((3.0 + 2.0 * running_pulse) * s, RUNNING_GLOW.gamma_multiply(0.6 + 0.4 * running_pulse)));
    }
    if focus > 0.0 {
        painter.circle_stroke(center, r + 5.0 * s, Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus)));
        painter.circle_stroke(center, r + 10.0 * s, Stroke::new(4.0 * s, Color32::from_white_alpha((40.0 * focus) as u8)));
    }
}

pub(super) fn draw_game_tile(ui: &Ui, game: &Game, rect: Rect, radius: u8) {
    if let Some(cover) = game.asset(AssetKind::Cover) {
        let placement = game.placement(AssetKind::Cover);
        if placement.zoom < 1.0 {
            ui.painter().rect_filled(rect, radius, placeholder_color(&game.name).gamma_multiply(0.5));
        }
        paint_image_placed(ui, cover, rect, 1.0, radius, placement);
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, radius, placeholder_color(&game.name));
    painter.text(rect.center(), Align2::CENTER_CENTER, initials(&game.name), FontId::proportional(rect.height() * 0.3), Color32::WHITE);
}

pub(super) fn object_floor(rect: Rect) -> (Pos2, Vec2) {
    (pos2(rect.center().x, rect.bottom() - rect.height() * 0.08), vec2(rect.width() * 0.36, rect.height() * 0.06))
}

pub(super) fn placeholder_color(name: &str) -> Color32 {
    let hue = name.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 360;
    egui::ecolor::Hsva::new(hue as f32 / 360.0, 0.5, 0.45, 1.0).into()
}

pub(super) fn initials(name: &str) -> String {
    let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(3).collect();
    initials.to_uppercase()
}

pub(super) struct BoxShape {
    pub(super) front: Rect,
    pub(super) depth: Vec2,
}

impl BoxShape {
    pub(super) const RECEDE: Vec2 = vec2(-0.8, -0.6);

    pub(super) fn fit(rect: Rect, [w, h, d]: [f32; 3]) -> Self {
        let (aspect, depth_ratio) = (w / h, d / h);
        let extra = -Self::RECEDE * depth_ratio;
        let h = (rect.height() / (1.0 + extra.y)).min(rect.width() / (aspect + extra.x));
        let depth = Self::RECEDE * depth_ratio * h;
        let size = vec2(h * aspect, h);
        let corner = rect.center() - (size - depth) / 2.0;
        BoxShape { front: Rect::from_min_size(corner - depth, size), depth }
    }

    pub(super) fn spine(&self) -> [Pos2; 4] {
        let f = self.front;
        [f.left_top() + self.depth, f.left_top(), f.left_bottom(), f.left_bottom() + self.depth]
    }

    pub(super) fn top(&self) -> [Pos2; 4] {
        let f = self.front;
        [f.left_top() + self.depth, f.right_top() + self.depth, f.right_top(), f.left_top()]
    }

    pub(super) fn outline(&self) -> [Pos2; 6] {
        let (f, d) = (self.front, self.depth);
        [f.left_bottom(), f.left_bottom() + d, f.left_top() + d, f.right_top() + d, f.right_top(), f.right_bottom()]
    }
}

impl App {
    pub(super) fn paint_tile(&self, ui: &Ui, i: usize, rect: Rect, radius: u8, s: f32, focus: f32) {
        self.paint_running_glow(ui, i, rect, s);
        self.paint_tile_art(ui, i, rect, radius, s, focus);
        self.paint_artwork_badge(ui, i, rect, s);
        self.paint_app_badges(ui, i, rect, s);
        self.paint_swap_mark(ui, i, rect, radius, s);
    }

    /// Outlines the game being moved while another slot is picked for it.
    fn paint_swap_mark(&self, ui: &Ui, i: usize, rect: Rect, radius: u8, s: f32) {
        let Mode::SwapTile { from } = self.mode else { return };
        if i != from {
            return;
        }
        let pulse = 0.6 + 0.4 * (ui.input(|inp| inp.time) as f32 * 4.0).sin();
        ui.painter().rect_stroke(rect.expand(4.0 * s), radius, Stroke::new(4.0 * s, SWAP_GLOW.gamma_multiply(pulse)), StrokeKind::Outside);
    }

    pub(super) fn paint_tile_art(&self, ui: &Ui, i: usize, rect: Rect, radius: u8, s: f32, focus: f32) {
        match self.theme().settings.tile_style {
            TileStyle::Flat | TileStyle::Cover => self.paint_flat(ui, i, rect, radius, s, focus),
            TileStyle::FlatBox => self.paint_box(ui, i, rect, s, focus),
            TileStyle::Model => self.paint_model(ui, i, rect, s, focus),
            TileStyle::Object => self.paint_object(ui, i, rect, s, focus),
            TileStyle::Disc => self.paint_disc(ui, i, rect, s, focus),
            TileStyle::Case => self.paint_case(ui, i, rect, s, focus),
            TileStyle::Crt => self.paint_crt(ui, i, rect, s, focus),
        }
    }

    pub(super) fn paint_artwork_badge(&self, ui: &Ui, i: usize, rect: Rect, s: f32) {
        let Some(status) = self.library.games.get(i).and_then(|g| self.artwork.status(&g.id)) else { return };
        let r = (rect.width() * 0.09).clamp(11.0 * s, 20.0 * s);
        let center = rect.right_top() + vec2(-r - 6.0 * s, r + 6.0 * s);
        let painter = ui.painter();
        let glyph = |fill: Color32, text: &str| {
            painter.circle_filled(center, r, fill);
            painter.text(center, Align2::CENTER_CENTER, text, FontId::proportional(r * 1.2), Color32::WHITE);
        };
        match status {
            metadata::Status::Queued | metadata::Status::Fetching => {
                painter.circle_filled(center, r, Color32::from_black_alpha(190));
                let color = if matches!(status, metadata::Status::Fetching) { Color32::WHITE } else { Color32::from_gray(140) };
                let size = r * 1.3;
                egui::Spinner::new().size(size).color(color).paint_at(ui, Rect::from_center_size(center, vec2(size, size)));
            }
            metadata::Status::Done(at, _) if at.elapsed() < metadata::DONE_BADGE => glyph(PAD_A, "✔"),
            metadata::Status::Done(..) => {}
            metadata::Status::Failed(_) => glyph(PAD_B, "!"),
        }
    }

    pub(super) fn paint_flat(&self, ui: &Ui, i: usize, rect: Rect, radius: u8, s: f32, focus: f32) {
        let painter = ui.painter();
        painter.rect_filled(rect.translate(vec2(0.0, 8.0 * s)), radius, Color32::from_black_alpha(90));
        match self.library.games.get(i) {
            Some(game) => draw_game_tile(ui, game, rect, radius),
            None => {
                painter.rect_filled(rect, radius, Color32::from_white_alpha(28));
                self.paint_special_mark(painter, self.special(i), rect.center(), rect.height() * 0.4);
            }
        }
        if i == self.selected {
            let stroke = Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4)));
            painter.rect_stroke(rect, radius, stroke, StrokeKind::Outside);
        }
    }

    pub(super) fn paint_box(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let game = self.library.games.get(i);
        let defaults = &self.theme().settings.boxes;
        let look = game.map_or_else(|| defaults.clone(), |g| defaults.with(&g.box_style));
        let b = BoxShape::fit(rect, look.size_mm());
        let painter = ui.painter();
        let outline = b.outline();

        let shadow: Vec<Pos2> = outline.iter().map(|p| *p + vec2(0.0, 8.0 * s)).collect();
        painter.add(Shape::convex_polygon(shadow, Color32::from_black_alpha(90), Stroke::NONE));

        const TOP_SHADE: f32 = 0.9;
        const SPINE_SHADE: f32 = 0.75;
        let face_image = |kind: AssetKind, quad: [Pos2; 4], color: Color32, shade: f32| {
            painter.add(Shape::convex_polygon(quad.to_vec(), box3d::shade(color, shade), Stroke::NONE));
            game.and_then(|g| Some((g.asset(kind)?, g.placement(kind))))
                .is_some_and(|(path, placement)| paint_image_quad(ui, path, quad, box3d::shade(Color32::WHITE, shade), placement))
        };
        face_image(AssetKind::Top, b.top(), look.top_color, TOP_SHADE);
        if !face_image(AssetKind::Spine, b.spine(), look.spine_color, SPINE_SHADE) && look.spine_label == SpineLabel::Name {
            let (name, font) = match game {
                Some(g) => (g.name.as_str(), self.game_font(g)),
                None => (self.special(i).map_or("", Special::name), egui::FontFamily::Proportional),
            };
            paint_spine_text(ui, name, font, look.spine_text_color, &b);
        }
        match game {
            Some(game) => draw_game_tile(ui, game, b.front, 0),
            None => {
                painter.rect_filled(b.front, 0, Color32::from_rgb(0x2a, 0x2d, 0x34));
                self.paint_special_mark(painter, self.special(i), b.front.center(), b.front.width() * 0.45);
            }
        }

        let edge = Stroke::new(1.0, Color32::from_black_alpha(110));
        painter.line_segment([b.front.left_top() + b.depth, b.front.left_top()], Stroke::new(1.0, Color32::from_white_alpha(40)));
        painter.line_segment([b.front.left_top(), b.front.left_bottom()], edge);
        painter.line_segment([b.front.left_top(), b.front.right_top()], edge);
        if i == self.selected {
            let stroke = Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4)));
            painter.add(Shape::closed_line(outline.to_vec(), stroke));
        }
    }

    pub(super) fn turn(&self, i: usize, rest: f32) -> f32 {
        self.turn_every(i, rest, SPIN_TIME.as_secs_f32())
    }

    pub(super) fn turn_every(&self, i: usize, rest: f32, period: f32) -> f32 {
        if i != self.selected || matches!(self.mode, Mode::CoverEditor(_)) {
            return rest;
        }
        let (_, since) = self.spin;
        let turns = since.elapsed().as_secs_f32() / period;
        rest - turns.fract() * std::f32::consts::TAU
    }

    pub(super) fn paint_object(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let game = self.library.games.get(i);
        let painter = ui.painter();
        let (floor, radius) = object_floor(rect);
        painter.add(Shape::ellipse_filled(floor, radius, Color32::from_black_alpha(90)));
        if i == self.selected {
            let stroke = Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4)));
            painter.add(Shape::ellipse_stroke(floor, radius, stroke));
        }

        let name = match game {
            Some(g) => g.name.as_str(),
            None => self.special(i).map_or("", Special::glyph),
        };
        let title = object3d::Source::Title { text: name.to_owned(), font: self.title_font_file(game) };
        let mut objects = self.objects.borrow_mut();
        let model = match game.and_then(|g| Some(object3d::Source::File(g.asset(AssetKind::Model)?.to_path_buf(), g.model_up))) {
            Some(file) => objects.model(&file).or_else(|()| objects.model(&title)),
            None => objects.model(&title),
        };
        let model = match model {
            Ok(Some(model)) => model,
            Ok(None) => return,
            Err(()) => {
                painter.text(rect.center(), Align2::CENTER_CENTER, name, FontId::proportional(rect.height() * 0.15), Color32::WHITE);
                return;
            }
        };

        let area = Rect::from_min_max(rect.min, pos2(rect.right(), floor.y + radius.y));
        let ppp = ui.ctx().pixels_per_point();
        let pixels = [area.width(), area.height()].map(|v| ((v * ppp / 8.0).ceil() as usize).max(1) * 8);
        let color = game.and_then(|g| g.box_style.object_color).unwrap_or(self.theme().settings.objects.color);
        let tile = game.map_or_else(|| name.to_owned(), |g| g.id.clone());
        let texture = objects.texture(&tile, &model, pixels, self.turn(i, object3d::REST_YAW), color);
        let shown = Rect::from_center_size(area.center(), vec2(pixels[0] as f32, pixels[1] as f32) / ppp);
        painter.image(texture, shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }

    pub(super) fn title_font_file(&self, game: Option<&Game>) -> Option<PathBuf> {
        let loaded = |f: &&String| self.fonts.contains(f);
        let file = game.and_then(|g| g.title_font.as_ref()).filter(loaded).or(Some(&self.settings.fonts.title).filter(loaded))?;
        Some(fonts::dir().join(file))
    }

    pub(super) fn paint_model(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let (yaw, pitch) = match &self.mode {
            Mode::CoverEditor(editor) if i == self.selected => match editor.kind {
                AssetKind::Back => (box3d::REST_YAW + std::f32::consts::PI, box3d::PITCH),
                AssetKind::Side => (-box3d::REST_YAW, box3d::PITCH),
                AssetKind::Bottom => (box3d::REST_YAW, -box3d::PITCH * 2.0),
                _ => (box3d::REST_YAW, box3d::PITCH),
            },
            _ => (self.turn(i, box3d::REST_YAW), box3d::PITCH),
        };
        let stroke = (i == self.selected).then(|| Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4))));
        self.with_box_art(i, |art, look| box3d::paint(ui, rect, look.size_mm(), (yaw, pitch), art, vec2(0.0, 8.0 * s), stroke));
    }

    pub(super) fn with_box_art<R>(&self, i: usize, f: impl FnOnce(&box3d::Art, &crate::theme::BoxSettings) -> R) -> R {
        let game = self.library.games.get(i);
        let defaults = &self.theme().settings.boxes;
        let look = game.map_or_else(|| defaults.clone(), |g| defaults.with(&g.box_style));

        use box3d::{Fill, Label};
        let placeholder = game.map_or(Color32::from_rgb(0x2a, 0x2d, 0x34), |g| placeholder_color(&g.name));
        let front_text = match game {
            Some(g) => initials(&g.name),
            None => self.special(i).map_or("", Special::glyph).to_owned(),
        };
        let name = match game {
            Some(g) => g.name.as_str(),
            None => self.special(i).map_or("", Special::name),
        };
        let font = game.map_or(egui::FontFamily::Proportional, |g| self.game_font(g));
        let cover = game.and_then(|g| g.asset(AssetKind::Cover));
        let spine = game.and_then(|g| g.asset(AssetKind::Spine));
        let image = |kind: AssetKind, fallback: Color32| match game.and_then(|g| Some((g.asset(kind)?, g.placement(kind)))) {
            Some((path, placement)) if matches!(kind, AssetKind::Cover | AssetKind::Back) => Fill::Cover(path, fallback, placement),
            Some((path, placement)) => Fill::Image(path, fallback, placement),
            None => Fill::Color(fallback),
        };
        let art = box3d::Art {
            front: image(AssetKind::Cover, placeholder),
            front_label: cover.is_none().then(|| Label { text: &front_text, font: font.clone(), color: Color32::WHITE, size: 0.3, down: false }),
            back: image(AssetKind::Back, look.spine_color),
            spine: image(AssetKind::Spine, look.spine_color),
            spine_label: (spine.is_none() && look.spine_label == SpineLabel::Name)
                .then(|| Label { text: name, font: font.clone(), color: look.spine_text_color, size: 0.5, down: true }),
            top: image(AssetKind::Top, look.top_color),
            side: image(AssetKind::Side, look.top_color),
            bottom: image(AssetKind::Bottom, look.top_color),
            plain: look.top_color,
        };
        f(&art, &look)
    }

    pub(super) fn paint_case(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let pose = match &self.mode {
            Mode::CoverEditor(editor) if i == self.selected && editor.kind == AssetKind::Booklet => case3d::Pose::opening(1.0, 0.0),
            Mode::CoverEditor(_) => case3d::Pose::CLOSED,
            _ => {
                let ctx = ui.ctx();
                let progress = ctx.animate_bool_with_time(Id::new(("case-open", i)), i == self.selected, case3d::OPEN_TIME);
                let sway = ctx.input(|input| input.time) as f32 / case3d::SWAY_PERIOD * std::f32::consts::TAU;
                case3d::Pose::opening(progress, sway)
            }
        };
        let stroke = (i == self.selected).then(|| Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4))));
        let game = self.library.games.get(i);
        let placeholder = game.map_or(Color32::from_rgb(0x2a, 0x2d, 0x34), |g| placeholder_color(&g.name));
        let booklet = match game.and_then(|g| Some((g.asset(AssetKind::Booklet).or(g.asset(AssetKind::Cover))?, g))) {
            Some((path, g)) => box3d::Fill::Cover(path, placeholder, g.placement(if g.asset(AssetKind::Booklet).is_some() { AssetKind::Booklet } else { AssetKind::Cover })),
            None => box3d::Fill::Color(placeholder),
        };
        self.with_box_art(i, |case, look| {
            self.with_disc_art(i, |disc| {
                let art = case3d::Art { case, disc, booklet, plastic: look.top_color };
                case3d::paint(ui, rect, look.size_mm(), pose, &art, Some(vec2(0.0, 8.0 * s)), stroke);
            })
        });
    }

    pub(super) fn disc_pose(&self, i: usize) -> disc3d::Pose {
        let discs = &self.theme().settings.discs;
        let turn = if discs.spin { self.turn_every(i, 0.0, 60.0 / discs.spin_speed.max(0.1)) } else { 0.0 };
        disc3d::Pose { tilt: discs.shown_angle().to_radians(), spin: discs.spin_direction, turn }
    }

    pub(super) fn paint_disc(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let stroke = (i == self.selected).then(|| Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4))));
        self.with_disc_art(i, |art| disc3d::paint(ui, rect, self.disc_pose(i), art, vec2(0.0, 8.0 * s), stroke));
    }

    pub(super) fn with_disc_art<R>(&self, i: usize, f: impl FnOnce(&disc3d::Art) -> R) -> R {
        let game = self.library.games.get(i);
        let text = match game {
            Some(g) => initials(&g.name),
            None => self.special(i).map_or("", Special::glyph).to_owned(),
        };
        let placeholder = game.map_or(Color32::from_rgb(0x2a, 0x2d, 0x34), |g| placeholder_color(&g.name));
        let cover = game.and_then(|g| g.asset(AssetKind::Cover));
        let font = game.map_or(egui::FontFamily::Proportional, |g| self.game_font(g));
        let art = disc3d::Art {
            kind: self.theme().settings.discs.kind,
            print: match cover {
                Some(path) => box3d::Fill::Cover(path, placeholder, game.map(|g| g.placement(AssetKind::Cover)).unwrap_or_default()),
                None => box3d::Fill::Color(placeholder),
            },
            label: cover.is_none().then(|| disc3d::Label { text: &text, font, color: Color32::WHITE, size: 0.2 }),
        };
        f(&art)
    }

    pub(super) fn crt_video<'a>(&self, game: &'a Game, selected: bool) -> Option<(&'a Path, video::Format)> {
        let crt = &self.theme().settings.crt;
        match game.asset(AssetKind::Video) {
            Some(path) if selected && crt.play_video => Some((path, video::Format::CRT)),
            _ => game.asset(AssetKind::IdleVideo).filter(|_| crt.idle_video).map(|path| (path, video::Format::CRT_IDLE)),
        }
    }

    pub(super) fn paint_crt(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        const POWER_ON: f32 = 0.55;
        const POP: f32 = 0.45;
        const POP_BACK: f32 = 0.25;
        const STATIC: f32 = 0.35;
        let ctx = ui.ctx();
        let selected = i == self.selected && !matches!(self.mode, Mode::CoverEditor(_));
        let now = ctx.input(|input| input.time);
        let id = Id::new(("crt-switch", i));
        let since = match ctx.data(|d| d.get_temp::<(bool, f64)>(id)) {
            Some((was, at)) if was == selected => (now - at) as f32,
            Some(_) => {
                ctx.data_mut(|d| d.insert_temp(id, (selected, now)));
                0.0
            }
            None => {
                ctx.data_mut(|d| d.insert_temp(id, (selected, now - 60.0)));
                60.0
            }
        };
        let (pop, state) = if selected {
            let x = (since / POP).min(1.0) - 1.0;
            let back = 1.0 + 2.7 * x.powi(3) + 1.7 * x.powi(2);
            (back, crt::State { power: (since / POWER_ON).min(1.0), noise: 0.0, time: now as f32 })
        } else {
            let fade = (1.0 - since / POP_BACK).max(0.0);
            let noise = (1.0 - since / STATIC).max(0.0).powf(0.7) * 0.9;
            (fade * fade, crt::State { power: 1.0, noise, time: now as f32 })
        };
        let lift = rect.height() * 0.05 * pop;
        let rect = Rect::from_center_size(rect.center() - vec2(0.0, lift), rect.size() * (1.0 + 0.07 * pop));

        let game = self.library.games.get(i);
        let video = game.and_then(|g| self.crt_video(g, selected)).and_then(|(path, _)| self.videos.borrow_mut().frame(ctx, path));
        let cover = game.and_then(|g| g.asset(AssetKind::Cover)).and_then(|path| {
            match egui::Image::new(file_uri(path)).load_for_size(ctx, rect.size()) {
                Ok(egui::load::TexturePoll::Ready { texture }) => Some((texture.id, texture.size)),
                _ => None,
            }
        });
        let picture = match video.or(cover) {
            Some((id, size)) => crt::Picture::Texture(id, size),
            None => crt::Picture::Color(game.map_or(Color32::from_rgb(0x1c, 0x22, 0x2c), |g| placeholder_color(&g.name))),
        };
        let label = match (&picture, game) {
            (crt::Picture::Texture(..), _) => None,
            (_, Some(g)) => Some((initials(&g.name), self.game_font(g))),
            (_, None) => Some((self.special(i).map_or("", Special::glyph).to_owned(), egui::FontFamily::Proportional)),
        };
        let crt_look = &self.theme().settings.crt;
        let art = crt::Art { picture, label, filter: crt_look.filter };
        let stroke = (i == self.selected).then(|| Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4))));
        crt::paint(ui, rect, &art, state, vec2(0.0, 8.0 * s + lift * 0.6), stroke);
    }
}

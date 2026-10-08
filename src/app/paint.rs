//! Shared painting helpers: images, gradients, bars, panels and shadowed text.

use super::*;

pub(super) fn paint_checkbox(painter: &Painter, rect: Rect, checked: bool, s: f32) {
    let radius = (4.0 * s) as u8;
    if checked {
        painter.rect_filled(rect, radius, ACCENT);
        painter.text(rect.center(), Align2::CENTER_CENTER, "✔", FontId::proportional(rect.height() * 0.75), Color32::WHITE);
    } else {
        painter.rect_stroke(rect, radius, Stroke::new(2.0 * s, Color32::from_gray(160)), StrokeKind::Inside);
    }
}

/// `toggles` puts an On/Off pill at the end of a row; rows past its end have none.
pub(super) fn side_panel(
    ui: &Ui,
    screen: Rect,
    s: f32,
    title: &str,
    rows: &[(&str, Option<&str>)],
    toggles: &[Option<bool>],
    cursor: &mut usize,
) -> Option<usize> {
    let painter = ui.painter();
    let text_w = |text: &str, size: f32| painter.layout_no_wrap(text.to_owned(), FontId::proportional(size * s), Color32::WHITE).size().x;
    // Grow past the default width so labels fit, but never cover more than 60% of the screen;
    // anything still too long is shrunk to fit below.
    let pill_w = if toggles.iter().any(Option::is_some) { 104.0 * s } else { 0.0 };
    let needed = rows
        .iter()
        .flat_map(|(label, sub)| [text_w(label, 24.0) + pill_w, sub.map_or(0.0, |sub| text_w(sub, 16.0))])
        .fold(text_w(title, 30.0), f32::max)
        + 80.0 * s;
    let width = needed.clamp(520.0 * s, (screen.width() * 0.6).max(520.0 * s));
    let panel = Rect::from_min_max(pos2(screen.right() - width, screen.top()), screen.max);
    let max_text = width - 80.0 * s;
    let fit = |text: &str, size: f32| FontId::proportional(size * s * (max_text / text_w(text, size)).min(1.0));
    painter.rect_filled(screen, 0, Color32::from_black_alpha(80));
    painter.rect_filled(panel, 0, Color32::from_rgba_unmultiplied(10, 16, 32, 240));
    painter.text(pos2(panel.left() + 40.0 * s, panel.top() + 110.0 * s), Align2::LEFT_TOP, title, fit(title, 30.0), Color32::WHITE);

    let row_h = if rows.iter().any(|(_, sub)| sub.is_some()) { 84.0 } else { 60.0 } * s;
    let mut clicked = None;
    for (i, (label, sub)) in rows.iter().enumerate() {
        let row = Rect::from_min_size(pos2(panel.left(), panel.top() + 190.0 * s + i as f32 * (row_h + 6.0 * s)), vec2(panel.width(), row_h));
        let resp = ui.interact(row, Id::new(("panel-row", i)), Sense::click());
        if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
            *cursor = i;
        }
        if resp.clicked() {
            clicked = Some(i);
        }
        let active = i == *cursor;
        if active {
            painter.rect_filled(row, 0, Color32::from_white_alpha(22));
            painter.rect_filled(Rect::from_min_size(row.min, vec2(5.0 * s, row.height())), 0, ACCENT);
        }
        let color = if active { Color32::WHITE } else { Color32::from_gray(180) };
        if let Some(&Some(on)) = toggles.get(i) {
            let pill = Rect::from_center_size(pos2(row.right() - 82.0 * s, row.center().y), vec2(84.0 * s, 32.0 * s));
            painter.rect_filled(pill, (16.0 * s) as u8, if on { ACCENT } else { Color32::from_white_alpha(25) });
            let text = if on { tr!("On") } else { tr!("Off") };
            painter.text(pill.center(), Align2::CENTER_CENTER, text, FontId::proportional(17.0 * s), Color32::WHITE);
        }
        let x = row.left() + 40.0 * s;
        match sub {
            Some(sub) => {
                painter.text(pos2(x, row.center().y - 4.0 * s), Align2::LEFT_BOTTOM, *label, fit(label, 24.0), color);
                painter.text(pos2(x, row.center().y + 4.0 * s), Align2::LEFT_TOP, *sub, fit(sub, 16.0), Color32::from_gray(140));
            }
            None => {
                painter.text(pos2(x, row.center().y), Align2::LEFT_CENTER, *label, fit(label, 24.0), color);
            }
        }
    }
    clicked
}

pub(super) fn paint_image_quad(ui: &Ui, path: &Path, quad: [Pos2; 4], tint: Color32, placement: CoverPlacement) -> bool {
    let size = vec2(quad[0].distance(quad[1]), quad[0].distance(quad[3]));
    let texture = match egui::Image::new(file_uri(path)).load_for_size(ui.ctx(), size) {
        Ok(egui::load::TexturePoll::Ready { texture }) => texture,
        _ => return false,
    };
    let turn = box3d::turns_to_fit(texture.size, size);
    let (part, uv) = box3d::place_turned(texture.size, size, placement, turn);
    if !part.is_positive() {
        return true;
    }
    let at = |p: Pos2| quad[0].lerp(quad[1], p.x).lerp(quad[3].lerp(quad[2], p.x), p.y);
    let corners = [part.left_top(), part.right_top(), part.right_bottom(), part.left_bottom()].map(at);
    let mut mesh = Mesh::with_texture(texture.id);
    for (pos, uv) in corners.into_iter().zip(uv) {
        mesh.vertices.push(egui::epaint::Vertex { pos, uv, color: tint });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
    true
}

pub(super) fn paint_spine_text(ui: &Ui, text: &str, font: egui::FontFamily, color: Color32, b: &BoxShape) {
    let painter = ui.painter();
    let width = b.depth.length();
    let max_len = b.front.height() * 0.88;
    let mut size = width * 0.5;
    let mut galley = painter.layout_no_wrap(text.to_owned(), FontId::new(size, font.clone()), color);
    if galley.size().x > max_len {
        size *= max_len / galley.size().x;
        galley = painter.layout_no_wrap(text.to_owned(), FontId::new(size.max(1.0), font), color);
    }
    let center = b.front.left_center() + b.depth / 2.0;
    let (down, back) = (vec2(0.0, 1.0), b.depth / width);
    let half = galley.size() / 2.0;
    let [w, h] = ui.ctx().fonts(|f| f.font_image_size());
    let uv_scale = vec2(1.0 / w as f32, 1.0 / h as f32);
    let mut mesh = Mesh::default();
    for row in &galley.rows {
        let first = mesh.vertices.len() as u32;
        mesh.indices.extend(row.visuals.mesh.indices.iter().map(|i| i + first));
        mesh.vertices.extend(row.visuals.mesh.vertices.iter().map(|v| {
            let p = row.pos.to_vec2() + v.pos.to_vec2() - half;
            egui::epaint::Vertex {
                pos: center + down * p.x + back * p.y,
                uv: (v.uv.to_vec2() * uv_scale).to_pos2(),
                color: if v.color == Color32::PLACEHOLDER { color } else { v.color },
            }
        }));
    }
    painter.add(Shape::mesh(mesh));
}

/// Paints the whole image inside `rect`, keeping its proportions.
pub(super) fn paint_image_contained(ctx: &Context, painter: &Painter, path: &Path, rect: Rect) {
    let Ok(egui::load::TexturePoll::Ready { texture }) = egui::Image::new(file_uri(path)).load_for_size(ctx, rect.size()) else {
        return;
    };
    let scale = (rect.width() / texture.size.x).min(rect.height() / texture.size.y);
    let fitted = Rect::from_center_size(rect.center(), texture.size * scale);
    painter.image(texture.id, fitted, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
}

pub(super) fn paint_image_cover(ui: &Ui, path: &Path, rect: Rect, alpha: f32, radius: u8) {
    paint_image_placed(ui, path, rect, alpha, radius, CoverPlacement::default());
}

pub(super) fn paint_image_placed(ui: &Ui, path: &Path, rect: Rect, alpha: f32, radius: u8, placement: CoverPlacement) {
    paint_placed(ui, egui::Image::new(file_uri(path)), rect, alpha, radius, placement);
}

pub(super) fn paint_placed(ui: &Ui, image: egui::Image, rect: Rect, alpha: f32, radius: u8, placement: CoverPlacement) {
    let image = image.show_loading_spinner(false).tint(Color32::WHITE.gamma_multiply(alpha)).corner_radius(radius);
    let Some(size) = image.load_for_size(ui.ctx(), rect.size()).ok().and_then(|t| t.size()) else {
        return image.paint_at(ui, rect);
    };
    let (part, uv) = box3d::place(size, rect.size(), placement);
    if part.is_positive() {
        let target = Rect::from_min_max(rect.lerp_inside(part.min.to_vec2()), rect.lerp_inside(part.max.to_vec2()));
        image.uv(uv).paint_at(ui, target);
    }
}

pub(crate) fn file_uri(path: &Path) -> String {
    let p = path.to_string_lossy();
    if p.starts_with('/') { format!("file://{p}") } else { format!("file:///{p}") }
}

pub(super) fn gradient(painter: &Painter, rect: Rect, from: Color32, to: Color32, direction: GradientDirection) {
    let mid = from.lerp_to_gamma(to, 0.5);
    let corners = match direction {
        GradientDirection::Vertical => [from, from, to, to],
        GradientDirection::Horizontal => [from, to, from, to],
        GradientDirection::Diagonal => [from, mid, mid, to],
    };
    let mut mesh = egui::Mesh::default();
    for (pos, color) in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom()].into_iter().zip(corners) {
        mesh.colored_vertex(pos, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(egui::Shape::mesh(mesh));
}

pub(super) fn paint_theme_background(ui: &Ui, theme: &Theme, rect: Rect) {
    let bg = &theme.settings.background;
    let painter = ui.painter();
    match bg.kind {
        BackgroundKind::Color => {
            painter.rect_filled(rect, 0, bg.color);
        }
        BackgroundKind::Gradient => gradient(painter, rect, bg.gradient_from, bg.gradient_to, bg.gradient_direction),
        BackgroundKind::Image => {
            painter.rect_filled(rect, 0, bg.color);
            if let Some(path) = theme.background_image() {
                paint_image_fit(ui, &path, rect, bg.image_fit);
            }
        }
    }
}

pub(super) fn paint_widget_item(painter: &Painter, item: &addons::Item, cell: Rect, s: f32) {
    if !item.layers.is_empty() {
        let side = cell.width().min(cell.height()) - 10.0 * s;
        return paint_dial(painter, &item.layers, Rect::from_center_size(cell.center(), vec2(side, side)));
    }
    let x = cell.center().x;
    let mid = cell.center().y;
    let meter = item.level.map(|l| l.clamp(0.0, 1.0));
    let shift = if meter.is_some() { -4.0 * s } else { 0.0 };
    shadow_text(painter, pos2(x, mid - 18.0 * s + shift), Align2::CENTER_CENTER, &item.label, 15.0 * s, Color32::from_gray(170));
    shadow_text(painter, pos2(x, mid + 6.0 * s + shift), Align2::CENTER_CENTER, &item.value, 24.0 * s, Color32::WHITE);
    if let Some(level) = meter {
        let track = Rect::from_center_size(pos2(x, mid + 28.0 * s), vec2(cell.width() - 30.0 * s, 6.0 * s));
        let color = item.color().unwrap_or(match level {
            ..0.6 => PAD_A,
            ..0.85 => PAD_Y,
            _ => PAD_B,
        });
        painter.rect_filled(track, (3.0 * s) as u8, Color32::from_white_alpha(30));
        painter.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * level, track.height())), (3.0 * s) as u8, color);
    }
}

/// Draws each layer fitted into `square` (keeping its aspect) and turned clockwise about the square's center.
fn paint_dial(painter: &Painter, layers: &[addons::Layer], square: Rect) {
    for layer in layers {
        let Ok(egui::load::TexturePoll::Ready { texture }) = egui::Image::new(file_uri(&layer.image)).load_for_size(painter.ctx(), square.size()) else {
            continue;
        };
        if texture.size.x <= 0.0 || texture.size.y <= 0.0 {
            continue;
        }
        let half = texture.size * (square.width() / texture.size.x).min(square.height() / texture.size.y) / 2.0;
        let turn = egui::emath::Rot2::from_angle(layer.angle.to_radians());
        let mut mesh = Mesh::with_texture(texture.id);
        for (corner, uv) in [(vec2(-1.0, -1.0), pos2(0.0, 0.0)), (vec2(1.0, -1.0), pos2(1.0, 0.0)), (vec2(1.0, 1.0), pos2(1.0, 1.0)), (vec2(-1.0, 1.0), pos2(0.0, 1.0))] {
            mesh.vertices.push(egui::epaint::Vertex { pos: square.center() + turn * (corner * half), uv, color: Color32::WHITE });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(Shape::mesh(mesh));
    }
}

pub(super) fn paint_bar(ui: &Ui, theme: &Theme, slot: BarSlot, rect: Rect) {
    let bar = theme.settings.bar(slot);
    if !bar.enabled {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    match bar.kind {
        BackgroundKind::Color => {
            painter.rect_filled(rect, 0, bar.color);
        }
        BackgroundKind::Gradient => gradient(&painter, rect, bar.gradient_from, bar.gradient_to, bar.gradient_direction),
        BackgroundKind::Image => {
            painter.rect_filled(rect, 0, bar.color);
            if let Some(path) = theme.bar_image(slot) {
                paint_bar_image(ui, &painter, &path, rect, bar);
            }
        }
    }
}

pub(super) fn paint_bar_image(ui: &Ui, painter: &Painter, path: &Path, rect: Rect, bar: &BarStyle) {
    let Ok(egui::load::TexturePoll::Ready { texture }) = egui::Image::new(file_uri(path)).load_for_size(ui.ctx(), rect.size()) else {
        return;
    };
    if texture.size.x <= 0.0 || texture.size.y <= 0.0 {
        return;
    }
    let cover = (rect.width() / texture.size.x).max(rect.height() / texture.size.y);
    let half = texture.size * cover * bar.image_zoom / 2.0;
    let center = rect.center() + vec2(bar.image_x * rect.width(), bar.image_y * rect.height());
    let turn = egui::emath::Rot2::from_angle(bar.image_rotation.to_radians());
    let mut mesh = Mesh::with_texture(texture.id);
    for (corner, uv) in [(vec2(-1.0, -1.0), pos2(0.0, 0.0)), (vec2(1.0, -1.0), pos2(1.0, 0.0)), (vec2(1.0, 1.0), pos2(1.0, 1.0)), (vec2(-1.0, 1.0), pos2(0.0, 1.0))] {
        mesh.vertices.push(egui::epaint::Vertex { pos: center + turn * (corner * half), uv, color: Color32::WHITE });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

pub(super) fn paint_image_fit(ui: &Ui, path: &Path, rect: Rect, fit: ImageFit) {
    if fit == ImageFit::Fill {
        return paint_image_cover(ui, path, rect, 1.0, 0);
    }
    let options = match fit {
        ImageFit::Tile => TextureOptions::LINEAR_REPEAT,
        ImageFit::Mirror => TextureOptions::LINEAR_MIRRORED_REPEAT,
        _ => TextureOptions::LINEAR,
    };
    let image = egui::Image::new(file_uri(path)).show_loading_spinner(false).texture_options(options);
    let Some(texels) = image.load_for_size(ui.ctx(), rect.size()).ok().and_then(|t| t.size()) else { return };
    let natural = texels / ui.ctx().pixels_per_point();
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let (target, uv) = match fit {
        ImageFit::Stretch => (rect, full),
        ImageFit::Fit => {
            let scale = (rect.width() / texels.x).min(rect.height() / texels.y);
            (Rect::from_center_size(rect.center(), texels * scale), full)
        }
        ImageFit::Center => (Rect::from_center_size(rect.center(), natural), full),
        ImageFit::Tile | ImageFit::Mirror => {
            (rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(rect.width() / natural.x, rect.height() / natural.y)))
        }
        ImageFit::Fill => unreachable!(),
    };
    image.uv(uv).paint_at(ui, target);
}

pub(super) fn shadow_text(painter: &Painter, pos: egui::Pos2, align: Align2, text: &str, size: f32, color: Color32) {
    shadow_text_font(painter, pos, align, text, FontId::proportional(size), color);
}

pub(super) fn shadow_text_font(painter: &Painter, pos: egui::Pos2, align: Align2, text: &str, font: FontId, color: Color32) {
    painter.text(pos + vec2(0.0, 2.0), align, text, font.clone(), Color32::from_black_alpha(160));
    painter.text(pos, align, text, font, color);
}


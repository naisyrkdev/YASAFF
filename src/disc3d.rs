
use crate::app::file_uri;
use crate::box3d::{Fill, V3, convex_hull, place, shade, v3};
use crate::library::CoverPlacement;
use crate::theme::{DiscKind, DiscSpin};
use egui::epaint::{Vertex, WHITE_UV};
use egui::{Color32, FontId, Mesh, Pos2, Rect, Shape, Stroke, TextureId, Ui, Vec2, pos2, vec2};
use std::f32::consts::TAU;

const R_OUTER: f32 = 60.0 / 120.0;
const R_HOLE: f32 = 7.5 / 120.0;
const HALF_THICKNESS: f32 = 0.6 / 120.0;
const R_PRINT: (f32, f32) = (11.5 / 120.0, 59.0 / 120.0);
const R_MIRROR: f32 = 16.5 / 120.0;
const R_DATA: (f32, f32) = (22.5 / 120.0, 58.5 / 120.0);

const DISTANCE: f32 = 4.0;
const SEGMENTS: usize = 128;
const RING_STEP: f32 = 0.025;
const LIGHT: V3 = v3(-0.15, 0.3, 0.94);
const LAMPS: [V3; 2] = [v3(-1.3, 1.5, 1.3), v3(1.6, -0.5, 1.1)];
const CLEAR_PLASTIC: Color32 = Color32::from_rgba_premultiplied(60, 62, 66, 70);
const EDGE: Color32 = Color32::from_rgb(0xa8, 0xac, 0xb4);

#[derive(Clone, Copy)]
pub struct Pose {
    pub tilt: f32,
    pub spin: DiscSpin,
    pub turn: f32,
}

pub struct Label<'a> {
    pub text: &'a str,
    pub font: egui::FontFamily,
    pub color: Color32,
    pub size: f32,
    /// A white icon drawn in place of `text`, tinted with `color`.
    pub icon: Option<egui::ImageSource<'static>>,
}

pub struct Art<'a> {
    pub kind: DiscKind,
    pub print: Fill<'a>,
    pub label: Option<Label<'a>>,
}

impl DiscKind {
    fn tint(self) -> [f32; 3] {
        match self {
            DiscKind::Cd => [0.80, 0.82, 0.86],
            DiscKind::Dvd => [0.82, 0.78, 0.70],
            DiscKind::BluRay => [0.52, 0.55, 0.70],
        }
    }

    fn track_pitch(self) -> f32 {
        match self {
            DiscKind::Cd => 1600.0,
            DiscKind::Dvd => 740.0,
            DiscKind::BluRay => 320.0,
        }
    }
}

struct View {
    center: Pos2,
    scale: f32,
    axes: [V3; 3],
    offset: V3,
    distance: f32,
}

#[derive(Clone, Copy)]
pub struct Placement {
    pub center: Pos2,
    pub scale: f32,
    pub axes: [V3; 3],
    pub offset: V3,
    pub distance: f32,
}

impl View {
    fn new(rect: Rect, pose: Pose) -> Self {
        let (st, ct) = pose.tilt.sin_cos();
        let (ss, cs) = pose.turn.sin_cos();
        let spin = |p: V3| match pose.spin {
            DiscSpin::Around | DiscSpin::Flat => v3(p.x * cs - p.y * ss, p.x * ss + p.y * cs, p.z),
            DiscSpin::Flip => v3(p.x * cs + p.z * ss, p.y, -p.x * ss + p.z * cs),
        };
        let tilt = |p: V3| v3(p.x, p.y * ct - p.z * st, p.y * st + p.z * ct);
        let axes = [v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0), v3(0.0, 0.0, 1.0)].map(|a| tilt(spin(a)));
        let scale = rect.width().min(rect.height()) / (2.0 * R_OUTER * 1.18);
        View { center: rect.center(), scale, axes, offset: v3(0.0, 0.0, 0.0), distance: DISTANCE }
    }

    fn rotate(&self, p: V3) -> V3 {
        self.axes[0] * p.x + self.axes[1] * p.y + self.axes[2] * p.z
    }

    fn at(&self, p: V3) -> V3 {
        self.rotate(p) + self.offset
    }

    fn camera(&self) -> V3 {
        v3(0.0, 0.0, self.distance)
    }

    fn project(&self, p: V3) -> Pos2 {
        let r = self.at(p);
        let k = self.scale * self.distance / (self.distance - r.z);
        pos2(self.center.x + r.x * k, self.center.y - r.y * k)
    }

    fn outline(&self) -> Vec<Pos2> {
        let edge = (0..64).flat_map(|i| {
            let (s, c) = (i as f32 / 64.0 * TAU).sin_cos();
            [HALF_THICKNESS, -HALF_THICKNESS].map(|z| self.project(v3(c * R_OUTER, s * R_OUTER, z)))
        });
        convex_hull(&edge.collect::<Vec<_>>())
    }
}

pub fn outline(rect: Rect, pose: Pose) -> Vec<Pos2> {
    View::new(rect, pose).outline()
}

pub fn paint(ui: &Ui, rect: Rect, pose: Pose, art: &Art, shadow: Vec2, stroke: Option<Stroke>) {
    paint_view(ui, &View::new(rect, pose), art, shadow, stroke);
}

pub fn paint_placed(ui: &Ui, placement: &Placement, art: &Art, shadow: Vec2) {
    let Placement { center, scale, axes, offset, distance } = *placement;
    paint_view(ui, &View { center, scale, axes, offset, distance }, art, shadow, None);
}

fn paint_view(ui: &Ui, view: &View, art: &Art, shadow: Vec2, stroke: Option<Stroke>) {
    let painter = ui.painter();
    let hull = view.outline();
    let shadow_points = hull.iter().map(|p| *p + shadow).collect();
    painter.add(Shape::convex_polygon(shadow_points, Color32::from_black_alpha(90), Stroke::NONE));

    paint_edge(ui, view, R_HOLE, -1.0);
    paint_edge(ui, view, R_OUTER, 1.0);
    let normal = view.rotate(v3(0.0, 0.0, 1.0));
    if normal.dot(view.camera() - view.at(v3(0.0, 0.0, HALF_THICKNESS))) > 0.0 {
        let brightness = 0.55 + 0.45 * normal.dot(LIGHT.normalized()).max(0.0);
        paint_label_side(ui, view, art, brightness);
    } else {
        paint_data_side(ui, view, art.kind);
    }
    if let Some(stroke) = stroke {
        painter.add(Shape::closed_line(hull, stroke));
    }
}

fn paint_edge(ui: &Ui, view: &View, r: f32, outward: f32) {
    let light = LIGHT.normalized();
    let mut mesh = Mesh::default();
    for i in 0..SEGMENTS {
        let (a0, a1) = (i as f32 / SEGMENTS as f32 * TAU, (i + 1) as f32 / SEGMENTS as f32 * TAU);
        let (s, c) = ((a0 + a1) / 2.0).sin_cos();
        let normal = view.rotate(v3(c, s, 0.0) * outward);
        if normal.dot(view.camera() - view.at(v3(c * r, s * r, 0.0))) <= 0.0 {
            continue;
        }
        let color = shade(EDGE, 0.5 + 0.5 * normal.dot(light).max(0.0));
        let first = mesh.vertices.len() as u32;
        for a in [a0, a1] {
            let (s, c) = a.sin_cos();
            for z in [HALF_THICKNESS, -HALF_THICKNESS] {
                mesh.vertices.push(Vertex { pos: view.project(v3(c * r, s * r, z)), uv: WHITE_UV, color });
            }
        }
        mesh.add_triangle(first, first + 1, first + 3);
        mesh.add_triangle(first, first + 3, first + 2);
    }
    ui.painter().add(Shape::mesh(mesh));
}

fn ring(view: &View, r0: f32, r1: f32, z: f32, texture: TextureId, vertex: impl Fn(V3) -> (Color32, Pos2)) -> Shape {
    let rings = ((r1 - r0) / RING_STEP).ceil().max(1.0) as usize;
    let mut mesh = Mesh::with_texture(texture);
    for j in 0..=rings {
        let r = r0 + (r1 - r0) * j as f32 / rings as f32;
        for i in 0..=SEGMENTS {
            let (s, c) = (i as f32 / SEGMENTS as f32 * TAU).sin_cos();
            let p = v3(c * r, s * r, z);
            let (color, uv) = vertex(p);
            mesh.vertices.push(Vertex { pos: view.project(p), uv, color });
        }
    }
    let row = (SEGMENTS + 1) as u32;
    for j in 0..rings as u32 {
        for i in 0..SEGMENTS as u32 {
            let k = j * row + i;
            mesh.add_triangle(k, k + 1, k + row + 1);
            mesh.add_triangle(k, k + row + 1, k + row);
        }
    }
    Shape::mesh(mesh)
}

fn plain(view: &View, r0: f32, r1: f32, z: f32, color: Color32) -> Shape {
    ring(view, r0, r1, z, TextureId::default(), |_| (color, WHITE_UV))
}

fn paint_label_side(ui: &Ui, view: &View, art: &Art, brightness: f32) {
    let painter = ui.painter();
    let z = HALF_THICKNESS;
    painter.add(plain(view, R_HOLE, R_PRINT.0, z, CLEAR_PLASTIC));
    painter.add(plain(view, R_PRINT.1, R_OUTER, z, CLEAR_PLASTIC));

    let (path, fallback, placement) = match art.print {
        Fill::Color(color) => (None, color, CoverPlacement::default()),
        Fill::Image(path, color, placement) | Fill::Cover(path, color, placement) => (Some(path), color, placement),
    };
    let fallback = shade(fallback, brightness);
    let diameter = 2.0 * R_OUTER;
    let texture = path.and_then(|path| match egui::Image::new(file_uri(path)).load_for_size(ui.ctx(), Vec2::splat(diameter * view.scale)) {
        Ok(egui::load::TexturePoll::Ready { texture }) => Some(texture),
        _ => None,
    });
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let (part, uv) = match &texture {
        Some(texture) => place(texture.size, Vec2::splat(1.0), placement),
        None => (Rect::NOTHING, whole),
    };
    if part != whole {
        painter.add(plain(view, R_PRINT.0, R_PRINT.1, z, fallback));
    }
    if let Some(texture) = texture.filter(|_| part.is_positive()) {
        let color = shade(Color32::WHITE, brightness);
        painter.add(ring(view, R_PRINT.0, R_PRINT.1, z, texture.id, |p| {
            let at = pos2(p.x / diameter + 0.5, 0.5 - p.y / diameter);
            let t = ((at - part.min) / part.size()).clamp(Vec2::ZERO, Vec2::splat(1.0));
            (if part.contains(at) { color } else { Color32::TRANSPARENT }, uv.lerp_inside(t))
        }));
    }
    if let Some(label) = &art.label {
        paint_label(ui, view, label, brightness);
    }
}

fn paint_label(ui: &Ui, view: &View, label: &Label, brightness: f32) {
    let painter = ui.painter();
    let color = shade(label.color, brightness);
    if let Some(icon) = &label.icon {
        let side = label.size * 2.0 * R_OUTER;
        let center = v3(0.0, (R_PRINT.0 + R_OUTER) / 2.0, HALF_THICKNESS);
        return crate::box3d::paint_icon_quad(ui, icon, side * view.scale, color, |u, v| {
            view.project(center + v3((u - 0.5) * side, (0.5 - v) * side, 0.0))
        });
    }
    let max_len = 0.62 * view.scale;
    let mut size = label.size * 2.0 * R_OUTER * view.scale;
    let mut galley = painter.layout_no_wrap(label.text.to_owned(), FontId::new(size, label.font.clone()), color);
    if galley.size().x > max_len {
        size *= max_len / galley.size().x;
        galley = painter.layout_no_wrap(label.text.to_owned(), FontId::new(size.max(1.0), label.font.clone()), color);
    }
    let center = v3(0.0, (R_PRINT.0 + R_OUTER) / 2.0, HALF_THICKNESS);
    let half = galley.size() / 2.0;
    let [w, h] = ui.ctx().fonts(|f| f.font_image_size());
    let uv_scale = vec2(1.0 / w as f32, 1.0 / h as f32);
    let mut mesh = Mesh::default();
    for row in &galley.rows {
        let first = mesh.vertices.len() as u32;
        mesh.indices.extend(row.visuals.mesh.indices.iter().map(|i| i + first));
        mesh.vertices.extend(row.visuals.mesh.vertices.iter().map(|v| {
            let p = (row.pos.to_vec2() + v.pos.to_vec2() - half) / view.scale;
            Vertex {
                pos: view.project(center + v3(p.x, -p.y, 0.0)),
                uv: (v.uv.to_vec2() * uv_scale).to_pos2(),
                color: if v.color == Color32::PLACEHOLDER { color } else { v.color },
            }
        }));
    }
    painter.add(Shape::mesh(mesh));
}

fn paint_data_side(ui: &Ui, view: &View, kind: DiscKind) {
    let painter = ui.painter();
    let z = -HALF_THICKNESS;
    let normal = view.rotate(v3(0.0, 0.0, -1.0));
    painter.add(plain(view, R_HOLE, R_MIRROR, z, CLEAR_PLASTIC));
    painter.add(ring(view, R_MIRROR, R_DATA.0, z, TextureId::default(), |p| (shine(view, normal, p, kind, false), WHITE_UV)));
    painter.add(ring(view, R_DATA.0, R_DATA.1, z, TextureId::default(), |p| (shine(view, normal, p, kind, true), WHITE_UV)));
    painter.add(plain(view, R_DATA.1, R_OUTER, z, CLEAR_PLASTIC));
}

fn shine(view: &View, normal: V3, p: V3, kind: DiscKind, tracks: bool) -> Color32 {
    let at = view.at(p);
    let to_eye = (view.camera() - at).normalized();
    let reflected = normal * (2.0 * normal.dot(to_eye)) - to_eye;
    let room = 0.2 + 0.55 * (reflected.y * 0.5 + 0.5).clamp(0.0, 1.0);
    let mut rgb = kind.tint().map(|c| c * room);
    let radial = view.rotate(v3(p.x, p.y, 0.0).normalized());
    let along = view.rotate(v3(-p.y, p.x, 0.0).normalized());
    for lamp in LAMPS {
        let to_lamp = (lamp + view.offset - at).normalized();
        let highlight = reflected.dot(to_lamp).max(0.0).powi(60) * 1.2;
        rgb = rgb.map(|c| c + highlight);
        if !tracks {
            continue;
        }
        let sum = to_lamp + to_eye;
        let (s, q) = (sum.dot(radial).abs(), sum.dot(along));
        let strength = (-(q / 0.3).powi(2)).exp();
        for m in 1..=3 {
            let color = spectrum(kind.track_pitch() * s / m as f32);
            for k in 0..3 {
                rgb[k] += color[k] * strength * 0.9 / m as f32;
            }
        }
    }
    let [r, g, b] = rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8);
    Color32::from_rgb(r, g, b)
}

fn spectrum(nm: f32) -> [f32; 3] {
    let rgb = match nm {
        380.0..440.0 => [(440.0 - nm) / 60.0, 0.0, 1.0],
        440.0..490.0 => [0.0, (nm - 440.0) / 50.0, 1.0],
        490.0..510.0 => [0.0, 1.0, (510.0 - nm) / 20.0],
        510.0..580.0 => [(nm - 510.0) / 70.0, 1.0, 0.0],
        580.0..645.0 => [1.0, (645.0 - nm) / 65.0, 0.0],
        645.0..780.0 => [1.0, 0.0, 0.0],
        _ => return [0.0; 3],
    };
    let fade = match nm {
        ..420.0 => 0.3 + 0.7 * (nm - 380.0) / 40.0,
        700.0.. => 0.3 + 0.7 * (780.0 - nm) / 80.0,
        _ => 1.0,
    };
    rgb.map(|c| c * fade)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(tilt: f32, spin: DiscSpin, turn: f32) -> View {
        View::new(Rect::from_min_size(Pos2::ZERO, vec2(200.0, 200.0)), Pose { tilt, spin, turn })
    }

    fn label_side_visible(view: &View) -> bool {
        view.rotate(v3(0.0, 0.0, 1.0)).dot(view.camera() - view.at(v3(0.0, 0.0, HALF_THICKNESS))) > 0.0
    }

    #[test]
    fn flat_disc_is_a_circle_in_its_tile() {
        let hull = view(0.0, DiscSpin::Around, 0.0).outline();
        let center = pos2(100.0, 100.0);
        let radii: Vec<f32> = hull.iter().map(|p| (*p - center).length()).collect();
        let (lo, hi) = radii.iter().fold((f32::MAX, 0.0f32), |(lo, hi), r| (lo.min(*r), hi.max(*r)));
        assert!(hi - lo < 0.5, "{lo}..{hi}");
        assert!(hi < 100.0);
    }

    #[test]
    fn flipping_shows_the_data_side_half_the_time() {
        assert!(label_side_visible(&view(0.0, DiscSpin::Flip, 0.0)));
        assert!(!label_side_visible(&view(0.0, DiscSpin::Flip, TAU / 2.0)));
        assert!(label_side_visible(&view(1.2, DiscSpin::Around, 2.0)));
    }

    #[test]
    fn tilting_squashes_the_disc() {
        let hull = view(60f32.to_radians(), DiscSpin::Around, 0.0).outline();
        let (top, bottom) = hull.iter().fold((f32::MAX, 0.0f32), |(t, b), p| (t.min(p.y), b.max(p.y)));
        let (left, right) = hull.iter().fold((f32::MAX, 0.0f32), |(l, r), p| (l.min(p.x), r.max(p.x)));
        assert!(bottom - top < (right - left) * 0.6);
    }

    #[test]
    fn a_disc_lying_flat_shows_its_label_while_it_spins() {
        for turn in [0.0, 1.0, 2.5, 4.0] {
            let view = view(70f32.to_radians(), DiscSpin::Flat, turn);
            assert!(label_side_visible(&view));
            let hull = view.outline();
            let (top, bottom) = hull.iter().fold((f32::MAX, 0.0f32), |(t, b), p| (t.min(p.y), b.max(p.y)));
            assert!(bottom - top < 100.0);
        }
    }

    #[test]
    fn spectrum_covers_what_the_eye_sees() {
        assert_eq!(spectrum(300.0), [0.0; 3]);
        assert_eq!(spectrum(900.0), [0.0; 3]);
        assert_eq!(spectrum(530.0)[1], 1.0);
        assert_eq!(spectrum(460.0)[2], 1.0);
    }
}

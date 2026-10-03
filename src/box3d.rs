
use crate::app::file_uri;
use crate::library::CoverPlacement;
use egui::epaint::{Vertex, WHITE_UV};
use egui::{Color32, FontId, Mesh, Pos2, Rect, Shape, Stroke, TextureId, Ui, Vec2, pos2, vec2};
use std::ops::{Add, Mul, Sub};
use std::path::Path;

pub const REST_YAW: f32 = 0.5;
const PITCH: f32 = 0.25;
pub(crate) const DISTANCE: f32 = 4.0;
const LIGHT: V3 = v3(-0.15, 0.3, 0.94);
const SUBDIVISIONS: usize = 8;

pub enum Fill<'a> {
    Image(&'a Path, Color32, CoverPlacement),
    Cover(&'a Path, Color32, CoverPlacement),
    Color(Color32),
}

pub struct Label<'a> {
    pub text: &'a str,
    pub font: egui::FontFamily,
    pub color: Color32,
    pub size: f32,
    pub down: bool,
}

pub struct Art<'a> {
    pub front: Fill<'a>,
    pub front_label: Option<Label<'a>>,
    pub back: Fill<'a>,
    pub spine: Fill<'a>,
    pub spine_label: Option<Label<'a>>,
    pub top: Fill<'a>,
    pub side: Fill<'a>,
    pub plain: Color32,
}

#[derive(Clone, Copy)]
pub(crate) struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub(crate) const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, k: f32) -> V3 {
        v3(self.x * k, self.y * k, self.z * k)
    }
}

impl V3 {
    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalized(self) -> V3 {
        self * (1.0 / self.length())
    }
}

pub(crate) struct View {
    half: V3,
    pub center: Pos2,
    pub scale: f32,
    yaw: (f32, f32),
    pitch: (f32, f32),
}

impl View {
    fn new(rect: Rect, size: [f32; 3], yaw: f32) -> Self {
        let [w, h, d] = size;
        let half = v3(w / h / 2.0, 0.5, d / h / 2.0);
        let turning_width = 2.0 * (half.x * half.x + half.z * half.z).sqrt();
        let scale = (rect.height() / 1.15).min(rect.width() / (turning_width * 1.12));
        Self::placed(rect.center(), scale, half, yaw)
    }

    pub(crate) fn placed(center: Pos2, scale: f32, half: V3, yaw: f32) -> Self {
        let (sin_y, cos_y) = yaw.sin_cos();
        let (sin_p, cos_p) = PITCH.sin_cos();
        View { half, center, scale, yaw: (sin_y, cos_y), pitch: (sin_p, cos_p) }
    }

    pub(crate) fn rotate(&self, p: V3) -> V3 {
        let ((sy, cy), (sp, cp)) = (self.yaw, self.pitch);
        let (x, z) = (p.x * cy + p.z * sy, -p.x * sy + p.z * cy);
        v3(x, p.y * cp - z * sp, p.y * sp + z * cp)
    }

    pub(crate) fn project(&self, p: V3) -> Pos2 {
        let r = self.rotate(p);
        let k = self.scale * DISTANCE / (DISTANCE - r.z);
        pos2(self.center.x + r.x * k, self.center.y - r.y * k)
    }

    fn corners(&self) -> [Pos2; 8] {
        let h = self.half;
        let mut out = [Pos2::ZERO; 8];
        for (i, p) in out.iter_mut().enumerate() {
            let pick = |bit: usize, v: f32| if i & bit == 0 { -v } else { v };
            *p = self.project(v3(pick(1, h.x), pick(2, h.y), pick(4, h.z)));
        }
        out
    }
}

pub(crate) struct Face {
    pub corners: [V3; 4],
    pub normal: V3,
}

impl Face {
    fn width(&self) -> f32 {
        (self.corners[1] - self.corners[0]).length()
    }

    fn height(&self) -> f32 {
        (self.corners[3] - self.corners[0]).length()
    }

    fn center(&self) -> V3 {
        (self.corners[0] + self.corners[2]) * 0.5
    }

    pub(crate) fn flipped(&self) -> Face {
        let [lt, rt, rb, lb] = self.corners;
        Face { corners: [rt, lt, lb, rb], normal: self.normal * -1.0 }
    }

    pub(crate) fn lit(&self, view: &View) -> Option<f32> {
        let normal = view.rotate(self.normal);
        let camera = v3(0.0, 0.0, DISTANCE);
        (normal.dot(camera - view.rotate(self.center())) > 0.0).then(|| 0.55 + 0.45 * normal.dot(LIGHT.normalized()).max(0.0))
    }

    fn at(&self, u: f32, v: f32) -> V3 {
        let [lt, rt, rb, lb] = self.corners;
        let top = lt + (rt - lt) * u;
        let bottom = lb + (rb - lb) * u;
        top + (bottom - top) * v
    }
}

fn faces(h: V3) -> [Face; 6] {
    cuboid(h * -1.0, h)
}

pub(crate) fn cuboid(min: V3, max: V3) -> [Face; 6] {
    let ([x0, y0, z0], [x1, y1, z1]) = ([min.x, min.y, min.z], [max.x, max.y, max.z]);
    [
        Face { corners: [v3(x0, y1, z1), v3(x1, y1, z1), v3(x1, y0, z1), v3(x0, y0, z1)], normal: v3(0.0, 0.0, 1.0) },
        Face { corners: [v3(x1, y1, z0), v3(x0, y1, z0), v3(x0, y0, z0), v3(x1, y0, z0)], normal: v3(0.0, 0.0, -1.0) },
        Face { corners: [v3(x0, y1, z0), v3(x0, y1, z1), v3(x0, y0, z1), v3(x0, y0, z0)], normal: v3(-1.0, 0.0, 0.0) },
        Face { corners: [v3(x1, y1, z1), v3(x1, y1, z0), v3(x1, y0, z0), v3(x1, y0, z1)], normal: v3(1.0, 0.0, 0.0) },
        Face { corners: [v3(x0, y1, z0), v3(x1, y1, z0), v3(x1, y1, z1), v3(x0, y1, z1)], normal: v3(0.0, 1.0, 0.0) },
        Face { corners: [v3(x0, y0, z1), v3(x1, y0, z1), v3(x1, y0, z0), v3(x0, y0, z0)], normal: v3(0.0, -1.0, 0.0) },
    ]
}

pub fn outline(rect: Rect, size: [f32; 3], yaw: f32) -> Vec<Pos2> {
    convex_hull(&View::new(rect, size, yaw).corners())
}

pub fn paint(ui: &Ui, rect: Rect, size: [f32; 3], yaw: f32, art: &Art, shadow: Vec2, stroke: Option<Stroke>) {
    let view = View::new(rect, size, yaw);
    let painter = ui.painter();
    let hull = convex_hull(&view.corners());
    let shadow_points = hull.iter().map(|p| *p + shadow).collect();
    painter.add(Shape::convex_polygon(shadow_points, Color32::from_black_alpha(90), Stroke::NONE));

    let plain = Fill::Color(art.plain);
    let fills = [
        (&art.front, art.front_label.as_ref()),
        (&art.back, None),
        (&art.spine, art.spine_label.as_ref()),
        (&art.side, None),
        (&art.top, None),
        (&plain, None),
    ];
    for (face, (fill, label)) in faces(view.half).iter().zip(fills) {
        let Some(brightness) = face.lit(&view) else { continue };
        paint_face(ui, &view, face, fill, brightness);
        if let Some(label) = label {
            paint_label(ui, &view, face, label, brightness);
        }
    }
    if let Some(stroke) = stroke {
        painter.add(Shape::closed_line(hull, stroke));
    }
}

pub(crate) fn paint_face(ui: &Ui, view: &View, face: &Face, fill: &Fill, brightness: f32) {
    let size = vec2(face.width(), face.height());
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let (path, fallback, placement, can_turn) = match fill {
        Fill::Color(color) => return paint_face_part(ui, view, face, whole, TextureId::default(), [WHITE_UV; 4], *color, brightness),
        Fill::Image(path, fallback, placement) => (*path, *fallback, *placement, true),
        Fill::Cover(path, fallback, placement) => (*path, *fallback, *placement, false),
    };
    let image = egui::Image::new(file_uri(path));
    let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(ui.ctx(), size * view.scale) else {
        return paint_face_part(ui, view, face, whole, TextureId::default(), [WHITE_UV; 4], fallback, brightness);
    };
    let turn = can_turn && turns_to_fit(texture.size, size);
    let (part, uv) = place_turned(texture.size, size, placement, turn);
    if part != whole {
        paint_face_part(ui, view, face, whole, TextureId::default(), [WHITE_UV; 4], fallback, brightness);
    }
    if part.is_positive() {
        paint_face_part(ui, view, face, part, texture.id, uv, Color32::WHITE, brightness);
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_face_part(ui: &Ui, view: &View, face: &Face, part: Rect, texture: TextureId, uv: [Pos2; 4], color: Color32, brightness: f32) {
    let color = shade(color, brightness);
    let n = SUBDIVISIONS;
    let mut mesh = Mesh::with_texture(texture);
    for j in 0..=n {
        for i in 0..=n {
            let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
            let top = uv[0].lerp(uv[1], u);
            let bottom = uv[3].lerp(uv[2], u);
            let at = part.lerp_inside(vec2(u, v));
            mesh.vertices.push(Vertex { pos: view.project(face.at(at.x, at.y)), uv: top.lerp(bottom, v), color });
        }
    }
    let row = (n + 1) as u32;
    for j in 0..n as u32 {
        for i in 0..n as u32 {
            let k = j * row + i;
            mesh.add_triangle(k, k + 1, k + row + 1);
            mesh.add_triangle(k, k + row + 1, k + row);
        }
    }
    ui.painter().add(Shape::mesh(mesh));
}

pub fn turns_to_fit(image: Vec2, target: Vec2) -> bool {
    (target.y > target.x) != (image.y > image.x)
}

pub fn place_turned(image: Vec2, target: Vec2, placement: CoverPlacement, turn: bool) -> (Rect, [Pos2; 4]) {
    let image = if turn { vec2(image.y, image.x) } else { image };
    let (part, uv) = place(image, target, placement);
    let corners = [uv.left_top(), uv.right_top(), uv.right_bottom(), uv.left_bottom()];
    (part, if turn { corners.map(|p| pos2(p.y, 1.0 - p.x)) } else { corners })
}

pub fn place(image: Vec2, target: Vec2, placement: CoverPlacement) -> (Rect, Rect) {
    let (image_aspect, target_aspect) = (image.x / image.y, target.x / target.y);
    let filled = if image_aspect > target_aspect { vec2(image_aspect / target_aspect, 1.0) } else { vec2(1.0, target_aspect / image_aspect) };
    let size = filled * placement.zoom;
    let min = pos2((1.0 - size.x) * placement.x, (1.0 - size.y) * placement.y);
    let shown = Rect::from_min_size(min, size).intersect(Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
    let uv = Rect::from_min_max(((shown.min - min) / size).to_pos2(), ((shown.max - min) / size).to_pos2());
    (shown, uv)
}

pub(crate) fn paint_label(ui: &Ui, view: &View, face: &Face, label: &Label, brightness: f32) {
    let [lt, rt, _, lb] = face.corners;
    let (along, across, long, short) = if label.down {
        ((lb - lt).normalized(), (lt - rt).normalized(), face.height(), face.width())
    } else {
        ((rt - lt).normalized(), (lb - lt).normalized(), face.width(), face.height())
    };
    let painter = ui.painter();
    let color = shade(label.color, brightness);
    let max_len = long * 0.88 * view.scale;
    let mut size = label.size * short * view.scale;
    let mut galley = painter.layout_no_wrap(label.text.to_owned(), FontId::new(size, label.font.clone()), color);
    if galley.size().x > max_len {
        size *= max_len / galley.size().x;
        galley = painter.layout_no_wrap(label.text.to_owned(), FontId::new(size.max(1.0), label.font.clone()), color);
    }
    let center = face.center();
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
                pos: view.project(center + along * p.x + across * p.y),
                uv: (v.uv.to_vec2() * uv_scale).to_pos2(),
                color: if v.color == Color32::PLACEHOLDER { color } else { v.color },
            }
        }));
    }
    painter.add(Shape::mesh(mesh));
}

pub fn shade(color: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let f = |c: u8| (c as f32 * factor).min(255.0) as u8;
    Color32::from_rgba_unmultiplied(f(r), f(g), f(b), a)
}

pub(crate) fn convex_hull(points: &[Pos2]) -> Vec<Pos2> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let cross = |o: Pos2, a: Pos2, b: Pos2| (a - o).x * (b - o).y - (a - o).y * (b - o).x;
    let mut hull: Vec<Pos2> = Vec::with_capacity(pts.len() * 2);
    for pass in [pts.clone(), pts.into_iter().rev().collect()] {
        let start = hull.len();
        for p in pass {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_defaults_to_a_centered_crop() {
        let (part, uv) = place(vec2(200.0, 100.0), vec2(1.0, 1.0), CoverPlacement::default());
        assert_eq!(part, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
        assert_eq!(uv, Rect::from_min_max(pos2(0.25, 0.0), pos2(0.75, 1.0)));
    }

    #[test]
    fn placement_zoomed_out_leaves_room_around() {
        let placement = CoverPlacement { zoom: 0.5, x: 0.0, y: 1.0 };
        let (part, uv) = place(vec2(100.0, 100.0), vec2(1.0, 1.0), placement);
        assert_eq!(part, Rect::from_min_max(pos2(0.0, 0.5), pos2(0.5, 1.0)));
        assert_eq!(uv, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
    }

    #[test]
    fn turned_placement_maps_the_left_top_to_the_image_left_bottom() {
        let (part, uv) = place_turned(vec2(200.0, 100.0), vec2(1.0, 2.0), CoverPlacement::default(), true);
        assert_eq!(part, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
        assert_eq!(uv, [pos2(0.0, 1.0), pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0)]);
    }

    #[test]
    fn hull_of_a_square_with_a_point_inside() {
        let pts = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0), pos2(0.5, 0.5)];
        let hull = convex_hull(&pts);
        assert_eq!(hull.len(), 4);
        assert!(!hull.contains(&pos2(0.5, 0.5)));
    }

    #[test]
    fn resting_box_shows_front_spine_and_top() {
        let view = View::new(Rect::from_min_size(Pos2::ZERO, vec2(200.0, 200.0)), [135.0, 170.0, 14.0], REST_YAW);
        let visible: Vec<bool> = faces(view.half).iter().map(|f| f.lit(&view).is_some()).collect();
        assert_eq!(visible, [true, false, true, false, true, false]);
    }
}

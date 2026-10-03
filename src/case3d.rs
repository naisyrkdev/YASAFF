use crate::box3d::{self, Face, Fill, V3, View, convex_hull, cuboid, shade, v3};
use crate::disc3d;
use egui::{Color32, Pos2, Rect, Shape, Stroke, Ui, Vec2, pos2, vec2};
use std::f32::consts::{PI, TAU};

const OPEN_ANGLE: f32 = 0.84 * PI;
const OPEN_YAW: f32 = 0.12;
const SWAY: f32 = 0.09;
pub const SWAY_PERIOD: f32 = 9.0;
pub const OPEN_TIME: f32 = 0.9;
const SPREAD: f32 = 0.65;

const PLASTIC_MM: f32 = 1.5;
const DISC_MM: f32 = 120.0;
const DISC_LIFT_MM: f32 = 1.5;
const HUB_MM: f32 = 11.0;
const HUB_RING_MM: f32 = 17.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub yaw: f32,
    pub open: f32,
}

impl Pose {
    pub const CLOSED: Pose = Pose { yaw: box3d::REST_YAW, open: 0.0 };

    pub fn opening(progress: f32, sway: f32) -> Pose {
        let p = progress.clamp(0.0, 1.0);
        let turn = smoothstep(0.0, 0.55, p);
        let open = smoothstep(0.3, 1.0, p);
        let yaw = box3d::REST_YAW + (OPEN_YAW - box3d::REST_YAW) * turn + SWAY * sway.sin() * open;
        Pose { yaw, open }
    }
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub struct Art<'a> {
    pub case: &'a box3d::Art<'a>,
    pub disc: &'a disc3d::Art<'a>,
    pub booklet: Fill<'a>,
    pub plastic: Color32,
}

struct Shape3 {
    half: V3,
    plastic: f32,
    disc: f32,
    angle: f32,
    shift: f32,
}

impl Shape3 {
    fn new(size: [f32; 3], open: f32) -> Self {
        let [w, h, d] = size;
        let half = v3(w / h / 2.0, 0.5, d / h / 2.0);
        let plastic = (PLASTIC_MM / h).min(half.z * 0.4);
        let disc = (DISC_MM / h).min(0.94 * (2.0 * half.x).min(1.0));
        let mut shape = Shape3 { half, plastic, disc, angle: OPEN_ANGLE * open, shift: 0.0 };
        let xs = shape.footprint().map(|p| p.x);
        let (min, max) = xs.iter().fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        shape.shift = -(min + max) / 2.0;
        shape
    }

    fn at(&self, p: V3) -> V3 {
        p + v3(self.shift, 0.0, 0.0)
    }

    fn cover(&self, u: f32, y: f32, w: f32) -> V3 {
        let h = self.half;
        let (s, c) = self.angle.sin_cos();
        self.at(v3(-h.x + c * u - s * w, y, h.z + s * u + c * w))
    }

    fn footprint(&self) -> [V3; 6] {
        let (h, width, t) = (self.half, 2.0 * self.half.x, self.plastic);
        [
            self.at(v3(-h.x, 0.0, -h.z)),
            self.at(v3(h.x, 0.0, -h.z)),
            self.at(v3(-h.x, 0.0, h.z)),
            self.at(v3(h.x, 0.0, h.z)),
            self.cover(width, 0.0, 0.0),
            self.cover(width, 0.0, -t),
        ]
    }

    fn corners(&self) -> Vec<V3> {
        let (h, width, t) = (self.half, 2.0 * self.half.x, self.plastic);
        let mut out = Vec::with_capacity(16);
        for y in [-h.y, h.y] {
            for (x, z) in [(-h.x, -h.z), (h.x, -h.z), (-h.x, h.z), (h.x, h.z)] {
                out.push(self.at(v3(x, y, z)));
            }
            for (u, w) in [(0.0, 0.0), (width, 0.0), (0.0, -t), (width, -t)] {
                out.push(self.cover(u, y, w));
            }
        }
        out
    }

    fn tray(&self) -> [Face; 5] {
        let [_, back, spine, side, top, bottom] = self.shifted(cuboid(self.half * -1.0, self.half));
        [back, spine, side, top, bottom]
    }

    fn cavity(&self) -> (Face, [Face; 4]) {
        let (h, t) = (self.half, self.plastic);
        let [_, back, spine, side, top, bottom] = self.shifted(cuboid(v3(-h.x + t, -h.y + t, -h.z + t), v3(h.x - t, h.y - t, h.z)));
        (back.flipped(), [spine.flipped(), side.flipped(), top.flipped(), bottom.flipped()])
    }

    fn rim(&self) -> [Face; 4] {
        let (h, t) = (self.half, self.plastic);
        let (x0, x1, y0, y1) = (-h.x, h.x, -h.y, h.y);
        let p = |x: f32, y: f32| self.at(v3(x, y, h.z));
        let front = v3(0.0, 0.0, 1.0);
        [
            Face { corners: [p(x0, y1), p(x1, y1), p(x1 - t, y1 - t), p(x0 + t, y1 - t)], normal: front },
            Face { corners: [p(x0 + t, y0 + t), p(x1 - t, y0 + t), p(x1, y0), p(x0, y0)], normal: front },
            Face { corners: [p(x0, y1), p(x0 + t, y1 - t), p(x0 + t, y0 + t), p(x0, y0)], normal: front },
            Face { corners: [p(x1 - t, y1 - t), p(x1, y1), p(x1, y0), p(x1 - t, y0 + t)], normal: front },
        ]
    }

    fn lid(&self) -> [Face; 6] {
        let (h, width, t) = (self.half, 2.0 * self.half.x, self.plastic);
        let (s, c) = self.angle.sin_cos();
        let p = |u: f32, y: f32, w: f32| self.cover(u, y, w);
        let (top, bottom) = (h.y, -h.y);
        [
            Face { corners: [p(0.0, top, 0.0), p(width, top, 0.0), p(width, bottom, 0.0), p(0.0, bottom, 0.0)], normal: v3(-s, 0.0, c) },
            Face { corners: [p(width, top, -t), p(0.0, top, -t), p(0.0, bottom, -t), p(width, bottom, -t)], normal: v3(s, 0.0, -c) },
            Face { corners: [p(0.0, top, -t), p(width, top, -t), p(width, top, 0.0), p(0.0, top, 0.0)], normal: v3(0.0, 1.0, 0.0) },
            Face { corners: [p(0.0, bottom, 0.0), p(width, bottom, 0.0), p(width, bottom, -t), p(0.0, bottom, -t)], normal: v3(0.0, -1.0, 0.0) },
            Face { corners: [p(width, top, 0.0), p(width, top, -t), p(width, bottom, -t), p(width, bottom, 0.0)], normal: v3(c, 0.0, s) },
            Face { corners: [p(0.0, top, -t), p(0.0, top, 0.0), p(0.0, bottom, 0.0), p(0.0, bottom, -t)], normal: v3(-c, 0.0, -s) },
        ]
    }

    fn booklet(&self) -> (Face, [Face; 2]) {
        let (h, width, t) = (self.half, 2.0 * self.half.x, self.plastic);
        let (s, c) = self.angle.sin_cos();
        let normal = v3(s, 0.0, -c);
        let mm = 1.0 / 170.0;
        let (u0, u1) = (width * 0.06, width * 0.94);
        let (y0, y1) = (-h.y + 0.05, h.y - 0.05);
        let quad = |u0: f32, u1: f32, y0: f32, y1: f32, w: f32| {
            let p = |u: f32, y: f32| self.cover(u, y, w);
            Face { corners: [p(u1, y1), p(u0, y1), p(u0, y0), p(u1, y0)], normal }
        };
        let (book_w, clip_w) = (-t - 0.5 * mm, -t - 1.2 * mm);
        let clip = |y: f32| quad(u0 - 2.0 * mm, u0 + 9.0 * mm, y - 4.0 * mm, y + 4.0 * mm, clip_w);
        (quad(u0, u1, y0, y1, book_w), [clip(y1 - 3.0 * mm), clip(y0 + 3.0 * mm)])
    }

    fn disc_center(&self) -> V3 {
        let lift = self.plastic + DISC_LIFT_MM / 170.0;
        self.at(v3(0.0, 0.0, -self.half.z + lift))
    }

    fn shifted(&self, faces: [Face; 6]) -> [Face; 6] {
        faces.map(|f| Face { corners: f.corners.map(|p| self.at(p)), normal: f.normal })
    }
}

fn fit(rect: Rect, room_left: f32, size: [f32; 3], pose: Pose) -> (Shape3, View) {
    let shape = Shape3::new(size, pose.open);
    let h = shape.half;
    let unit = View::placed(Pos2::ZERO, 1.0, h, pose.yaw);
    let bounds = Rect::from_points(&shape.corners().iter().map(|p| unit.project(*p)).collect::<Vec<_>>());
    let turning = 2.0 * (h.x * h.x + h.z * h.z).sqrt();
    let width = turning * 1.12 + (bounds.width() * 1.04 - turning * 1.12) * pose.open;
    let height = 1.15 + (bounds.height() * 1.04 - 1.15) * pose.open;
    let spread = (rect.width() * SPREAD).min(room_left.max(0.0)) * pose.open;
    let area = Rect::from_min_max(pos2(rect.left() - spread, rect.top()), rect.max);
    let scale = (area.height() / height).min(area.width() / width);
    let center = area.center() - bounds.center().to_vec2() * scale * pose.open;
    let view = View::placed(center, scale, h, pose.yaw);
    (shape, view)
}

pub fn outline(rect: Rect, size: [f32; 3], pose: Pose) -> Vec<Pos2> {
    let (shape, view) = fit(rect, 0.0, size, pose);
    convex_hull(&shape.corners().iter().map(|p| view.project(*p)).collect::<Vec<_>>())
}

pub fn paint(ui: &Ui, rect: Rect, size: [f32; 3], pose: Pose, art: &Art, shadow: Option<Vec2>, stroke: Option<Stroke>) {
    let room_left = rect.left() - ui.clip_rect().left() - 16.0;
    let (shape, view) = fit(rect, room_left, size, pose);
    let painter = ui.painter();
    let hull = convex_hull(&shape.corners().iter().map(|p| view.project(*p)).collect::<Vec<_>>());
    if let Some(shadow) = shadow {
        let shadow_points = hull.iter().map(|p| *p + shadow).collect();
        painter.add(Shape::convex_polygon(shadow_points, Color32::from_black_alpha(90), Stroke::NONE));
    }

    let opened = shape.angle > 0.01;
    if opened {
        paint_inside(ui, &shape, &view, art);
    }

    let case = &art.case;
    let plain = Fill::Color(case.plain);
    let tray_fills = [(&case.back, None), (&case.spine, case.spine_label.as_ref()), (&case.side, None), (&case.top, None), (&plain, None)];
    for (face, (fill, label)) in shape.tray().iter().zip(tray_fills) {
        let Some(brightness) = face.lit(&view) else { continue };
        box3d::paint_face(ui, &view, face, fill, brightness);
        if let Some(label) = label {
            box3d::paint_label(ui, &view, face, label, brightness);
        }
    }
    if opened {
        for face in shape.rim() {
            if let Some(brightness) = face.lit(&view) {
                box3d::paint_face(ui, &view, &face, &Fill::Color(shade(art.plastic, 1.15)), brightness);
            }
        }
    }

    let [outside, inside, edges @ ..] = shape.lid();
    if let Some(brightness) = outside.lit(&view) {
        box3d::paint_face(ui, &view, &outside, &case.front, brightness);
        if let Some(label) = &case.front_label {
            box3d::paint_label(ui, &view, &outside, label, brightness);
        }
    }
    if opened {
        if let Some(brightness) = inside.lit(&view) {
            box3d::paint_face(ui, &view, &inside, &Fill::Color(art.plastic), brightness);
            let (booklet, clips) = shape.booklet();
            box3d::paint_face(ui, &view, &booklet, &art.booklet, brightness * 0.92);
            for clip in &clips {
                box3d::paint_face(ui, &view, clip, &Fill::Color(shade(art.plastic, 1.3)), brightness);
            }
        }
        for edge in &edges {
            if let Some(brightness) = edge.lit(&view) {
                box3d::paint_face(ui, &view, edge, &Fill::Color(case.plain), brightness);
            }
        }
    }

    if let Some(stroke) = stroke {
        let stroke = Stroke::new(stroke.width, stroke.color.gamma_multiply(1.0 - pose.open));
        if stroke.color.a() > 0 {
            painter.add(Shape::closed_line(hull, stroke));
        }
    }
}

fn paint_inside(ui: &Ui, shape: &Shape3, view: &View, art: &Art) {
    let painter = ui.painter();
    let (floor, walls) = shape.cavity();
    let floor_light = floor.lit(view).unwrap_or(0.55);
    box3d::paint_face(ui, view, &floor, &Fill::Color(shade(art.plastic, 0.8)), floor_light);

    let center = shape.disc_center();
    let floor_z = floor.corners[0].z;
    let circle = |r: f32, color: Color32| {
        let points = (0..48)
            .map(|i| {
                let (s, c) = (i as f32 / 48.0 * TAU).sin_cos();
                view.project(v3(center.x + c * r, center.y + s * r, floor_z + 0.001))
            })
            .collect();
        Shape::convex_polygon(points, shade(color, floor_light), Stroke::NONE)
    };
    painter.add(circle(HUB_RING_MM / 170.0, shade(art.plastic, 0.65)));
    painter.add(circle(HUB_MM / 170.0, shade(art.plastic, 1.35)));
    painter.add(circle(HUB_MM / 170.0 * 0.45, shade(art.plastic, 0.9)));

    for wall in &walls {
        if let Some(brightness) = wall.lit(view) {
            box3d::paint_face(ui, view, wall, &Fill::Color(shade(art.plastic, 0.9)), brightness);
        }
    }

    let placement = disc_placement(view, center, shape.disc);
    disc3d::paint_placed(ui, &placement, art.disc, vec2(0.0, 0.01 * view.scale));
}

fn disc_placement(view: &View, center: V3, diameter: f32) -> disc3d::Placement {
    disc3d::Placement {
        center: view.center,
        scale: view.scale * diameter,
        axes: [v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0), v3(0.0, 0.0, 1.0)].map(|a| view.rotate(a)),
        offset: view.rotate(center) * (1.0 / diameter),
        distance: box3d::DISTANCE / diameter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLU_RAY: [f32; 3] = [135.0, 170.0, 14.0];

    fn tile() -> Rect {
        Rect::from_min_size(pos2(300.0, 0.0), vec2(200.0, 200.0))
    }

    #[test]
    fn closed_case_is_the_model_box() {
        let case = outline(tile(), BLU_RAY, Pose::CLOSED);
        let model = box3d::outline(tile(), BLU_RAY, box3d::REST_YAW);
        let near = |p: &Pos2, hull: &[Pos2]| hull.iter().any(|q| (*p - *q).length() < 0.01);
        assert!(model.iter().all(|p| near(p, &case)), "{case:?} {model:?}");
        assert_eq!(Rect::from_points(&case), Rect::from_points(&model));
    }

    #[test]
    fn opening_turns_first_then_opens_and_closes_back() {
        assert_eq!(Pose::opening(0.0, 1.0), Pose::CLOSED);
        let early = Pose::opening(0.25, 0.0);
        assert!(early.open == 0.0 && early.yaw < box3d::REST_YAW);
        let open = Pose::opening(1.0, 0.0);
        assert!((open.yaw - OPEN_YAW).abs() < 1e-6 && open.open == 1.0);
    }

    #[test]
    fn open_case_shows_the_inside_of_the_cover_and_the_tray() {
        let (shape, view) = fit(tile(), 1000.0, BLU_RAY, Pose::opening(1.0, 0.0));
        let [outside, inside, ..] = shape.lid();
        assert!(outside.lit(&view).is_none());
        assert!(inside.lit(&view).is_some());
        assert!(shape.cavity().0.lit(&view).is_some());
        let (booklet, _) = shape.booklet();
        assert!(booklet.lit(&view).is_some());
    }

    #[test]
    fn open_case_spreads_left_but_stays_in_its_room() {
        let hull = {
            let (shape, view) = fit(tile(), 1000.0, BLU_RAY, Pose::opening(1.0, 0.0));
            convex_hull(&shape.corners().iter().map(|p| view.project(*p)).collect::<Vec<_>>())
        };
        let (left, right) = hull.iter().fold((f32::MAX, f32::MIN), |(l, r), p| (l.min(p.x), r.max(p.x)));
        assert!(left < tile().left(), "spreads past the tile: {left}");
        assert!(left > tile().left() - SPREAD * tile().width(), "{left}");
        assert!(right < tile().right(), "{right}");

        let (shape, view) = fit(tile(), 0.0, BLU_RAY, Pose::opening(1.0, 0.0));
        for p in shape.corners() {
            let x = view.project(p).x;
            assert!(x > tile().left() - 1.0 && x < tile().right() + 1.0, "{x}");
        }
    }

    #[test]
    fn cover_stays_in_front_of_the_tray() {
        for open in [0.2, 0.5, 0.8, 1.0] {
            let shape = Shape3::new(BLU_RAY, open);
            for p in shape.corners().iter().skip(4).take(4) {
                assert!(p.z >= shape.half.z - shape.plastic - 1e-4, "{open}: {}", p.z);
            }
        }
    }

    #[test]
    fn disc_lands_where_the_case_puts_it() {
        let (shape, view) = fit(tile(), 1000.0, BLU_RAY, Pose::opening(1.0, 0.3));
        let center = shape.disc_center();
        let placed = disc_placement(&view, center, shape.disc);
        let k = placed.scale * placed.distance / (placed.distance - placed.offset.z);
        let on_screen = pos2(placed.center.x + placed.offset.x * k, placed.center.y - placed.offset.y * k);
        assert!((on_screen - view.project(center)).length() < 0.01);
        assert!(shape.disc / 2.0 < shape.half.x - shape.plastic);
    }
}

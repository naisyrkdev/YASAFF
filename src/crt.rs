use egui::epaint::{Vertex, WHITE_UV};
use egui::{Align2, Color32, ColorImage, Context, FontFamily, FontId, Id, Mesh, Pos2, Rect, Shape, Stroke, TextureHandle, TextureId, TextureOptions, Ui, Vec2, pos2, vec2};
use std::f32::consts::TAU;

const SEGMENTS: usize = 72;
const RINGS: usize = 10;
const SCREEN_SHAPE: f32 = 3.6;
const BARREL: f32 = 0.14;
const NOISE_SIZE: [usize; 2] = [160, 120];

pub enum Picture {
    Texture(TextureId, Vec2),
    Color(Color32),
}

pub struct Art {
    pub picture: Picture,
    pub label: Option<(String, FontFamily)>,
    pub filter: bool,
}

#[derive(Clone, Copy)]
pub struct State {
    pub power: f32,
    pub noise: f32,
    pub time: f32,
}

struct Shape2 {
    center: Pos2,
    half: Vec2,
    n: f32,
}

impl Shape2 {
    fn unit(&self, rho: f32, theta: f32) -> Vec2 {
        let (sin, cos) = theta.sin_cos();
        let r = (cos.abs().powf(self.n) + sin.abs().powf(self.n)).powf(-1.0 / self.n);
        vec2(cos, sin) * r * rho
    }

    fn at(&self, q: Vec2) -> Pos2 {
        self.center + q * self.half
    }

    fn outline(&self) -> Vec<Pos2> {
        (0..SEGMENTS).map(|k| self.at(self.unit(1.0, k as f32 / SEGMENTS as f32 * TAU))).collect()
    }

    fn half_width(&self, y: f32) -> f32 {
        (1.0 - y.abs().min(1.0).powf(self.n)).max(0.0).powf(1.0 / self.n)
    }

    fn mesh(&self, texture: TextureId, vertex: impl Fn(Vec2) -> (Pos2, Pos2, Color32)) -> Mesh {
        let mut mesh = Mesh::with_texture(texture);
        let mut push = |q: Vec2| {
            let (pos, uv, color) = vertex(q);
            mesh.vertices.push(Vertex { pos, uv, color });
        };
        push(Vec2::ZERO);
        for ring in 1..=RINGS {
            let rho = ring as f32 / RINGS as f32;
            for seg in 0..SEGMENTS {
                push(self.unit(rho, seg as f32 / SEGMENTS as f32 * TAU));
            }
        }
        let at = |ring: usize, seg: usize| (1 + (ring - 1) * SEGMENTS + seg % SEGMENTS) as u32;
        for seg in 0..SEGMENTS {
            mesh.add_triangle(0, at(1, seg), at(1, seg + 1));
            for ring in 1..RINGS {
                mesh.add_triangle(at(ring, seg), at(ring + 1, seg), at(ring + 1, seg + 1));
                mesh.add_triangle(at(ring, seg), at(ring + 1, seg + 1), at(ring, seg + 1));
            }
        }
        mesh
    }
}

fn screen(rect: Rect) -> Shape2 {
    Shape2 { center: rect.center(), half: rect.size() / 2.0, n: SCREEN_SHAPE }
}

pub fn outline(rect: Rect) -> Vec<Pos2> {
    screen(rect).outline()
}

fn scale(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_premultiplied(f(c.r()), f(c.g()), f(c.b()), c.a())
}

fn gray_alpha(v: f32, a: f32) -> Color32 {
    let a = a.clamp(0.0, 1.0);
    let v = (v.clamp(0.0, 1.0) * a * 255.0) as u8;
    Color32::from_rgba_premultiplied(v, v, v, (a * 255.0) as u8)
}

fn barrel(q: Vec2) -> Vec2 {
    vec2(q.x * (1.0 - BARREL * (1.0 - q.y * q.y)), q.y * (1.0 - BARREL * (1.0 - q.x * q.x)))
}

fn cover_uv(size: Vec2, aspect: f32) -> Vec2 {
    let texture = size.x / size.y.max(1.0);
    if texture > aspect { vec2(aspect / texture, 1.0) * 0.5 } else { vec2(1.0, texture / aspect) * 0.5 }
}

fn noise_texture(ctx: &Context) -> TextureHandle {
    let id = Id::new("crt-noise");
    if let Some(texture) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let [w, h] = NOISE_SIZE;
    let mut seed = 0x9e37_79b9u32;
    let pixels = (0..w * h)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let v = (seed >> 24) as u8;
            Color32::from_gray(v)
        })
        .collect();
    let options = TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..TextureOptions::NEAREST };
    let texture = ctx.load_texture("crt-noise", ColorImage::new([w, h], pixels), options);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

fn power_squish(power: f32) -> (f32, f32) {
    const LINE: f32 = 0.012;
    const SPLIT: f32 = 0.3;
    if power >= 1.0 {
        (1.0, 1.0)
    } else if power < SPLIT {
        (ease_out(power / SPLIT), LINE)
    } else {
        (1.0, LINE + (1.0 - LINE) * ease_out((power - SPLIT) / (1.0 - SPLIT)))
    }
}

pub fn paint(ui: &Ui, rect: Rect, art: &Art, state: State, shadow: Vec2, stroke: Option<Stroke>) {
    let painter = ui.painter();
    let screen = screen(rect);
    let outline = screen.outline();

    let soft: Vec<Pos2> = outline.iter().map(|p| *p + shadow * 1.6).collect();
    painter.add(Shape::convex_polygon(soft, Color32::from_black_alpha(40), Stroke::NONE));
    let hard: Vec<Pos2> = outline.iter().map(|p| *p + shadow).collect();
    painter.add(Shape::convex_polygon(hard, Color32::from_black_alpha(90), Stroke::NONE));
    painter.add(Shape::convex_polygon(outline.clone(), Color32::from_rgb(0x07, 0x09, 0x0b), Stroke::NONE));

    let (sx, sy) = power_squish(state.power);
    let squish = vec2(sx, sy);
    let aspect = screen.half.x / screen.half.y;
    let vignette = |q: Vec2| if art.filter { 1.0 - 0.5 * (q.length_sq() / 2.0).powf(1.5) } else { 1.0 };
    let flicker = if art.filter { 0.97 + 0.03 * (state.time * 47.0).sin() } else { 1.0 };
    let (texture, uv_half, base) = match art.picture {
        Picture::Texture(id, size) => (id, cover_uv(size, aspect), Color32::WHITE),
        Picture::Color(color) => (TextureId::default(), Vec2::ZERO, color),
    };
    painter.add(screen.mesh(texture, |q| {
        let uv = if uv_half == Vec2::ZERO { WHITE_UV } else { pos2(0.5, 0.5) + barrel(q) * uv_half };
        (screen.at(q * squish), uv, scale(base, vignette(q) * flicker))
    }));
    if let (Some((text, family)), true) = (&art.label, sy > 0.5) {
        let size = screen.half.y * 0.75 * sy;
        painter.text(screen.center + vec2(2.0, 2.0), Align2::CENTER_CENTER, text, FontId::new(size, family.clone()), Color32::from_black_alpha(120));
        painter.text(screen.center, Align2::CENTER_CENTER, text, FontId::new(size, family.clone()), Color32::from_white_alpha((230.0 * sy) as u8));
    }

    if state.noise > 0.0 {
        let noise = noise_texture(ui.ctx());
        let frame = (state.time * 24.0) as u32;
        let jump = |k: u32| (frame.wrapping_mul(2_654_435_761).wrapping_add(k) >> 8) as f32 / (1u32 << 24) as f32;
        let offset = vec2(jump(0), jump(7_919));
        const SPECK: f32 = 2.0;
        let reach = vec2(screen.half.x / NOISE_SIZE[0] as f32, screen.half.y / NOISE_SIZE[1] as f32) / SPECK;
        let color = gray_alpha(1.0, state.noise);
        painter.add(screen.mesh(noise.id(), |q| (screen.at(q), (q * reach + offset).to_pos2(), color)));
    }

    if art.filter && state.power > 0.0 {
        let spacing = (screen.half.y / 48.0).max(2.0);
        let width = spacing * 0.45;
        let band = (state.time * 0.18).fract() * 2.6 - 1.3;
        let mut y = -sy;
        while y <= sy {
            let half = screen.half_width(y / sy.max(1e-3)) * sx;
            if half > 0.0 {
                let row = |c: Color32| {
                    let [a, b] = [-half, half].map(|x| screen.at(vec2(x, y)));
                    painter.line_segment([a, b], Stroke::new(width, c));
                };
                row(Color32::from_black_alpha(70));
                let glow = (1.0 - ((y - band) / 0.18).abs()).max(0.0);
                if glow > 0.0 {
                    row(Color32::from_white_alpha((14.0 * glow) as u8));
                }
            }
            y += spacing / screen.half.y;
        }
    }

    if state.power < 1.0 {
        let flash = (1.0 - state.power).powi(2);
        painter.add(screen.mesh(TextureId::default(), |q| (screen.at(q * squish), WHITE_UV, gray_alpha(1.0, 0.25 + 0.75 * flash))));
        let glow = Shape2 { center: screen.center, half: screen.half * vec2(sx, (sy * 4.0).min(1.0)), n: SCREEN_SHAPE };
        painter.add(glow.mesh(TextureId::default(), |q| (glow.at(q), WHITE_UV, gray_alpha(1.0, 0.35 * flash * (1.0 - q.length_sq() / 2.0).max(0.0)))));
    }

    let turn = egui::emath::Rot2::from_angle(-0.32);
    let glare = Shape2 { center: screen.at(vec2(-0.42, -0.55)), half: screen.half * vec2(0.55, 0.2), n: 2.0 };
    painter.add(glare.mesh(TextureId::default(), |q| {
        let fade = (1.0 - q.length_sq()).max(0.0).powi(2);
        (glare.center + turn * (q * glare.half), WHITE_UV, Color32::from_white_alpha((42.0 * fade) as u8))
    }));
    painter.add(Shape::closed_line(screen.outline(), Stroke::new(1.0, Color32::from_white_alpha(28))));

    if let Some(stroke) = stroke {
        painter.add(Shape::closed_line(outline, stroke));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barrel_keeps_edge_centers_and_stays_inside() {
        assert!((barrel(vec2(1.0, 1.0)) - vec2(1.0, 1.0)).length() < 1e-6);
        assert!(barrel(vec2(0.5, 0.0)).x < 0.5);
        let s = screen(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0)));
        for k in 0..SEGMENTS {
            let q = barrel(s.unit(1.0, k as f32 / SEGMENTS as f32 * TAU));
            assert!(q.x.abs() <= 1.0 + 1e-4 && q.y.abs() <= 1.0 + 1e-4, "{q:?}");
        }
    }

    #[test]
    fn switching_on_opens_a_line_into_the_picture() {
        assert_eq!(power_squish(1.0), (1.0, 1.0));
        let (w, h) = power_squish(0.1);
        assert!(w < 1.0 && h < 0.05);
        let (w, h) = power_squish(0.6);
        assert!(w == 1.0 && h > 0.05 && h < 1.0);
    }

    #[test]
    fn covers_fill_the_screen() {
        assert_eq!(cover_uv(vec2(100.0, 100.0), 4.0 / 3.0), vec2(1.0, 0.75) * 0.5);
        assert_eq!(cover_uv(vec2(200.0, 100.0), 1.0), vec2(0.5, 1.0) * 0.5);
    }
}

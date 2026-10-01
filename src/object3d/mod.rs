
pub mod formats;
mod text;

use egui::{Color32, ColorImage, Context, TextureHandle, TextureId, TextureOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

pub use formats::EXTS;

pub const REST_YAW: f32 = 0.45;
const PITCH: f32 = 0.3;
const DISTANCE: f32 = 5.0;
const SUPERSAMPLE: usize = 2;
pub const TRIANGLE_BUDGET: usize = 80_000;
const TEXTURE_TTL: Duration = Duration::from_secs(10);

#[derive(Default, Clone, Debug)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[u8; 4]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    fn push(&mut self, p: [f32; 3], color: Option<[u8; 4]>) -> u32 {
        if let Some(c) = color {
            if self.colors.is_empty() {
                self.colors.resize(self.positions.len(), [0; 4]);
            }
            self.colors.push(c);
        } else if !self.colors.is_empty() {
            self.colors.push([0; 4]);
        }
        self.positions.push(p);
        (self.positions.len() - 1) as u32
    }

    fn z_up_to_y_up(&mut self) {
        for p in &mut self.positions {
            *p = [p[0], p[2], -p[1]];
        }
    }

    fn clean(&mut self) {
        let n = self.positions.len() as u32;
        let finite: Vec<bool> = self.positions.iter().map(|p| p.iter().all(|c| c.is_finite())).collect();
        self.triangles.retain(|t| t.iter().all(|&i| i < n && finite[i as usize]));
    }

    fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut used = self.triangles.iter().flatten().map(|&i| self.positions[i as usize]);
        let first = used.next()?;
        Some(used.fold((first, first), |(lo, hi), p| {
            (std::array::from_fn(|k| lo[k].min(p[k])), std::array::from_fn(|k| hi[k].max(p[k])))
        }))
    }
}

pub fn simplify(mesh: &Mesh, budget: usize) -> Mesh {
    let Some((lo, hi)) = mesh.bounds() else { return Mesh::default() };
    let extent = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max).max(f32::MIN_POSITIVE);
    let mut cells = 1024.0f32;
    let mut out = cluster(mesh, lo, extent / cells);
    while out.triangles.len() > budget && cells > 8.0 {
        cells *= (budget as f32 / out.triangles.len() as f32).sqrt().min(0.9) * 0.97;
        out = cluster(&out, lo, extent / cells);
    }
    out
}

fn cluster(mesh: &Mesh, origin: [f32; 3], cell: f32) -> Mesh {
    struct Sum {
        pos: [f64; 3],
        count: u32,
        color: [u32; 3],
        colored: u32,
    }
    let mut ids: HashMap<[i32; 3], u32> = HashMap::new();
    let mut sums: Vec<Sum> = Vec::new();
    let mut remap = vec![u32::MAX; mesh.positions.len()];
    for &i in mesh.triangles.iter().flatten() {
        let i = i as usize;
        if remap[i] != u32::MAX {
            continue;
        }
        let p = mesh.positions[i];
        let key = std::array::from_fn(|k| ((p[k] - origin[k]) / cell).floor() as i32);
        let id = *ids.entry(key).or_insert_with(|| {
            sums.push(Sum { pos: [0.0; 3], count: 0, color: [0; 3], colored: 0 });
            (sums.len() - 1) as u32
        });
        let sum = &mut sums[id as usize];
        for (s, v) in sum.pos.iter_mut().zip(p) {
            *s += v as f64;
        }
        sum.count += 1;
        if let Some(c) = mesh.colors.get(i).filter(|c| c[3] > 0) {
            for (s, v) in sum.color.iter_mut().zip(c) {
                *s += *v as u32;
            }
            sum.colored += 1;
        }
        remap[i] = id;
    }
    let has_colors = !mesh.colors.is_empty();
    let mut out = Mesh::default();
    for s in &sums {
        out.positions.push(std::array::from_fn(|k| (s.pos[k] / s.count as f64) as f32));
        if has_colors {
            out.colors.push(if s.colored * 2 >= s.count {
                let c: [u8; 3] = std::array::from_fn(|k| (s.color[k] / s.colored) as u8);
                [c[0], c[1], c[2], 255]
            } else {
                [0; 4]
            });
        }
    }
    let mut seen = std::collections::HashSet::new();
    for t in &mesh.triangles {
        let t = t.map(|i| remap[i as usize]);
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            continue;
        }
        let mut key = t;
        key.sort_unstable();
        if seen.insert(key) {
            out.triangles.push(t);
        }
    }
    out
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelUp {
    #[default]
    AsIs,
    Front,
    Back,
    Left,
    Right,
    Bottom,
}

impl ModelUp {
    pub const ALL: [Self; 6] = [Self::AsIs, Self::Front, Self::Back, Self::Left, Self::Right, Self::Bottom];

    pub fn label(self) -> &'static str {
        match self {
            Self::AsIs => tr!("As imported"),
            Self::Front => tr!("Front side up"),
            Self::Back => tr!("Back side up"),
            Self::Left => tr!("Left side up"),
            Self::Right => tr!("Right side up"),
            Self::Bottom => tr!("Upside down"),
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Self::AsIs
    }

    fn apply(self, [x, y, z]: [f32; 3]) -> [f32; 3] {
        match self {
            Self::AsIs => [x, y, z],
            Self::Front => [x, z, -y],
            Self::Back => [x, -z, y],
            Self::Left => [y, -x, z],
            Self::Right => [-y, x, z],
            Self::Bottom => [x, -y, -z],
        }
    }
}

pub struct Model {
    positions: Vec<[f32; 3]>,
    colors: Vec<[u8; 4]>,
    triangles: Vec<[u32; 3]>,
    radius: f32,
    half_height: f32,
}

impl Model {
    pub fn new(mesh: Mesh) -> Option<Model> {
        let (lo, hi) = mesh.bounds()?;
        let center: [f32; 3] = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
        let mut positions: Vec<[f32; 3]> = mesh.positions.iter().map(|p| std::array::from_fn(|k| p[k] - center[k])).collect();
        let radius = positions.iter().map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt()).fold(0.0f32, f32::max);
        let half_height = (hi[1] - lo[1]) / 2.0;
        let size = radius.max(half_height);
        if size <= 0.0 || !size.is_finite() {
            return None;
        }
        for p in &mut positions {
            *p = p.map(|c| c / size);
        }
        Some(Model { positions, colors: mesh.colors, triangles: mesh.triangles, radius: radius / size, half_height: half_height / size })
    }
}

pub fn render(model: &Model, size: [usize; 2], yaw: f32, base: Color32) -> ColorImage {
    let (w, h) = (size[0] * SUPERSAMPLE, size[1] * SUPERSAMPLE);
    let (sin_p, cos_p) = PITCH.sin_cos();
    let reach_x = model.radius;
    let reach_y = model.half_height * cos_p + model.radius * sin_p;
    let nearest = (model.radius * model.radius + model.half_height * model.half_height).sqrt();
    let growth = DISTANCE / (DISTANCE - nearest);
    let scale = (w as f32 / (2.0 * reach_x * growth)).min(h as f32 / (2.0 * reach_y * growth)) * 0.97;
    let (sin_y, cos_y) = yaw.sin_cos();
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);

    let camera: Vec<[f32; 3]> = model
        .positions
        .iter()
        .map(|p| {
            let (x, z) = (p[0] * cos_y + p[2] * sin_y, -p[0] * sin_y + p[2] * cos_y);
            [x, p[1] * cos_p - z * sin_p, p[1] * sin_p + z * cos_p]
        })
        .collect();
    let screen: Vec<[f32; 3]> = camera
        .iter()
        .map(|c| {
            let k = scale * DISTANCE / (DISTANCE - c[2]);
            [cx + c[0] * k, cy - c[1] * k, c[2]]
        })
        .collect();

    let key = normalize([-0.35, 0.55, 0.75]);
    let fill = normalize([0.8, -0.1, 0.45]);
    let base = [base.r(), base.g(), base.b()].map(|c| c as f32);
    let mut depth = vec![f32::NEG_INFINITY; w * h];
    let mut color = vec![[0u8; 3]; w * h];

    for t in &model.triangles {
        let [a, b, c] = t.map(|i| camera[i as usize]);
        let mut n = normalize(cross(sub(b, a), sub(c, a)));
        let eye = sub([0.0, 0.0, DISTANCE], a);
        if dot(n, eye) < 0.0 {
            n = n.map(|v| -v);
        }
        let light = 0.3 + 0.62 * dot(n, key).max(0.0) + 0.18 * dot(n, fill).max(0.0);
        let rgb = face_color(model, t, base).map(|v| (v * light).min(255.0) as u8);
        raster(t.map(|i| screen[i as usize]), w, h, &mut depth, &mut color, rgb);
    }

    let mut pixels = Vec::with_capacity(size[0] * size[1]);
    for y in 0..size[1] {
        for x in 0..size[0] {
            let (mut sum, mut covered) = ([0u32; 3], 0u32);
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let i = (y * SUPERSAMPLE + sy) * w + x * SUPERSAMPLE + sx;
                    if depth[i] > f32::NEG_INFINITY {
                        covered += 1;
                        for k in 0..3 {
                            sum[k] += color[i][k] as u32;
                        }
                    }
                }
            }
            let n = (SUPERSAMPLE * SUPERSAMPLE) as u32;
            let [r, g, b] = sum.map(|s| (s / n) as u8);
            pixels.push(Color32::from_rgba_premultiplied(r, g, b, (covered * 255 / n) as u8));
        }
    }
    ColorImage { size, source_size: egui::vec2(size[0] as f32, size[1] as f32), pixels }
}

fn face_color(model: &Model, t: &[u32; 3], base: [f32; 3]) -> [f32; 3] {
    if model.colors.is_empty() {
        return base;
    }
    let (mut sum, mut n) = ([0.0f32; 3], 0.0);
    for &i in t {
        let c = model.colors[i as usize];
        if c[3] > 0 {
            for k in 0..3 {
                sum[k] += c[k] as f32;
            }
            n += 1.0;
        }
    }
    if n == 0.0 { base } else { sum.map(|s| s / n) }
}

fn raster(p: [[f32; 3]; 3], w: usize, h: usize, depth: &mut [f32], color: &mut [[u8; 3]], rgb: [u8; 3]) {
    let edge = |a: [f32; 3], b: [f32; 3], x: f32, y: f32| (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
    let area = edge(p[0], p[1], p[2][0], p[2][1]);
    if area.abs() < 1e-6 {
        return;
    }
    let min_x = p.iter().map(|v| v[0]).fold(f32::INFINITY, f32::min).floor().max(0.0) as usize;
    let max_x = (p.iter().map(|v| v[0]).fold(f32::NEG_INFINITY, f32::max).ceil() as isize).min(w as isize - 1);
    let min_y = p.iter().map(|v| v[1]).fold(f32::INFINITY, f32::min).floor().max(0.0) as usize;
    let max_y = (p.iter().map(|v| v[1]).fold(f32::NEG_INFINITY, f32::max).ceil() as isize).min(h as isize - 1);
    if max_x < min_x as isize || max_y < min_y as isize {
        return;
    }
    let inv = 1.0 / area;
    let step_x = [(p[1][1] - p[2][1]) * inv, (p[2][1] - p[0][1]) * inv, (p[0][1] - p[1][1]) * inv];
    let step_y = [(p[2][0] - p[1][0]) * inv, (p[0][0] - p[2][0]) * inv, (p[1][0] - p[0][0]) * inv];
    let (x0, y0) = (min_x as f32 + 0.5, min_y as f32 + 0.5);
    let mut row = [edge(p[1], p[2], x0, y0) * inv, edge(p[2], p[0], x0, y0) * inv, edge(p[0], p[1], x0, y0) * inv];
    for y in min_y..=max_y as usize {
        let mut b = row;
        for x in min_x..=max_x as usize {
            if b[0] >= 0.0 && b[1] >= 0.0 && b[2] >= 0.0 {
                let z = b[0] * p[0][2] + b[1] * p[1][2] + b[2] * p[2][2];
                let i = y * w + x;
                if z > depth[i] {
                    depth[i] = z;
                    color[i] = rgb;
                }
            }
            for k in 0..3 {
                b[k] += step_x[k];
            }
        }
        for k in 0..3 {
            row[k] += step_y[k];
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len > 0.0 { v.map(|c| c / len) } else { [0.0, 0.0, 1.0] }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Source {
    File(PathBuf, ModelUp),
    Title { text: String, font: Option<PathBuf> },
}

enum Slot {
    Loading,
    Ready(Arc<Model>),
    Failed,
}

struct Rendered {
    texture: TextureHandle,
    shows: (usize, [usize; 2], u32, Color32),
    used: Instant,
}

pub struct Objects {
    ctx: Context,
    models: HashMap<Source, Slot>,
    done_tx: Sender<(Source, Option<Model>)>,
    done_rx: Receiver<(Source, Option<Model>)>,
    textures: HashMap<String, Rendered>,
}

impl Objects {
    pub fn new(ctx: Context) -> Self {
        let (done_tx, done_rx) = mpsc::channel();
        Objects { ctx, models: HashMap::new(), done_tx, done_rx, textures: HashMap::new() }
    }

    pub fn model(&mut self, source: &Source) -> Result<Option<Arc<Model>>, ()> {
        while let Ok((done, model)) = self.done_rx.try_recv() {
            let slot = match model {
                Some(m) => Slot::Ready(Arc::new(m)),
                None => Slot::Failed,
            };
            self.models.insert(done, slot);
        }
        match self.models.get(source) {
            Some(Slot::Ready(m)) => return Ok(Some(m.clone())),
            Some(Slot::Failed) => return Err(()),
            Some(Slot::Loading) => return Ok(None),
            None => {}
        }
        self.models.insert(source.clone(), Slot::Loading);
        let (tx, ctx, source) = (self.done_tx.clone(), self.ctx.clone(), source.clone());
        std::thread::spawn(move || {
            let model = match load(&source) {
                Ok(m) => Some(m),
                Err(e) => {
                    eprintln!("3D object {source:?}: {e:#}");
                    None
                }
            };
            let _ = tx.send((source, model));
            ctx.request_repaint();
        });
        Ok(None)
    }

    pub fn texture(&mut self, tile: &str, model: &Arc<Model>, size: [usize; 2], yaw: f32, color: Color32) -> TextureId {
        let shows = (Arc::as_ptr(model) as usize, size, yaw.to_bits(), color);
        let now = Instant::now();
        if let Some(r) = self.textures.get_mut(tile) {
            if r.shows != shows {
                r.texture.set(render(model, size, yaw, color), TextureOptions::LINEAR);
                r.shows = shows;
            }
            r.used = now;
            return r.texture.id();
        }
        let texture = self.ctx.load_texture(format!("object:{tile}"), render(model, size, yaw, color), TextureOptions::LINEAR);
        let id = texture.id();
        self.textures.insert(tile.to_owned(), Rendered { texture, shows, used: now });
        id
    }

    pub fn collect_garbage(&mut self) {
        self.textures.retain(|_, r| r.used.elapsed() < TEXTURE_TTL);
    }
}

fn load(source: &Source) -> anyhow::Result<Model> {
    let mesh = match source {
        Source::File(path, up) => {
            let mut mesh = formats::load(path)?;
            for p in &mut mesh.positions {
                *p = up.apply(*p);
            }
            mesh
        }
        Source::Title { text, font } => text::title_mesh(text, font.as_deref())?,
    };
    let mesh = if mesh.triangles.len() > TRIANGLE_BUDGET { simplify(&mesh, TRIANGLE_BUDGET) } else { mesh };
    Model::new(mesh).ok_or_else(|| anyhow::anyhow!("the model is empty"))
}

pub fn import(path: &Path) -> anyhow::Result<Mesh> {
    let mesh = formats::load(path)?;
    let mesh = simplify(&mesh, TRIANGLE_BUDGET);
    anyhow::ensure!(!mesh.triangles.is_empty(), "the model has no triangles");
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Mesh {
        let mut m = Mesh::default();
        for i in 0..8 {
            let c = |bit: usize| if i & bit == 0 { -1.0 } else { 1.0 };
            m.push([c(1), c(2), c(4)], None);
        }
        for f in [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]] {
            m.triangles.push([f[0], f[1], f[2]]);
            m.triangles.push([f[0], f[2], f[3]]);
        }
        m
    }

    #[test]
    fn simplify_welds_triangle_soup() {
        let cube = cube();
        let mut soup = Mesh::default();
        for t in &cube.triangles {
            let t = t.map(|i| soup.push(cube.positions[i as usize], None));
            soup.triangles.push(t);
        }
        let welded = simplify(&soup, 1000);
        assert_eq!(welded.positions.len(), 8);
        assert_eq!(welded.triangles.len(), 12);
    }

    #[test]
    fn simplify_meets_the_budget() {
        let mut m = Mesh::default();
        let n = 100;
        for j in 0..=n {
            for i in 0..=n {
                m.push([i as f32, 0.0, j as f32], None);
            }
        }
        for j in 0..n {
            for i in 0..n {
                let k = (j * (n + 1) + i) as u32;
                let r = (n + 1) as u32;
                m.triangles.extend([[k, k + 1, k + r + 1], [k, k + r + 1, k + r]]);
            }
        }
        let small = simplify(&m, 2000);
        assert!(small.triangles.len() <= 2000 && small.triangles.len() > 200, "{}", small.triangles.len());
    }

    #[test]
    fn renders_an_opaque_middle_and_clear_corners() {
        let model = Model::new(cube()).unwrap();
        let image = render(&model, [64, 64], REST_YAW, Color32::WHITE);
        assert_eq!(image.pixels[32 * 64 + 32].a(), 255);
        assert_eq!(image.pixels[0].a(), 0);
    }

    #[test]
    fn model_fits_the_unit_cylinder() {
        let model = Model::new(cube()).unwrap();
        assert!((model.radius - 1.0).abs() < 1e-5);
        assert!((model.half_height - 1.0 / 2f32.sqrt()).abs() < 1e-5);
    }
}

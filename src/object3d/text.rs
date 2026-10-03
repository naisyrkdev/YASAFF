
use super::Mesh;
use anyhow::{Context, Result};
use lyon_tessellation::math::{Point, point};
use lyon_tessellation::path::iterator::PathIterator;
use lyon_tessellation::path::{Path as LyonPath, PathEvent};
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};
use std::borrow::Cow;
use std::path::Path;

const DEPTH: f32 = 0.24;
const LINE_HEIGHT: f32 = 1.15;
const TOLERANCE: f32 = 0.006;

pub fn title_mesh(text: &str, font: Option<&Path>) -> Result<Mesh> {
    let mut data: Vec<(Cow<'static, [u8]>, u32)> = Vec::new();
    if let Some(path) = font {
        match std::fs::read(path) {
            Ok(bytes) => data.push((Cow::Owned(bytes), 0)),
            Err(e) => log::warn!("title font {}: {e}", path.display()),
        }
    }
    let defaults = egui::FontDefinitions::default();
    for name in defaults.families.get(&egui::FontFamily::Proportional).into_iter().flatten() {
        if let Some(f) = defaults.font_data.get(name) {
            data.push((f.font.clone(), f.index));
        }
    }
    let fonts: Vec<Font> = data.iter().filter_map(|(bytes, index)| Font::new(bytes, *index)).collect();
    anyhow::ensure!(!fonts.is_empty(), "no usable font");

    let mut pen = PathPen { builder: LyonPath::builder(), open: false, scale: 1.0, offset: (0.0, 0.0) };
    let lines = wrap(text.trim());
    for (row, line) in lines.iter().enumerate() {
        let glyphs: Vec<(usize, skrifa::GlyphId, f32)> = line
            .chars()
            .map(|ch| {
                let found = fonts.iter().enumerate().find_map(|(i, f)| f.glyph(ch).map(|g| (i, g)));
                match found {
                    Some((i, g)) => (i, g, fonts[i].advance(g)),
                    None => (0, skrifa::GlyphId::NOTDEF, 0.3),
                }
            })
            .collect();
        let width: f32 = glyphs.iter().map(|g| g.2).sum();
        let mut x = -width / 2.0;
        let y = -(row as f32) * LINE_HEIGHT;
        for (font, glyph, advance) in glyphs {
            if glyph != skrifa::GlyphId::NOTDEF {
                let f = &fonts[font];
                pen.scale = 1.0 / f.units_per_em;
                pen.offset = (x, y);
                if let Some(outline) = f.font.outline_glyphs().get(glyph) {
                    let _ = outline.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen);
                }
                pen.finish();
            }
            x += advance;
        }
    }
    let path = pen.builder.build();
    extrude(&path).context("could not shape the letters")
}

struct Font<'a> {
    font: FontRef<'a>,
    units_per_em: f32,
}

impl<'a> Font<'a> {
    fn new(bytes: &'a [u8], index: u32) -> Option<Self> {
        let font = FontRef::from_index(bytes, index).ok()?;
        let units_per_em = font.metrics(Size::unscaled(), LocationRef::default()).units_per_em as f32;
        (units_per_em > 0.0).then_some(Font { font, units_per_em })
    }

    fn glyph(&self, ch: char) -> Option<skrifa::GlyphId> {
        self.font.charmap().map(ch).filter(|g| *g != skrifa::GlyphId::NOTDEF)
    }

    fn advance(&self, glyph: skrifa::GlyphId) -> f32 {
        let metrics = self.font.glyph_metrics(Size::unscaled(), LocationRef::default());
        metrics.advance_width(glyph).unwrap_or(0.0) / self.units_per_em
    }
}

struct PathPen {
    builder: lyon_tessellation::path::path::Builder,
    open: bool,
    scale: f32,
    offset: (f32, f32),
}

impl PathPen {
    fn at(&self, x: f32, y: f32) -> Point {
        point(self.offset.0 + x * self.scale, self.offset.1 + y * self.scale)
    }

    fn finish(&mut self) {
        if self.open {
            self.builder.end(true);
            self.open = false;
        }
    }
}

impl OutlinePen for PathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        let p = self.at(x, y);
        self.builder.begin(p);
        self.open = true;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.builder.line_to(p);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.at(cx0, cy0), self.at(x, y));
        self.builder.quadratic_bezier_to(c, p);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.at(cx0, cy0), self.at(cx1, cy1), self.at(x, y));
        self.builder.cubic_bezier_to(c0, c1, p);
    }

    fn close(&mut self) {
        self.finish();
    }
}

fn extrude(path: &LyonPath) -> Result<Mesh> {
    let mut fill: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let options = FillOptions::tolerance(TOLERANCE).with_fill_rule(FillRule::NonZero);
    FillTessellator::new().tessellate_path(path, &options, &mut BuffersBuilder::new(&mut fill, |v: FillVertex| v.position().to_array()))?;

    let mut mesh = Mesh::default();
    let half = DEPTH / 2.0;
    for (z, flip) in [(half, false), (-half, true)] {
        let first = mesh.positions.len() as u32;
        for v in &fill.vertices {
            mesh.push([v[0], v[1], z], None);
        }
        for t in fill.indices.as_chunks::<3>().0 {
            let t = [first + t[0], first + t[1], first + t[2]];
            mesh.triangles.push(if flip { [t[0], t[2], t[1]] } else { t });
        }
    }
    let mut wall = |a: Point, b: Point| {
        let q = [[a.x, a.y, half], [b.x, b.y, half], [b.x, b.y, -half], [a.x, a.y, -half]].map(|p| mesh.push(p, None));
        mesh.triangles.extend([[q[0], q[1], q[2]], [q[0], q[2], q[3]]]);
    };
    for event in path.iter().flattened(TOLERANCE) {
        match event {
            PathEvent::Line { from, to } => wall(from, to),
            PathEvent::End { last, first, close: true } if last != first => wall(last, first),
            _ => {}
        }
    }
    anyhow::ensure!(!mesh.triangles.is_empty(), "nothing to draw");
    Ok(mesh)
}

fn wrap(text: &str) -> Vec<String> {
    let len = text.chars().count();
    let lines = match len {
        0..=12 => 1,
        13..=28 => 2,
        _ => 3,
    };
    let target = len.div_ceil(lines);
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let full = out.len() >= lines;
        match out.last_mut() {
            Some(line) if full || line.chars().count() + 1 + word.chars().count() / 2 <= target => {
                line.push(' ');
                line.push_str(word);
            }
            _ => out.push(word.to_owned()),
        }
    }
    if out.is_empty() {
        out.push(text.to_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_long_names_into_even_lines() {
        assert_eq!(wrap("Portal"), ["Portal"]);
        assert_eq!(wrap("The Witcher 3: Wild Hunt").len(), 2);
        assert_eq!(wrap("Star Wars Jedi: Fallen Order Deluxe Edition").len(), 3);
    }

    #[test]
    fn builds_solid_letters_with_the_built_in_font() {
        let mesh = title_mesh("Hi", None).unwrap();
        assert!(mesh.triangles.len() > 20);
        let zs: Vec<f32> = mesh.positions.iter().map(|p| p[2]).collect();
        assert!(zs.iter().any(|&z| z > 0.0) && zs.iter().any(|&z| z < 0.0));
    }
}

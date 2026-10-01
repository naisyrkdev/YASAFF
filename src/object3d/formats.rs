
use super::Mesh;
use anyhow::{Context, Result, anyhow, bail};
use std::collections::HashMap;
use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

pub const EXTS: &[&str] = &["stl", "obj", "ply", "glb", "gltf", "3mf"];

pub fn load(path: &Path) -> Result<Mesh> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let mut mesh = match ext.as_str() {
        "stl" => {
            let mut m = stl(&fs::read(path)?)?;
            m.z_up_to_y_up();
            m
        }
        "3mf" => {
            let mut m = three_mf(path)?;
            m.z_up_to_y_up();
            m
        }
        "obj" => obj(path)?,
        "ply" => ply(&mut fs::File::open(path)?)?,
        "gltf" | "glb" => gltf(path)?,
        _ => bail!("unsupported model format \".{ext}\" (use {})", EXTS.join(", ")),
    };
    mesh.clean();
    if mesh.triangles.is_empty() {
        bail!("the model has no triangles");
    }
    Ok(mesh)
}

fn stl(bytes: &[u8]) -> Result<Mesh> {
    let count = bytes.get(80..84).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let binary_len = count.map(|n| 84 + 50 * n);
    let ascii = bytes.starts_with(b"solid") && binary_len != Some(bytes.len());
    let mut mesh = Mesh::default();
    if ascii {
        let text = String::from_utf8_lossy(bytes);
        let mut words = text.split_ascii_whitespace();
        let mut corners = Vec::with_capacity(3);
        while let Some(word) = words.next() {
            match word {
                "vertex" => {
                    let mut p = [0.0; 3];
                    for c in &mut p {
                        *c = words.next().context("truncated vertex")?.parse().context("bad vertex")?;
                    }
                    corners.push(mesh.push(p, None));
                }
                "endloop" => {
                    for k in 1..corners.len().saturating_sub(1) {
                        mesh.triangles.push([corners[0], corners[k], corners[k + 1]]);
                    }
                    corners.clear();
                }
                _ => {}
            }
        }
    } else {
        let (Some(count), Some(len)) = (count, binary_len) else { bail!("not an STL file") };
        if bytes.len() < len {
            bail!("the STL file is cut short");
        }
        for tri in bytes[84..len].as_chunks::<50>().0 {
            let f = |i: usize| f32::from_le_bytes(tri[i..i + 4].try_into().unwrap());
            let t: [u32; 3] = std::array::from_fn(|k| mesh.push([f(12 + 12 * k), f(16 + 12 * k), f(20 + 12 * k)], None));
            mesh.triangles.push(t);
        }
        debug_assert_eq!(mesh.triangles.len(), count);
    }
    Ok(mesh)
}

fn obj(path: &Path) -> Result<Mesh> {
    let options = tobj::LoadOptions { triangulate: true, single_index: true, ignore_points: true, ignore_lines: true };
    let (models, materials) = tobj::load_obj(path, &options)?;
    let materials = materials.unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new(""));
    let textures: Vec<Option<image::RgbImage>> = materials
        .iter()
        .map(|mat| {
            let file = mat.diffuse_texture.as_deref()?.split_whitespace().last()?;
            image::open(dir.join(file)).ok().map(|i| i.to_rgb8())
        })
        .collect();
    let mut mesh = Mesh::default();
    for model in models {
        let m = model.mesh;
        let material = m.material_id.and_then(|id| Some((materials.get(id)?, textures[id].as_ref())));
        let texture = material.and_then(|(_, t)| t);
        let plain = material.filter(|(mat, _)| mat.diffuse_texture.is_none()).and_then(|(mat, _)| mat.diffuse).map(rgb_f32);
        let first = mesh.positions.len() as u32;
        for (i, p) in m.positions.as_chunks::<3>().0.iter().enumerate() {
            let own = m.vertex_color.get(i * 3..i * 3 + 3).map(|c| rgb_f32([c[0], c[1], c[2]]));
            let textured = texture.and_then(|img| {
                let uv = m.texcoords.get(i * 2..i * 2 + 2)?;
                let [r, g, b] = sample(img.as_raw(), img.width(), img.height(), 3, [uv[0], 1.0 - uv[1]])?;
                Some([r, g, b, 255])
            });
            mesh.push([p[0], p[1], p[2]], own.or(textured).or(plain));
        }
        mesh.triangles.extend(m.indices.as_chunks::<3>().0.iter().map(|t| [first + t[0], first + t[1], first + t[2]]));
    }
    Ok(mesh)
}

fn ply(reader: &mut impl Read) -> Result<Mesh> {
    use ply_rs_bw::parser::Parser;
    use ply_rs_bw::ply::{DefaultElement, Property};
    let ply = Parser::<DefaultElement>::new().read_ply(reader).map_err(|e| anyhow!("{e}"))?;
    let scalar = |p: &Property| -> Option<f64> {
        Some(match *p {
            Property::Char(v) => v as f64,
            Property::UChar(v) => v as f64,
            Property::Short(v) => v as f64,
            Property::UShort(v) => v as f64,
            Property::Int(v) => v as f64,
            Property::UInt(v) => v as f64,
            Property::Float(v) => v as f64,
            Property::Double(v) => v,
            _ => return None,
        })
    };
    let channel = |p: &Property| -> Option<u8> {
        match p {
            Property::Float(_) | Property::Double(_) => scalar(p).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
            _ => scalar(p).map(|v| v.clamp(0.0, 255.0) as u8),
        }
    };
    let list = |p: &Property| -> Option<Vec<u32>> {
        Some(match p {
            Property::ListChar(v) => v.iter().map(|&i| i as u32).collect(),
            Property::ListUChar(v) => v.iter().map(|&i| i as u32).collect(),
            Property::ListShort(v) => v.iter().map(|&i| i as u32).collect(),
            Property::ListUShort(v) => v.iter().map(|&i| i as u32).collect(),
            Property::ListInt(v) => v.iter().map(|&i| i as u32).collect(),
            Property::ListUInt(v) => v.clone(),
            _ => return None,
        })
    };
    let mut mesh = Mesh::default();
    for v in ply.payload.get("vertex").context("the PLY file has no vertices")? {
        let coord = |k: &str| v.get(k).and_then(scalar).map(|c| c as f32);
        let p = [coord("x"), coord("y"), coord("z")];
        let [Some(x), Some(y), Some(z)] = p else { bail!("a PLY vertex lacks x, y or z") };
        let rgb = ["red", "green", "blue"].map(|k| v.get(k).or_else(|| v.get(&k[..1])).and_then(channel));
        let color = match rgb {
            [Some(r), Some(g), Some(b)] => Some([r, g, b, v.get("alpha").and_then(channel).unwrap_or(255)]),
            _ => None,
        };
        mesh.push([x, y, z], color);
    }
    for face in ply.payload.get("face").into_iter().flatten() {
        let Some(corners) = face.get("vertex_indices").or_else(|| face.get("vertex_index")).and_then(list) else { continue };
        for k in 1..corners.len().saturating_sub(1) {
            mesh.triangles.push([corners[0], corners[k], corners[k + 1]]);
        }
    }
    Ok(mesh)
}

pub fn write_ply(mesh: &Mesh, path: &Path) -> Result<()> {
    let mut out = BufWriter::new(fs::File::create(path)?);
    let colored = !mesh.colors.is_empty();
    write!(out, "ply\nformat binary_little_endian 1.0\ncomment YASAFF 3D object\nelement vertex {}\n", mesh.positions.len())?;
    write!(out, "property float x\nproperty float y\nproperty float z\n")?;
    if colored {
        write!(out, "property uchar red\nproperty uchar green\nproperty uchar blue\nproperty uchar alpha\n")?;
    }
    write!(out, "element face {}\nproperty list uchar uint vertex_indices\nend_header\n", mesh.triangles.len())?;
    for (i, p) in mesh.positions.iter().enumerate() {
        for c in p {
            out.write_all(&c.to_le_bytes())?;
        }
        if colored {
            out.write_all(&mesh.colors[i])?;
        }
    }
    for t in &mesh.triangles {
        out.write_all(&[3])?;
        for i in t {
            out.write_all(&i.to_le_bytes())?;
        }
    }
    out.flush()?;
    Ok(())
}

fn gltf(path: &Path) -> Result<Mesh> {
    let (doc, buffers, images) = gltf::import(path)?;
    let mut mesh = Mesh::default();
    let identity = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    match doc.default_scene().or_else(|| doc.scenes().next()) {
        Some(scene) => {
            for node in scene.nodes() {
                gltf_node(&node, identity, &buffers, &images, &mut mesh);
            }
        }
        None => {
            for m in doc.meshes() {
                gltf_mesh(&m, identity, &buffers, &images, &mut mesh);
            }
        }
    }
    Ok(mesh)
}

type Mat4 = [[f32; 4]; 4];

fn gltf_node(node: &gltf::Node, parent: Mat4, buffers: &[gltf::buffer::Data], images: &[gltf::image::Data], out: &mut Mesh) {
    let transform = mat_mul(parent, node.transform().matrix());
    if let Some(m) = node.mesh() {
        gltf_mesh(&m, transform, buffers, images, out);
    }
    for child in node.children() {
        gltf_node(&child, transform, buffers, images, out);
    }
}

fn gltf_mesh(m: &gltf::Mesh, transform: Mat4, buffers: &[gltf::buffer::Data], images: &[gltf::image::Data], out: &mut Mesh) {
    for prim in m.primitives() {
        if prim.mode() != gltf::mesh::Mode::Triangles {
            continue;
        }
        let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
        let Some(positions) = reader.read_positions() else { continue };
        let positions: Vec<[f32; 3]> = positions.collect();
        let pbr = prim.material().pbr_metallic_roughness();
        let factor = pbr.base_color_factor();
        let vertex_colors: Option<Vec<[u8; 4]>> = reader.read_colors(0).map(|c| c.into_rgba_u8().collect());
        let texture = pbr.base_color_texture().and_then(|info| {
            let image = images.get(info.texture().source().index())?;
            let uvs: Vec<[f32; 2]> = reader.read_tex_coords(info.tex_coord())?.into_f32().collect();
            Some((image, uvs))
        });
        let plain = vertex_colors.is_none() && texture.is_none() && factor == [1.0; 4];
        let first = out.positions.len() as u32;
        for (i, p) in positions.iter().enumerate() {
            let color = (!plain).then(|| {
                let mut c = factor;
                if let Some(v) = vertex_colors.as_ref().and_then(|v| v.get(i)) {
                    for k in 0..3 {
                        c[k] *= v[k] as f32 / 255.0;
                    }
                }
                if let Some(t) = texture.as_ref().and_then(|(img, uvs)| sample_gltf(img, *uvs.get(i)?)) {
                    for k in 0..3 {
                        c[k] *= t[k] as f32 / 255.0;
                    }
                }
                let [r, g, b] = [c[0], c[1], c[2]].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
                [r, g, b, 255]
            });
            out.push(transform_point(transform, *p), color);
        }
        match reader.read_indices() {
            Some(indices) => {
                let indices: Vec<u32> = indices.into_u32().collect();
                out.triangles.extend(indices.as_chunks::<3>().0.iter().map(|t| [first + t[0], first + t[1], first + t[2]]));
            }
            None => out.triangles.extend((0..positions.len() as u32 / 3).map(|t| [first + 3 * t, first + 3 * t + 1, first + 3 * t + 2])),
        }
    }
}

fn sample_gltf(image: &gltf::image::Data, uv: [f32; 2]) -> Option<[u8; 3]> {
    use gltf::image::Format;
    let channels = match image.format {
        Format::R8 => 1,
        Format::R8G8 => 2,
        Format::R8G8B8 => 3,
        Format::R8G8B8A8 => 4,
        _ => return None,
    };
    sample(&image.pixels, image.width, image.height, channels, uv)
}

fn sample(pixels: &[u8], width: u32, height: u32, channels: usize, uv: [f32; 2]) -> Option<[u8; 3]> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let x = ((uv[0].rem_euclid(1.0) * w as f32) as usize).min(w - 1);
    let y = ((uv[1].rem_euclid(1.0) * h as f32) as usize).min(h - 1);
    let px = pixels.get((y * w + x) * channels..(y * w + x) * channels + channels)?;
    Some(if channels < 3 { [px[0]; 3] } else { [px[0], px[1], px[2]] })
}

fn mat_mul(a: Mat4, b: Mat4) -> Mat4 {
    std::array::from_fn(|col| std::array::from_fn(|row| (0..4).map(|k| a[k][row] * b[col][k]).sum()))
}

fn transform_point(m: Mat4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|row| m[0][row] * p[0] + m[1][row] * p[1] + m[2][row] * p[2] + m[3][row])
}

fn rgb_f32(c: [f32; 3]) -> [u8; 4] {
    let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    [r, g, b, 255]
}

type Affine = [f32; 12];
const IDENTITY: Affine = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

fn affine_apply(m: &Affine, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|c| p[0] * m[c] + p[1] * m[3 + c] + p[2] * m[6 + c] + m[9 + c])
}

fn affine_then(first: &Affine, then: &Affine) -> Affine {
    let mut out = [0.0; 12];
    for r in 0..4 {
        let row = if r < 3 { [first[r * 3], first[r * 3 + 1], first[r * 3 + 2]] } else { [first[9], first[10], first[11]] };
        for c in 0..3 {
            out[r * 3 + c] = row[0] * then[c] + row[1] * then[3 + c] + row[2] * then[6 + c] + if r == 3 { then[9 + c] } else { 0.0 };
        }
    }
    out
}

fn parse_affine(text: &str) -> Option<Affine> {
    let values: Vec<f32> = text.split_ascii_whitespace().map(|v| v.parse().ok()).collect::<Option<_>>()?;
    values.try_into().ok()
}

#[derive(Default)]
struct Object3mf {
    positions: Vec<[f32; 3]>,
    triangles: Vec<([u32; 3], Option<[u8; 4]>)>,
    components: Vec<(u32, Option<String>, Affine)>,
}

#[derive(Default)]
struct Part3mf {
    objects: HashMap<u32, Object3mf>,
    build: Vec<(u32, Option<String>, Affine)>,
}

fn three_mf(path: &Path) -> Result<Mesh> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?).context("not a 3MF (zip) file")?;
    let mut parts: HashMap<String, Part3mf> = HashMap::new();
    let mut root = None;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = format!("/{}", entry.name().trim_start_matches('/'));
        if !name.to_lowercase().ends_with(".model") {
            continue;
        }
        let mut text = String::new();
        entry.read_to_string(&mut text).with_context(|| format!("reading {name}"))?;
        let part = parse_3mf_model(&text).with_context(|| format!("reading {name}"))?;
        if name.eq_ignore_ascii_case("/3D/3dmodel.model") || (root.is_none() && !part.build.is_empty()) {
            root = Some(name.clone());
        }
        parts.insert(name.to_lowercase(), part);
    }
    let root = root.context("the 3MF file has no model")?.to_lowercase();
    let mut mesh = Mesh::default();
    for (object, file, transform) in &parts[&root].build {
        let file = file.as_deref().map(|f| f.to_lowercase()).unwrap_or_else(|| root.clone());
        emit_3mf(&parts, &file, *object, transform, &mut mesh, 0);
    }
    Ok(mesh)
}

fn emit_3mf(parts: &HashMap<String, Part3mf>, file: &str, id: u32, transform: &Affine, out: &mut Mesh, depth: u32) {
    let Some(object) = parts.get(file).and_then(|p| p.objects.get(&id)) else { return };
    if depth > 16 {
        return;
    }
    for (corners, color) in &object.triangles {
        let Some(ps) = corners.iter().map(|&i| object.positions.get(i as usize)).collect::<Option<Vec<_>>>() else { continue };
        let t: [u32; 3] = std::array::from_fn(|k| out.push(affine_apply(transform, *ps[k]), *color));
        out.triangles.push(t);
    }
    for (child, child_file, child_transform) in &object.components {
        let child_file = child_file.as_deref().map(|f| f.to_lowercase()).unwrap_or_else(|| file.to_owned());
        emit_3mf(parts, &child_file, *child, &affine_then(child_transform, transform), out, depth + 1);
    }
}

fn parse_3mf_model(text: &str) -> Result<Part3mf> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let mut part = Part3mf::default();
    let mut palettes: HashMap<u32, Vec<[u8; 4]>> = HashMap::new();
    let mut palette: Option<u32> = None;
    let mut current: Option<(u32, Object3mf, Option<[u8; 4]>)> = None;
    loop {
        let event = reader.read_event().map_err(|e| anyhow!("{e}"))?;
        let (e, empty) = match &event {
            Event::Start(e) => (e, false),
            Event::Empty(e) => (e, true),
            Event::End(e) => {
                match e.local_name().as_ref() {
                    b"object" => {
                        if let Some((id, object, _)) = current.take() {
                            part.objects.insert(id, object);
                        }
                    }
                    b"basematerials" | b"colorgroup" => palette = None,
                    _ => {}
                }
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        let mut attrs: HashMap<Vec<u8>, String> = HashMap::new();
        for a in e.attributes().flatten() {
            attrs.insert(a.key.local_name().as_ref().to_vec(), String::from_utf8_lossy(&a.value).into_owned());
        }
        let get = |k: &str| attrs.get(k.as_bytes()).map(String::as_str);
        let num = |k: &str| get(k).and_then(|v| v.trim().parse::<u32>().ok());
        let color_of = |pid: Option<u32>, index: Option<u32>| palettes.get(&pid?)?.get(index? as usize).copied();
        match e.local_name().as_ref() {
            b"basematerials" | b"colorgroup" => {
                if let Some(id) = num("id") {
                    palettes.insert(id, Vec::new());
                    palette = (!empty).then_some(id);
                }
            }
            b"base" | b"color" => {
                if let (Some(id), Some(c)) = (palette, get("displaycolor").or(get("color")).and_then(parse_color)) {
                    palettes.entry(id).or_default().push(c);
                }
            }
            b"object" => {
                let color = color_of(num("pid"), num("pindex"));
                current = num("id").map(|id| (id, Object3mf::default(), color));
            }
            b"vertex" => {
                if let Some((_, object, _)) = &mut current {
                    let c = |k: &str| get(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0);
                    object.positions.push([c("x"), c("y"), c("z")]);
                }
            }
            b"triangle" => {
                if let Some((_, object, default)) = &mut current
                    && let (Some(a), Some(b), Some(c)) = (num("v1"), num("v2"), num("v3"))
                {
                    let color = color_of(num("pid"), num("p1")).or(*default);
                    object.triangles.push(([a, b, c], color));
                }
            }
            b"component" => {
                if let (Some((_, object, _)), Some(id)) = (&mut current, num("objectid")) {
                    let transform = get("transform").and_then(parse_affine).unwrap_or(IDENTITY);
                    object.components.push((id, get("path").map(str::to_owned), transform));
                }
            }
            b"item" => {
                if let Some(id) = num("objectid") {
                    let transform = get("transform").and_then(parse_affine).unwrap_or(IDENTITY);
                    part.build.push((id, get("path").map(str::to_owned), transform));
                }
            }
            _ => {}
        }
        if empty
            && e.local_name().as_ref() == b"object"
            && let Some((id, object, _)) = current.take()
        {
            part.objects.insert(id, object);
        }
    }
    Ok(part)
}

fn parse_color(text: &str) -> Option<[u8; 4]> {
    let hex = text.trim().strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 | 8 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASCII_STL: &str = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";

    #[test]
    fn reads_ascii_and_binary_stl() {
        let m = stl(ASCII_STL.as_bytes()).unwrap();
        assert_eq!(m.triangles.len(), 1);
        assert_eq!(m.positions[1], [1.0, 0.0, 0.0]);

        let mut bin = vec![0u8; 80];
        bin.extend(1u32.to_le_bytes());
        bin.extend([0u8; 12]);
        for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 2.0]] {
            for c in v {
                bin.extend(c.to_le_bytes());
            }
        }
        bin.extend([0u8; 2]);
        let m = stl(&bin).unwrap();
        assert_eq!(m.triangles.len(), 1);
        assert_eq!(m.positions[2], [0.0, 0.0, 2.0]);
    }

    #[test]
    fn ply_round_trip_keeps_colors() {
        let mut mesh = Mesh::default();
        let a = mesh.push([0.0, 0.0, 0.0], Some([255, 0, 0, 255]));
        let b = mesh.push([1.0, 0.0, 0.0], None);
        let c = mesh.push([0.0, 1.0, 0.0], Some([0, 0, 255, 255]));
        mesh.triangles.push([a, b, c]);
        let dir = std::env::temp_dir().join(format!("yasaff-ply-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.ply");
        write_ply(&mesh, &path).unwrap();
        let back = load(&path).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(back.positions, mesh.positions);
        assert_eq!(back.colors, mesh.colors);
        assert_eq!(back.triangles, mesh.triangles);
    }

    #[test]
    fn reads_3mf_objects_components_and_colors() {
        let xml = r##"<?xml version="1.0"?>
<model xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <resources>
  <basematerials id="5"><base name="red" displaycolor="#FF0000"/></basematerials>
  <object id="1" type="model" pid="5" pindex="0"><mesh>
   <vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices>
   <triangles><triangle v1="0" v2="1" v3="2"/></triangles>
  </mesh></object>
  <object id="2"><components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 10 0 0"/></components></object>
 </resources>
 <build><item objectid="2"/></build>
</model>"##;
        let part = parse_3mf_model(xml).unwrap();
        let parts = HashMap::from([("/3d/3dmodel.model".to_owned(), part)]);
        let mut mesh = Mesh::default();
        for (id, _, t) in &parts["/3d/3dmodel.model"].build {
            emit_3mf(&parts, "/3d/3dmodel.model", *id, t, &mut mesh, 0);
        }
        assert_eq!(mesh.triangles.len(), 1);
        assert_eq!(mesh.positions[1], [11.0, 0.0, 0.0]);
        assert_eq!(mesh.colors[0], [255, 0, 0, 255]);
    }

    #[test]
    fn affine_composition_applies_first_then_second() {
        let shift = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
        let double = [2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0];
        assert_eq!(affine_apply(&affine_then(&shift, &double), [1.0, 1.0, 1.0]), [4.0, 2.0, 2.0]);
    }
}

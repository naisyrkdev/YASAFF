
use crate::library::Library;
use anyhow::{Context as _, Result, bail};
use egui::{Context, FontData, FontDefinitions, FontFamily, FontId};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const EXTS: &[&str] = &["ttf", "otf", "ttc"];
const TITLE_FAMILY: &str = "yasaff-title";

pub fn title_family() -> FontFamily {
    FontFamily::Name(TITLE_FAMILY.into())
}

pub fn dir() -> PathBuf {
    Library::data_dir().join("fonts")
}

pub fn list() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file() && has_font_ext(&e.path()))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

pub fn display_name(file: &str) -> &str {
    Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or(file)
}

pub fn import(path: &Path) -> Result<String> {
    let name = path.file_name().and_then(|n| n.to_str()).context("the file name is not valid")?.to_owned();
    if !has_font_ext(path) {
        bail!("{name} is not a .ttf, .otf or .ttc font");
    }
    let bytes = fs::read(path).with_context(|| format!("reading {name}"))?;
    validate(&bytes).with_context(|| format!("{name} is not a usable font"))?;
    fs::create_dir_all(dir())?;
    fs::write(dir().join(&name), bytes)?;
    Ok(name)
}

pub fn remove(file: &str) -> Result<()> {
    Ok(fs::remove_file(dir().join(file))?)
}

pub fn family(file: &str) -> FontFamily {
    FontFamily::Name(format!("font:{file}").into())
}

pub fn of(file: &str, size: f32) -> FontId {
    FontId::new(size, family(file))
}

pub fn install(ctx: &Context, title_font: &str) -> (Vec<String>, Vec<String>) {
    let mut defs = FontDefinitions::default();
    let fallback = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let (mut loaded, mut warnings) = (Vec::new(), Vec::new());
    for file in list() {
        let bytes = match fs::read(dir().join(&file)).map_err(anyhow::Error::from).and_then(|b| validate(&b).map(|()| b)) {
            Ok(bytes) => bytes,
            Err(e) => {
                warnings.push(tr!("Skipping font {file}: {error}", file = file, error = format!("{e:#}")));
                continue;
            }
        };
        defs.font_data.insert(file.clone(), Arc::new(FontData::from_owned(bytes)));
        defs.families.insert(family(&file), std::iter::once(file.clone()).chain(fallback.iter().cloned()).collect());
        loaded.push(file);
    }
    let mut title = fallback;
    if loaded.iter().any(|f| f == title_font) {
        title.insert(0, title_font.to_owned());
    }
    defs.families.insert(title_family(), title);
    ctx.set_fonts(defs);
    (loaded, warnings)
}

fn has_font_ext(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTS.contains(&e.to_lowercase().as_str()))
}

fn validate(bytes: &[u8]) -> Result<()> {
    skrifa::FontRef::from_index(bytes, 0).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_non_fonts() {
        assert!(super::validate(b"not a font at all").is_err());
        assert!(super::validate(&[]).is_err());
    }
}


use super::ids::ExternalId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const EXTENSION: &str = "yasaff";
pub const MAGIC: &str = "disc";
pub const VERSION: u32 = 1;
const MAX_SIZE: u64 = 64 * 1024;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiscManifest {
    pub id: Option<String>,
    pub ids: Vec<ExternalId>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManifestError(pub String);

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ManifestError {}

fn err<T>(msg: impl Into<String>) -> Result<T, ManifestError> {
    Err(ManifestError(msg.into()))
}

#[derive(Serialize, Deserialize)]
struct Raw {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    yasaff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    ids: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

fn id_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

impl DiscManifest {
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let text = text.trim_start_matches('\u{feff}');
        let raw: Raw = serde_json::from_str(text).map_err(|e| ManifestError(format!("not valid JSON: {e}")))?;
        if let Some(magic) = &raw.yasaff
            && magic != MAGIC
        {
            return err(format!("\"yasaff\" should be \"{MAGIC}\", not \"{magic}\""));
        }
        if raw.version.unwrap_or(VERSION) > VERSION {
            return err("this disc was made for a newer version of YASAFF");
        }
        let id = raw.id.as_ref().and_then(id_text);
        let mut ids: Vec<ExternalId> =
            raw.ids.iter().filter_map(|(store, id)| Some(ExternalId::new(store, &id_text(id)?))).collect();
        ids.sort_by(|a, b| a.store.cmp(&b.store));
        if id.is_none() && ids.is_empty() {
            return err("it names no game (add \"id\" or \"ids\")");
        }
        let name = raw.name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
        Ok(DiscManifest { id, ids, name })
    }

    pub fn read(path: &Path) -> Result<Self, ManifestError> {
        let io = |e: std::io::Error| ManifestError(format!("can't read it: {e}"));
        let file = fs::File::open(path).map_err(io)?;
        if file.metadata().map_err(io)?.len() > MAX_SIZE {
            return err("the file is too big to be a disc file");
        }
        let mut text = String::new();
        file.take(MAX_SIZE).read_to_string(&mut text).map_err(|_| ManifestError("it isn't UTF-8 text".into()))?;
        Self::parse(&text)
    }

    pub fn to_json(&self) -> String {
        let raw = Raw {
            yasaff: Some(MAGIC.to_owned()),
            version: Some(VERSION),
            id: self.id.clone().map(Value::String),
            ids: self.ids.iter().map(|i| (i.store.clone(), Value::String(i.id.clone()))).collect(),
            name: self.name.clone(),
        };
        serde_json::to_string_pretty(&raw).expect("manifest serializes") + "\n"
    }

    pub fn display_name(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }
        let ids: Vec<String> = self.ids.iter().map(|i| format!("{} {}", i.store, i.id)).collect();
        if ids.is_empty() { format!("Game {}", self.id.as_deref().unwrap_or("?")) } else { ids.join(", ") }
    }
}

pub fn find(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
                || p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(".yasaff"))
        })
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    Ok(found)
}

pub fn read_all(dir: &Path) -> std::io::Result<Vec<(PathBuf, Result<DiscManifest, ManifestError>)>> {
    Ok(find(dir)?.into_iter().map(|p| {
        let m = DiscManifest::read(&p);
        (p, m)
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_file() {
        let m = DiscManifest::parse(
            r#"{ "yasaff": "disc", "version": 1, "id": "abc", "ids": { "Steam": "570", "gog": 1207658924 }, "name": " Dota 2 ", "extra": true }"#,
        )
        .unwrap();
        assert_eq!(m.id.as_deref(), Some("abc"));
        assert_eq!(m.ids, vec![ExternalId::new("gog", "1207658924"), ExternalId::new("steam", "570")]);
        assert_eq!(m.name.as_deref(), Some("Dota 2"));
    }

    #[test]
    fn minimal_hand_written_file_with_bom() {
        let m = DiscManifest::parse("\u{feff}{\"ids\":{\"steam\":570}}").unwrap();
        assert_eq!(m.ids, vec![ExternalId::new("steam", "570")]);
        assert_eq!(m.display_name(), "steam 570");
    }

    #[test]
    fn rejects_bad_files() {
        assert!(DiscManifest::parse("{}").is_err());
        assert!(DiscManifest::parse(r#"{"ids":{"steam":""}}"#).is_err());
        assert!(DiscManifest::parse(r#"{"yasaff":"theme","id":"x"}"#).is_err());
        assert!(DiscManifest::parse(r#"{"version":2,"id":"x"}"#).is_err());
        assert!(DiscManifest::parse("not json").is_err());
    }

    #[test]
    fn round_trip() {
        let m = DiscManifest { id: Some("abc".into()), ids: vec![ExternalId::new("steam", "570")], name: Some("Dota 2".into()) };
        assert_eq!(DiscManifest::parse(&m.to_json()).unwrap(), m);
    }

    #[test]
    fn finds_disc_files_by_extension() {
        let dir = std::env::temp_dir().join(format!("yasaff-find-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for name in [".conf.yasaff", "Game.YASAFF", "readme.txt", ".yasaff"] {
            fs::write(dir.join(name), "{}").unwrap();
        }
        let names: Vec<String> =
            find(&dir).unwrap().iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, [".conf.yasaff", ".yasaff", "Game.YASAFF"]);
    }
}

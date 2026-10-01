
use crate::apps::{self, AppEntry};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use crate::physical::{ExternalId, ids};

pub struct Found {
    pub entry: AppEntry,
    pub source: String,
    pub location: PathBuf,
}

pub fn scan() -> Vec<Found> {
    let mut found = steam_games();
    found.extend(heroic_games());

    let mut seen_ids: HashSet<ExternalId> = found.iter().flat_map(|f| ids::from_command(&f.entry.target, &f.entry.args)).collect();
    for entry in apps::installed() {
        if !entry.is_game || entry.file.as_deref().is_some_and(is_store_launcher) {
            continue;
        }
        let entry_ids = ids::from_command(&entry.target, &entry.args);
        if entry_ids.iter().any(|id| seen_ids.contains(id)) {
            continue;
        }
        seen_ids.extend(entry_ids);
        let location = entry.file.clone().unwrap_or_default();
        found.push(Found { entry, source: tr!("Shortcut").into(), location });
    }

    let mut seen = HashSet::new();
    found.retain(|f| seen.insert((f.entry.target.clone(), f.entry.args.clone())));
    found.sort_by_key(|f| f.entry.name.to_lowercase());
    found
}

fn game_entry(name: String, target: String, icon: Option<PathBuf>) -> AppEntry {
    AppEntry {
        name,
        target,
        args: Vec::new(),
        working_dir: None,
        icon,
        is_game: true,
        on_desktop: false,
        hidden: false,
        file: None,
    }
}

fn is_store_launcher(shortcut: &Path) -> bool {
    const LAUNCHERS: &[&str] = &[
        "steam",
        "com.valvesoftware.Steam",
        "heroic",
        "com.heroicgameslauncher.hgl",
        "lutris",
        "net.lutris.Lutris",
        "itch",
        "io.itch.itch",
        "com.usebottles.bottles",
        "minigalaxy",
        "io.github.sharkwouter.Minigalaxy",
    ];
    shortcut.file_stem().is_some_and(|stem| LAUNCHERS.iter().any(|l| stem.eq_ignore_ascii_case(l)))
}

pub fn home() -> PathBuf {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()).unwrap_or_default()
}

fn existing_dirs(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    dirs.into_iter().filter(|d| d.is_dir() && seen.insert(fs::canonicalize(d).unwrap_or_else(|_| d.clone()))).collect()
}

pub fn steam_roots() -> Vec<PathBuf> {
    let home = home();
    let mut roots = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(base) = std::env::var_os(var) {
                roots.push(PathBuf::from(base).join("Steam"));
            }
        }
    } else if cfg!(target_os = "macos") {
        roots.push(home.join("Library/Application Support/Steam"));
    } else {
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }
    existing_dirs(roots)
}

fn steam_libraries_of(root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![root.to_path_buf()];
    if let Ok(text) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        libraries.extend(vdf_values(&vdf_strings(&text), "path").map(PathBuf::from));
    }
    existing_dirs(libraries)
}

pub fn steam_libraries() -> Vec<PathBuf> {
    existing_dirs(steam_roots().iter().flat_map(|r| steam_libraries_of(r)).collect())
}

pub fn steam_installdir(libraries: &[PathBuf], appid: &str) -> Option<String> {
    libraries.iter().find_map(|library| {
        let text = fs::read_to_string(library.join(format!("steamapps/appmanifest_{appid}.acf"))).ok()?;
        vdf_values(&vdf_strings(&text), "installdir").next().map(str::to_owned)
    })
}

fn steam_games() -> Vec<Found> {
    let mut found = Vec::new();
    for root in steam_roots() {
        for library in steam_libraries_of(&root) {
            let Ok(dir) = fs::read_dir(library.join("steamapps")) else { continue };
            for file in dir.flatten().map(|e| e.path()) {
                let is_manifest = file.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("appmanifest_") && n.ends_with(".acf"));
                if let Some(game) = is_manifest.then(|| steam_game(&root, &library, &file)).flatten() {
                    found.push(game);
                }
            }
        }
    }
    found
}

fn steam_game(root: &Path, library: &Path, manifest: &Path) -> Option<Found> {
    let tokens = vdf_strings(&fs::read_to_string(manifest).ok()?);
    let appid = vdf_values(&tokens, "appid").next()?;
    let name = vdf_values(&tokens, "name").next()?;
    let installdir = vdf_values(&tokens, "installdir").next().unwrap_or_default();
    let flags: u32 = vdf_values(&tokens, "StateFlags").next().and_then(|f| f.parse().ok()).unwrap_or(0);
    if flags & 4 == 0 || is_steam_tool(appid, name) {
        return None;
    }
    let cache = root.join("appcache/librarycache");
    let icon = [cache.join(appid).join("library_600x900.jpg"), cache.join(format!("{appid}_library_600x900.jpg"))]
        .into_iter()
        .find(|p| p.is_file());
    Some(Found {
        entry: game_entry(name.to_owned(), format!("steam://rungameid/{appid}"), icon),
        source: "Steam".into(),
        location: library.join("steamapps/common").join(installdir),
    })
}

fn is_steam_tool(appid: &str, name: &str) -> bool {
    const TOOL_IDS: &[&str] = &["228980", "1070560", "1391110", "1628350", "4183110"];
    TOOL_IDS.contains(&appid)
        || ["Proton ", "Steam Linux Runtime", "Steamworks Common"].iter().any(|p| name.starts_with(p))
        || name == "Proton Experimental"
        || name == "Proton Hotfix"
}

fn vdf_strings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => s.extend(chars.next()),
                c => s.push(c),
            }
        }
        out.push(s);
    }
    out
}

fn vdf_values<'a>(tokens: &'a [String], key: &'a str) -> impl Iterator<Item = &'a str> {
    tokens.windows(2).filter(move |w| w[0].eq_ignore_ascii_case(key)).map(|w| w[1].as_str())
}

pub fn heroic_config_dirs() -> Vec<PathBuf> {
    let home = home();
    let mut dirs = Vec::new();
    if cfg!(windows) {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("heroic"));
        }
    } else if cfg!(target_os = "macos") {
        dirs.push(home.join("Library/Application Support/heroic"));
    } else {
        let config = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
        dirs.push(config.join("heroic"));
        dirs.push(home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic"));
    }
    existing_dirs(dirs)
}

pub fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn heroic_titles(config: &Path, file: &str, list_key: &str) -> Vec<(String, String)> {
    let Some(json) = read_json(&config.join("store_cache").join(file)) else { return Vec::new() };
    json[list_key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| Some((g["app_name"].as_str()?.to_owned(), g["title"].as_str()?.to_owned())))
        .collect()
}

fn heroic_games() -> Vec<Found> {
    let mut found = Vec::new();
    for config in heroic_config_dirs() {
        let mut add = |runner: &str, label: &str, app: &str, title: Option<String>, path: &str| {
            let location = PathBuf::from(path);
            let name = title.or_else(|| location.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_else(|| app.to_owned());
            let uri = format!("heroic://launch?appName={app}&runner={runner}");
            found.push(Found { entry: game_entry(name, uri, None), source: format!("Heroic · {label}"), location });
        };

        let legendary = read_json(&config.join("legendaryConfig/legendary/installed.json"));
        for (app, game) in legendary.as_ref().and_then(|j| j.as_object()).into_iter().flatten() {
            if game["is_dlc"].as_bool() != Some(true) {
                let title = game["title"].as_str().map(str::to_owned);
                add("legendary", "Epic", app, title, game["install_path"].as_str().unwrap_or_default());
            }
        }

        let gog_titles = heroic_titles(&config, "gog_library.json", "games");
        let gog = read_json(&config.join("gog_store/installed.json"));
        for game in gog.as_ref().and_then(|j| j["installed"].as_array()).into_iter().flatten() {
            let Some(app) = game["appName"].as_str() else { continue };
            if game["is_dlc"].as_bool() != Some(true) {
                let title = gog_titles.iter().find(|(a, _)| a == app).map(|(_, t)| t.clone());
                add("gog", "GOG", app, title, game["install_path"].as_str().unwrap_or_default());
            }
        }

        let amazon_titles = heroic_titles(&config, "nile_library.json", "library");
        let nile = read_json(&config.join("nile_config/nile/installed.json"));
        for game in nile.as_ref().and_then(|j| j.as_array()).into_iter().flatten() {
            let Some(app) = game["id"].as_str() else { continue };
            let title = amazon_titles.iter().find(|(a, _)| a == app).map(|(_, t)| t.clone());
            add("nile", "Amazon", app, title, game["path"].as_str().unwrap_or_default());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_manifest_values() {
        let acf = r#""AppState"
{
	"appid"		"570"
	"name"		"Dota 2"
	"StateFlags"		"4"
	"installdir"		"dota 2 beta"
	"UserConfig" { "language" "english" }
}"#;
        let tokens = vdf_strings(acf);
        assert_eq!(vdf_values(&tokens, "appid").next(), Some("570"));
        assert_eq!(vdf_values(&tokens, "name").next(), Some("Dota 2"));
        assert_eq!(vdf_values(&tokens, "installdir").next(), Some("dota 2 beta"));
    }

    #[test]
    fn library_folder_paths_unescape() {
        let vdf = r#""libraryfolders" { "0" { "path" "C:\\Program Files (x86)\\Steam" } "1" { "path" "D:\\Games" } }"#;
        let tokens = vdf_strings(vdf);
        let paths: Vec<_> = vdf_values(&tokens, "path").collect();
        assert_eq!(paths, [r"C:\Program Files (x86)\Steam", r"D:\Games"]);
    }

    #[test]
    fn steam_tools_are_skipped() {
        assert!(is_steam_tool("1493710", "Proton Experimental"));
        assert!(is_steam_tool("1", "Proton 9.0"));
        assert!(is_steam_tool("1628350", "Steam Linux Runtime 3.0 (sniper)"));
        assert!(!is_steam_tool("570", "Dota 2"));
    }
}

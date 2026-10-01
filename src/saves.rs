
use crate::library::Game;
use crate::physical::ids;
use crate::scan;
use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn detect(game: &Game) -> Option<PathBuf> {
    Finder::new().find(game)
}

pub fn browse_from(game: &Game) -> PathBuf {
    let finder = Finder::new();
    finder
        .prefix_users(game)
        .into_iter()
        .next()
        .or_else(|| directories::UserDirs::new().and_then(|d| d.document_dir().map(Path::to_path_buf)))
        .unwrap_or_else(scan::home)
}

pub fn backup(game_name: &str, save_dir: &Path, dest: &Path) -> Result<PathBuf> {
    use zip::write::SimpleFileOptions;

    if !save_dir.is_dir() {
        bail!(tr!("the save folder {folder} doesn't exist", folder = save_dir.display()));
    }
    if dest.starts_with(save_dir) {
        bail!(tr!("choose a folder outside the save folder"));
    }
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S");
    let name: String = game_name.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    let zip_path = dest.join(format!("{} saves {stamp}.zip", name.trim()));
    let partial = zip_path.with_extension("zip.partial");
    let written = (|| -> Result<usize> {
        let mut zip = zip::ZipWriter::new(io::BufWriter::new(fs::File::create(&partial).with_context(|| format!("creating {}", partial.display()))?));
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
        let mut files = 0;
        let mut dirs = vec![save_dir.to_path_buf()];
        while let Some(dir) = dirs.pop() {
            let mut entries: Vec<_> = fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            if entries.is_empty() && dir != save_dir {
                zip.add_directory(zip_name(save_dir, &dir), options)?;
            }
            for entry in entries {
                let path = entry.path();
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    dirs.push(path);
                } else if kind.is_file() {
                    zip.start_file(zip_name(save_dir, &path), options)?;
                    io::copy(&mut fs::File::open(&path).with_context(|| format!("reading {}", path.display()))?, &mut zip)?;
                    files += 1;
                }
            }
        }
        zip.finish()?;
        Ok(files)
    })();
    match written {
        Ok(0) => {
            let _ = fs::remove_file(&partial);
            bail!(tr!("there are no files in {folder}", folder = save_dir.display()))
        }
        Ok(_) => {
            fs::rename(&partial, &zip_path).with_context(|| format!("saving {}", zip_path.display()))?;
            Ok(zip_path)
        }
        Err(e) => {
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn zip_name(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

pub struct Finder {
    steam_libraries: Vec<PathBuf>,
    steam_roots: Vec<PathBuf>,
    heroic_configs: Vec<PathBuf>,
}

impl Finder {
    pub fn new() -> Self {
        Finder { steam_libraries: scan::steam_libraries(), steam_roots: scan::steam_roots(), heroic_configs: scan::heroic_config_dirs() }
    }

    pub fn find(&self, game: &Game) -> Option<PathBuf> {
        if let Some(id) = &game.app_id {
            return Some(crate::appstore::app_data_dir(id));
        }
        let names = self.names(game);
        if names.is_empty() {
            return None;
        }
        let prefix_users = self.prefix_users(game);
        let places = if prefix_users.is_empty() {
            native_places()
        } else {
            prefix_users.iter().flat_map(|u| windows_places(u)).collect()
        };
        search(&places, &names).or_else(|| self.steam_cloud(game))
    }

    fn steam_appid(game: &Game) -> Option<String> {
        game.external_ids().into_iter().find(|id| id.store == ids::STEAM).map(|id| id.id)
    }

    fn names(&self, game: &Game) -> Vec<String> {
        let mut names = vec![game.name.clone()];
        if let Some(appid) = Self::steam_appid(game) {
            names.extend(scan::steam_installdir(&self.steam_libraries, &appid));
        }
        if let Some(dir) = &game.working_dir {
            names.extend(dir.file_name().map(|n| n.to_string_lossy().into_owned()));
        }
        let target = Path::new(game.target.trim());
        if target.is_absolute() {
            names.extend(target.file_stem().map(|n| n.to_string_lossy().into_owned()));
            names.extend(target.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()));
        }
        let mut seen = HashSet::new();
        names.into_iter().map(|n| normalize(&n)).filter(|n| n.len() >= 3 && !GENERIC.contains(&n.as_str()) && seen.insert(n.clone())).collect()
    }

    fn prefix_users(&self, game: &Game) -> Vec<PathBuf> {
        let mut prefixes = Vec::new();
        for id in game.external_ids() {
            if id.store == ids::STEAM {
                for library in &self.steam_libraries {
                    prefixes.push(library.join("steamapps/compatdata").join(&id.id).join("pfx"));
                }
            } else {
                for config in &self.heroic_configs {
                    let json = scan::read_json(&config.join("GamesConfig").join(format!("{}.json", id.id)));
                    if let Some(prefix) = json.as_ref().and_then(|j| j[&id.id]["winePrefix"].as_str()) {
                        let prefix = PathBuf::from(prefix);
                        prefixes.push(if prefix.join("pfx/drive_c").is_dir() { prefix.join("pfx") } else { prefix });
                    }
                }
            }
        }
        let paths = [Some(PathBuf::from(game.target.trim())), game.working_dir.clone()];
        for path in paths.into_iter().flatten() {
            if let Some(drive_c) = path.ancestors().find(|a| a.file_name().is_some_and(|n| n.eq_ignore_ascii_case("drive_c"))) {
                prefixes.extend(drive_c.parent().map(Path::to_path_buf));
            }
        }
        let mut users = Vec::new();
        for prefix in prefixes {
            let Ok(entries) = fs::read_dir(prefix.join("drive_c/users")) else { continue };
            for user in entries.flatten().map(|e| e.path()) {
                let name = user.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                if user.is_dir() && name != "public" && !users.contains(&user) {
                    users.push(user);
                }
            }
        }
        users
    }

    fn steam_cloud(&self, game: &Game) -> Option<PathBuf> {
        let appid = Self::steam_appid(game)?;
        let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for root in &self.steam_roots {
            let Ok(users) = fs::read_dir(root.join("userdata")) else { continue };
            for user in users.flatten() {
                let remote = user.path().join(&appid).join("remote");
                let has_files = fs::read_dir(&remote).is_ok_and(|mut d| d.next().is_some());
                if has_files {
                    let modified = fs::metadata(&remote).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                    found.push((modified, remote));
                }
            }
        }
        found.into_iter().max_by_key(|(t, _)| *t).map(|(_, p)| p)
    }
}

const GENERIC: &[&str] = &["bin", "bin64", "binaries", "win64", "win32", "x64", "x86", "game", "games", "launcher", "start", "run", "play", "data", "wine", "proton", "umurun", "steam", "heroic"];

type Place = (PathBuf, bool);

fn windows_places(user: &Path) -> Vec<Place> {
    let docs = user.join("Documents");
    let appdata = user.join("AppData");
    vec![
        (user.join("Saved Games"), false),
        (docs.join("My Games"), true),
        (docs.clone(), true),
        (appdata.join("LocalLow"), true),
        (appdata.join("Roaming"), true),
        (appdata.join("Local"), true),
    ]
}

fn native_places() -> Vec<Place> {
    let home = scan::home();
    if cfg!(windows) {
        let mut places = windows_places(&home);
        let env = |var: &str| std::env::var_os(var).map(PathBuf::from);
        if let Some(docs) = directories::UserDirs::new().and_then(|d| d.document_dir().map(Path::to_path_buf)) {
            places.push((docs.join("My Games"), true));
            places.push((docs, true));
        }
        places.extend(env("APPDATA").map(|p| (p, true)));
        places.extend(env("LOCALAPPDATA").map(|p| (p, true)));
        places
    } else if cfg!(target_os = "macos") {
        vec![(home.join("Library/Application Support"), true), (home.join("Documents"), true)]
    } else {
        let xdg = |var: &str, default: &str| {
            std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(default))
        };
        let config = xdg("XDG_CONFIG_HOME", ".config");
        vec![(xdg("XDG_DATA_HOME", ".local/share"), true), (config.join("unity3d"), true), (config, true), (home.join("Documents"), true)]
    }
}

fn search(places: &[Place], names: &[String]) -> Option<PathBuf> {
    let children = |dir: &Path| -> Vec<PathBuf> {
        fs::read_dir(dir).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default()
    };
    let matching = |dirs: &[PathBuf]| dirs.iter().find(|d| d.file_name().is_some_and(|n| matches(&n.to_string_lossy(), names))).cloned();
    for (place, _) in places {
        if let Some(found) = matching(&children(place)) {
            return Some(found);
        }
    }
    for (place, _) in places.iter().filter(|(_, deep)| *deep) {
        for publisher in children(place) {
            if let Some(found) = matching(&children(&publisher)) {
                return Some(found);
            }
        }
    }
    None
}

fn matches(folder: &str, names: &[String]) -> bool {
    const MIN_PARTIAL: usize = 8;
    let folder = normalize(folder);
    if folder.len() < 3 {
        return false;
    }
    names.iter().any(|name| {
        let (short, long) = if folder.len() <= name.len() { (&folder, name) } else { (name, &folder) };
        short == long || (short.len() >= MIN_PARTIAL && long.starts_with(short.as_str()))
    })
}

fn normalize(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| normalize(n)).collect()
    }

    #[test]
    fn folder_names_match_loosely() {
        let n = names(&["The Witcher 3: Wild Hunt"]);
        assert!(matches("The Witcher 3", &n));
        assert!(matches("the-witcher-3-wild-hunt", &n));
        assert!(!matches("Microsoft", &n));
        assert!(!matches("The", &n));
        let n = names(&["Hades"]);
        assert!(matches("Hades", &n));
        assert!(!matches("Hades II", &n), "short names match only whole");
    }

    #[test]
    fn finds_saves_in_a_prefix() {
        let root = std::env::temp_dir().join(format!("yasaff-saves-{}", std::process::id()));
        let user = root.join("drive_c/users/steamuser");
        let gow = user.join("Saved Games/God of War");
        let unity = user.join("AppData/LocalLow/Team Cherry/Hollow Knight");
        fs::create_dir_all(&gow).unwrap();
        fs::create_dir_all(&unity).unwrap();
        fs::create_dir_all(user.join("AppData/Local/Microsoft")).unwrap();
        let places = windows_places(&user);
        assert_eq!(search(&places, &names(&["God of War"])), Some(gow));
        assert_eq!(search(&places, &names(&["Hollow Knight"])), Some(unity));
        assert_eq!(search(&places, &names(&["Celeste"])), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn backs_up_a_save_folder() {
        let root = std::env::temp_dir().join(format!("yasaff-backup-{}", std::process::id()));
        let saves = root.join("saves");
        fs::create_dir_all(saves.join("slot1")).unwrap();
        fs::create_dir_all(saves.join("empty")).unwrap();
        fs::write(saves.join("settings.ini"), "volume=5").unwrap();
        fs::write(saves.join("slot1/save.dat"), [1u8, 2, 3]).unwrap();
        let out = root.join("out");
        fs::create_dir_all(&out).unwrap();

        let zip_path = backup("Game: Name?", &saves, &out).unwrap();
        assert!(zip_path.file_name().unwrap().to_string_lossy().starts_with("Game_ Name_ saves "));
        let mut archive = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
        let mut names: Vec<_> = archive.file_names().map(str::to_owned).collect();
        names.sort();
        assert_eq!(names, ["empty/", "settings.ini", "slot1/save.dat"]);
        let mut data = Vec::new();
        io::Read::read_to_end(&mut archive.by_name("slot1/save.dat").unwrap(), &mut data).unwrap();
        assert_eq!(data, [1, 2, 3]);

        assert!(backup("Game", &saves, &saves.join("slot1")).is_err(), "not into the save folder itself");
        let empty = root.join("nothing");
        fs::create_dir_all(&empty).unwrap();
        assert!(backup("Game", &empty, &out).is_err());
        assert_eq!(fs::read_dir(&out).unwrap().count(), 1, "failed backups leave nothing behind");
        let _ = fs::remove_dir_all(&root);
    }
}

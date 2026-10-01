
use crate::apps;
use crate::library::Game;
use crate::settings::Settings;
use crate::stores::Store;
use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

pub fn launch(game: &Game, settings: &Settings) -> Result<Option<Store>> {
    let target = game.target.trim();
    let path = Path::new(target);

    let entry;
    let (target, args, working_dir) = if path.extension().is_some_and(|e| e == "desktop") && path.is_file() {
        entry = apps::parse_desktop(path).with_context(|| format!("{} is not a valid .desktop file", path.display()))?;
        (entry.target.as_str(), entry.args.as_slice(), entry.working_dir.as_deref())
    } else {
        (target, game.args.as_slice(), game.working_dir.as_deref())
    };

    if let Some((store, uri)) = Store::for_command(target, args)
        && settings.store(store).enabled
        && let Some(command) = settings.store_command(store)
    {
        store.launch_uri(&command, &uri)?;
        return Ok(Some(store));
    }
    run(target, args, working_dir)?;
    Ok(None)
}

fn run(target: &str, args: &[String], working_dir: Option<&Path>) -> Result<()> {
    let path = Path::new(target);

    if is_uri(target) || (path.exists() && !is_native_executable(path)) {
        if cfg!(not(windows)) && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
            bail!(tr!(
                "{file} is a Windows program. Add the game through its Heroic/Lutris/Steam shortcut, \
                 or use \"Edit launch command\" to run it with wine/umu-run.",
                file = path.display()
            ));
        }
        return open::that_detached(target).with_context(|| format!("opening {target}"));
    }

    let mut cmd = Command::new(target);
    cmd.args(args);
    let dir = working_dir.or_else(|| path.parent().filter(|_| path.is_absolute()));
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn().with_context(|| format!("starting {target}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub fn is_uri(s: &str) -> bool {
    s.split_once("://").is_some_and(|(scheme, _)| {
        scheme.len() > 1 && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    })
}

#[cfg(windows)]
fn is_native_executable(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "exe" | "bat" | "cmd" | "com")
}

#[cfg(unix)]
fn is_native_executable(path: &Path) -> bool {
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    let executable = path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if !executable {
        return false;
    }
    let mut magic = [0u8; 4];
    let read = std::fs::File::open(path).and_then(|mut f| f.read(&mut magic)).unwrap_or(0);
    let magic = &magic[..read];
    magic.starts_with(b"\x7fELF") || magic.starts_with(b"#!") || magic.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
}

#[cfg(test)]
mod tests {
    use super::is_uri;

    #[test]
    fn uri_detection() {
        assert!(is_uri("steam://rungameid/570"));
        assert!(is_uri("com.epicgames.launcher://apps/Fortnite?action=launch"));
        assert!(!is_uri(r"C:\Games\game.exe"));
        assert!(!is_uri("/home/user/games/run.sh"));
    }
}

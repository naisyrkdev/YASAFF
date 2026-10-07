
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Store {
    Heroic,
    Steam,
}

impl Store {
    pub const ALL: [Store; 2] = [Store::Heroic, Store::Steam];

    pub fn name(self) -> &'static str {
        match self {
            Store::Heroic => "Heroic Games Launcher",
            Store::Steam => "Steam",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Store::Heroic => "Heroic",
            Store::Steam => "Steam",
        }
    }

    fn uri_scheme(self) -> &'static str {
        match self {
            Store::Heroic => "heroic://",
            Store::Steam => "steam://",
        }
    }

    fn background_args(self) -> &'static [&'static str] {
        match self {
            Store::Heroic => &["--no-gui"],
            Store::Steam => &["-silent"],
        }
    }

    fn process_names(self) -> &'static [&'static str] {
        match self {
            Store::Heroic => &["heroic", "heroic.exe"],
            Store::Steam => &["steam", "steam.exe", "steam_osx"],
        }
    }

    pub fn detect(self) -> Option<Vec<String>> {
        match self {
            Store::Heroic => detect_heroic(),
            Store::Steam => detect_steam(),
        }
    }

    pub fn for_command(target: &str, args: &[String]) -> Option<(Store, String)> {
        std::iter::once(target).chain(args.iter().map(String::as_str)).find_map(|arg| {
            let lower = arg.to_ascii_lowercase();
            Store::ALL.into_iter().find(|s| lower.starts_with(s.uri_scheme())).map(|s| (s, arg.to_owned()))
        })
    }

    pub fn is_running(self) -> bool {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let names = self.process_names();
        sys.processes()
            .values()
            .any(|p| names.iter().any(|n| p.name().to_string_lossy().eq_ignore_ascii_case(n)))
    }

    pub fn start_in_background(self, command: &[String]) -> Result<()> {
        spawn(command, self.background_args().iter().map(|s| s.to_string()))
            .with_context(|| format!("starting {} in the background", self.name()))
    }

    pub fn launch_uri(self, command: &[String], uri: &str) -> Result<()> {
        spawn(command, self.uri_args(uri).into_iter()).with_context(|| format!("launching through {}", self.name()))
    }

    fn uri_args(self, uri: &str) -> Vec<String> {
        self.background_args().iter().map(|s| s.to_string()).chain([uri.to_owned()]).collect()
    }
}

fn spawn(command: &[String], extra: impl Iterator<Item = String>) -> Result<()> {
    let (program, base) = command.split_first().context("empty store command")?;
    let mut child = Command::new(program).args(base).args(extra).spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn in_path(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(&exe)).find(|p| p.is_file())
}

fn path_cmd(p: PathBuf) -> Vec<String> {
    vec![p.to_string_lossy().into_owned()]
}

#[cfg(all(unix, not(target_os = "macos")))]
fn flatpak_installed(app_id: &str) -> bool {
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()).unwrap_or_default();
    [PathBuf::from("/var/lib/flatpak/app"), home.join(".local/share/flatpak/app")]
        .iter()
        .any(|d| d.join(app_id).is_dir())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn flatpak_cmd(app_id: &str) -> Option<Vec<String>> {
    flatpak_installed(app_id).then(|| ["flatpak", "run", app_id].map(String::from).to_vec())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn detect_heroic() -> Option<Vec<String>> {
    in_path("heroic")
        .or_else(|| Some(PathBuf::from("/opt/Heroic/heroic")).filter(|p| p.is_file()))
        .map(path_cmd)
        .or_else(|| flatpak_cmd("com.heroicgameslauncher.hgl"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn detect_steam() -> Option<Vec<String>> {
    in_path("steam").map(path_cmd).or_else(|| flatpak_cmd("com.valvesoftware.Steam"))
}

#[cfg(windows)]
fn detect_heroic() -> Option<Vec<String>> {
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    Some(local.join(r"Programs\heroic\Heroic.exe")).filter(|p| p.is_file()).map(path_cmd)
}

#[cfg(windows)]
fn detect_steam() -> Option<Vec<String>> {
    ["ProgramFiles(x86)", "ProgramFiles"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join(r"Steam\steam.exe"))
        .find(|p| p.is_file())
        .map(path_cmd)
        .or_else(|| in_path("steam").map(path_cmd))
}

#[cfg(target_os = "macos")]
fn detect_heroic() -> Option<Vec<String>> {
    Some(PathBuf::from("/Applications/Heroic.app/Contents/MacOS/Heroic")).filter(|p| p.is_file()).map(path_cmd)
}

#[cfg(target_os = "macos")]
fn detect_steam() -> Option<Vec<String>> {
    Some(PathBuf::from("/Applications/Steam.app/Contents/MacOS/steam_osx")).filter(|p| p.is_file()).map(path_cmd)
}

#[cfg(test)]
mod tests {
    use super::Store;

    #[test]
    fn store_from_command() {
        let args = vec!["heroic://launch?appName=1234567890&runner=gog".to_owned()];
        assert_eq!(
            Store::for_command("xdg-open", &args),
            Some((Store::Heroic, args[0].clone()))
        );
        assert_eq!(
            Store::for_command("steam://rungameid/123456", &[]).map(|s| s.0),
            Some(Store::Steam)
        );
        assert_eq!(Store::for_command("/usr/bin/true", &[]), None);
    }

    #[test]
    fn silent_launch_args() {
        assert_eq!(
            Store::Heroic.uri_args("heroic://launch?appName=1234567890&runner=gog"),
            ["--no-gui", "heroic://launch?appName=1234567890&runner=gog"]
        );
        assert_eq!(Store::Steam.uri_args("steam://rungameid/123456"), ["-silent", "steam://rungameid/123456"]);
    }
}

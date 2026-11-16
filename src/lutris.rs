//! Lutris (Linux): its installed games, read with `lutris --list-games --json --installed`, and
//! when its game database last changed, so YASAFF can follow games added to or removed from it.

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::SystemTime;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Game {
    pub id: u64,
    pub slug: String,
    pub name: String,
}

impl Game {
    /// Lutris's link for the game, passed to the Lutris command to start it.
    pub fn uri(&self) -> String {
        format!("{URI_PREFIX}{}", self.id)
    }

    /// The cover Lutris downloaded for the game, if any.
    pub fn cover(&self) -> Option<PathBuf> {
        data_dirs().into_iter().flat_map(|d| ["coverart", "banners"].map(|k| d.join(k).join(format!("{}.jpg", self.slug)))).find(|p| p.is_file())
    }
}

const URI_PREFIX: &str = "lutris:rungameid/";

/// The Lutris game id a launch command's arguments start, if any.
pub fn id_in(args: &[String]) -> Option<u64> {
    args.iter().find_map(|a| a.strip_prefix(URI_PREFIX)?.parse().ok())
}

/// Lutris's data folders: native and Flatpak.
fn data_dirs() -> Vec<PathBuf> {
    let home = crate::scan::home();
    let data = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    [data.join("lutris"), home.join(".var/app/net.lutris.Lutris/data/lutris")].into_iter().filter(|d| d.is_dir()).collect()
}

/// When Lutris's game database last changed; a sync re-reads the games only then.
pub fn changed_at() -> Option<SystemTime> {
    data_dirs().iter().filter_map(|d| d.join("pga.db").metadata().ok()?.modified().ok()).max()
}

/// The games installed in Lutris.
pub fn installed_games(command: &[String]) -> Result<Vec<Game>> {
    let (program, base) = command.split_first().context("no Lutris command")?;
    let output = Command::new(program)
        .args(base)
        .args(["--list-games", "--json", "--installed"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("running {program}"))?;
    if !output.status.success() {
        bail!("Lutris exited with {}", output.status);
    }
    parse(&String::from_utf8_lossy(&output.stdout))
}

/// Lutris may print other lines before its JSON list.
fn parse(stdout: &str) -> Result<Vec<Game>> {
    let start = stdout.find('[').context("Lutris printed no game list")?;
    let mut games: Vec<Game> = serde_json::from_str(&stdout[start..]).context("reading Lutris's game list")?;
    games.retain(|g| !g.name.trim().is_empty());
    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_game_list() {
        let out = r#"some log line
[{"id": 3, "slug": "hollow-knight", "name": "Hollow Knight", "runner": "wine", "platform": "Windows", "year": 2017, "directory": "/games/hk", "playtime": "1h", "lastplayed": null}]"#;
        let games = parse(out).unwrap();
        assert_eq!(games, [Game { id: 3, slug: "hollow-knight".into(), name: "Hollow Knight".into() }]);
        assert_eq!(games[0].uri(), "lutris:rungameid/3");
        assert_eq!(id_in(&["-d".into(), "lutris:rungameid/3".into()]), Some(3));
        assert_eq!(id_in(&["lutris:rungame/hollow-knight".into()]), None);
        assert!(parse("[]").unwrap().is_empty());
        assert!(parse("nothing").is_err());
    }
}

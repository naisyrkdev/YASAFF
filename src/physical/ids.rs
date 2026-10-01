
use serde::{Deserialize, Serialize};

pub const STEAM: &str = "steam";
pub const GOG: &str = "gog";
pub const EPIC: &str = "epic";
pub const AMAZON: &str = "amazon";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExternalId {
    pub store: String,
    pub id: String,
}

impl ExternalId {
    pub fn new(store: &str, id: &str) -> Self {
        ExternalId { store: store.trim().to_ascii_lowercase(), id: id.trim().to_owned() }
    }

    pub fn matches(&self, other: &ExternalId) -> bool {
        self.store.eq_ignore_ascii_case(&other.store) && self.id == other.id && !self.id.is_empty()
    }
}

pub fn from_command(target: &str, args: &[String]) -> Vec<ExternalId> {
    let argv: Vec<&str> = std::iter::once(target).chain(args.iter().map(String::as_str)).collect();
    let mut ids = Vec::new();
    for (i, arg) in argv.iter().enumerate() {
        let found = from_uri(arg).or_else(|| {
            let lower = arg.to_ascii_lowercase();
            if lower == "-applaunch" {
                argv.get(i + 1).filter(|n| is_digits(n)).map(|n| ExternalId::new(STEAM, n))
            } else {
                lower.strip_prefix("/gameid=").filter(|n| is_digits(n)).map(|n| ExternalId::new(GOG, n))
            }
        });
        if let Some(id) = found
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    ids
}

fn from_uri(uri: &str) -> Option<ExternalId> {
    let (scheme, rest) = uri.trim().split_once("://")?;
    match scheme.to_ascii_lowercase().as_str() {
        "steam" => {
            let mut parts = rest.split('/');
            let verb = parts.next()?.to_ascii_lowercase();
            let id = parts.next()?;
            let id = &id[..id.find(|c: char| !c.is_ascii_digit()).unwrap_or(id.len())];
            (["rungameid", "run", "launch"].contains(&verb.as_str()) && !id.is_empty()).then(|| ExternalId::new(STEAM, id))
        }
        "heroic" => heroic(rest),
        "goggalaxy" => {
            let (verb, id) = rest.split_once('/')?;
            let id = id.split(['/', '?']).next()?;
            (verb.eq_ignore_ascii_case("openGameView") && is_digits(id)).then(|| ExternalId::new(GOG, id))
        }
        "com.epicgames.launcher" => {
            let path = rest.strip_prefix("apps/")?.split('?').next()?;
            let decoded = percent_decode(path);
            let app = decoded.rsplit(':').next()?;
            (!app.is_empty()).then(|| ExternalId::new(EPIC, app))
        }
        _ => None,
    }
}

fn heroic(rest: &str) -> Option<ExternalId> {
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut segments = path.split('/').filter(|s| !s.is_empty());
    if !segments.next()?.eq_ignore_ascii_case("launch") {
        return None;
    }
    let rest: Vec<&str> = segments.collect();
    let (mut app, mut runner) = match rest.as_slice() {
        [app] => (Some(percent_decode(app)), None),
        [runner, app, ..] => (Some(percent_decode(app)), Some(runner.to_ascii_lowercase())),
        [] => (None, None),
    };
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some((k, v)) if k.eq_ignore_ascii_case("appName") => app = Some(percent_decode(v)),
            Some((k, v)) if k.eq_ignore_ascii_case("runner") => runner = Some(v.to_ascii_lowercase()),
            _ => {}
        }
    }
    let app = app.filter(|a| !a.is_empty())?;
    let store = match runner.as_deref() {
        Some("gog") => GOG,
        Some("legendary") => EPIC,
        Some("nile") => AMAZON,
        Some(_) => return None,
        None if is_digits(&app) => GOG,
        None => EPIC,
    };
    Some(ExternalId::new(store, &app))
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (bytes.get(i + 1).and_then(|&b| hex(b)), bytes.get(i + 2).and_then(|&b| hex(b)))
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(target: &str, args: &[&str]) -> Vec<(String, String)> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        from_command(target, &args).into_iter().map(|i| (i.store, i.id)).collect()
    }

    fn one(store: &str, id: &str) -> Vec<(String, String)> {
        vec![(store.to_owned(), id.to_owned())]
    }

    #[test]
    fn steam() {
        assert_eq!(ids("steam://rungameid/570", &[]), one("steam", "570"));
        assert_eq!(ids("steam", &["steam://run/570//-novid"]), one("steam", "570"));
        assert_eq!(ids("steam", &["-applaunch", "570", "-novid"]), one("steam", "570"));
        assert_eq!(ids("steam://store/570", &[]), vec![]);
    }

    #[test]
    fn heroic() {
        assert_eq!(ids("xdg-open", &["heroic://launch?appName=Min&runner=legendary"]), one("epic", "Min"));
        assert_eq!(ids("heroic://launch?appName=1207658924&runner=gog", &[]), one("gog", "1207658924"));
        assert_eq!(ids("heroic://launch?appName=amzn1.adg.product.x&runner=nile", &[]), one("amazon", "amzn1.adg.product.x"));
        assert_eq!(ids("heroic://launch/gog/1207658924", &[]), one("gog", "1207658924"));
        assert_eq!(ids("heroic://launch/1207658924", &[]), one("gog", "1207658924"));
        assert_eq!(ids("heroic://launch?appName=abc&runner=sideload", &[]), vec![]);
    }

    #[test]
    fn gog_and_epic() {
        assert_eq!(ids("goggalaxy://openGameView/1207658924", &[]), one("gog", "1207658924"));
        assert_eq!(ids(r"C:\GOG Galaxy\GalaxyClient.exe", &["/command=runGame", "/gameId=1207658924"]), one("gog", "1207658924"));
        assert_eq!(
            ids("com.epicgames.launcher://apps/fn%3A4fe75bbc5a674f4f9b356b5c90567da5%3AFortnite?action=launch&silent=true", &[]),
            one("epic", "Fortnite")
        );
    }

    #[test]
    fn plain_commands_have_none() {
        assert_eq!(ids("/games/tq/run.sh", &[]), vec![]);
        assert_eq!(ids("lutris", &["lutris:rungameid/3"]), vec![]);
    }

    #[test]
    fn matching_ignores_store_case_only() {
        assert!(ExternalId::new("Steam", "570").matches(&ExternalId::new("steam", "570")));
        assert!(!ExternalId::new("epic", "Min").matches(&ExternalId::new("epic", "min")));
    }
}

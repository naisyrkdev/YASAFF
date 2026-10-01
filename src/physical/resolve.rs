
use super::ids::ExternalId;
use super::manifest::DiscManifest;

pub struct GameIds {
    pub id: String,
    pub external: Vec<ExternalId>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Resolution {
    Launch(usize),
    Choose(Vec<usize>),
    NotFound,
}

pub fn resolve(manifests: &[DiscManifest], games: &[GameIds]) -> Resolution {
    let by_id = candidates(games, |g| manifests.iter().any(|m| m.id.as_deref() == Some(g.id.as_str())));
    let found = if by_id.is_empty() {
        candidates(games, |g| manifests.iter().flat_map(|m| &m.ids).any(|want| g.external.iter().any(|have| have.matches(want))))
    } else {
        by_id
    };
    match found.as_slice() {
        [] => Resolution::NotFound,
        [one] => Resolution::Launch(*one),
        _ => Resolution::Choose(found),
    }
}

fn candidates(games: &[GameIds], wanted: impl Fn(&GameIds) -> bool) -> Vec<usize> {
    games.iter().enumerate().filter(|(_, g)| wanted(g)).map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: &str, ext: &[(&str, &str)]) -> GameIds {
        GameIds { id: id.into(), external: ext.iter().map(|(s, i)| ExternalId::new(s, i)).collect() }
    }

    fn disc(id: Option<&str>, ext: &[(&str, &str)]) -> DiscManifest {
        DiscManifest { id: id.map(Into::into), ids: ext.iter().map(|(s, i)| ExternalId::new(s, i)).collect(), name: None }
    }

    fn library() -> Vec<GameIds> {
        vec![
            game("a", &[("steam", "570")]),
            game("b", &[("gog", "42")]),
            game("c", &[("steam", "42")]),
            game("d", &[]),
        ]
    }

    #[test]
    fn yasaff_id_wins_over_store_ids() {
        assert_eq!(resolve(&[disc(Some("d"), &[("steam", "570")])], &library()), Resolution::Launch(3));
    }

    #[test]
    fn unknown_yasaff_id_falls_back_to_store_ids() {
        assert_eq!(resolve(&[disc(Some("other-pc"), &[("steam", "570")])], &library()), Resolution::Launch(0));
    }

    #[test]
    fn several_matches_ask() {
        assert_eq!(resolve(&[disc(None, &[("gog", "42"), ("steam", "42")])], &library()), Resolution::Choose(vec![1, 2]));
        assert_eq!(resolve(&[disc(Some("a"), &[]), disc(Some("b"), &[])], &library()), Resolution::Choose(vec![0, 1]));
    }

    #[test]
    fn nothing_matches() {
        assert_eq!(resolve(&[disc(None, &[("epic", "570")])], &library()), Resolution::NotFound);
        assert_eq!(resolve(&[], &library()), Resolution::NotFound);
    }
}

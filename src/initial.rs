//! First-run setup: the GitHub repositories every new profile is offered, listed in
//! `initial_content/initial_install_repos.txt`.

use crate::appstore;
use crate::library::Library;
use std::fs;
use std::path::PathBuf;

const DONE: &str = "initialContent.json";
const REPOS: &str = include_str!("../initial_content/initial_install_repos.txt");

pub fn wanted(library: &Library) -> bool {
    if marker().exists() {
        return false;
    }
    if !library.games.is_empty() {
        mark_done();
        return false;
    }
    true
}

pub fn mark_done() {
    if let Err(e) = fs::write(marker(), "{ \"imported\": true }\n") {
        log::warn!("could not write {}: {e}", marker().display());
    }
}

fn marker() -> PathBuf {
    crate::profiles::dir().join(DONE)
}

/// The repositories to offer, as `owner/repo`, in the order they are listed.
pub fn repos() -> Vec<String> {
    parse(REPOS)
}

/// Entries are separated by `;`; spaces and line breaks around them don't matter.
fn parse(text: &str) -> Vec<String> {
    let mut repos: Vec<String> = Vec::new();
    for entry in text.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        match appstore::github_repo(entry) {
            Some(repo) if !repos.contains(&repo) => repos.push(repo),
            Some(_) => {}
            None => log::warn!("initial_install_repos.txt: \"{entry}\" is not a GitHub repository"),
        }
    }
    repos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_repositories() {
        let text = " naisyrkdev/YasaffDoubleAxisTheme;\n https://github.com/a/b ;;not a repo; a/b;\n";
        assert_eq!(parse(text), ["naisyrkdev/YasaffDoubleAxisTheme", "a/b"]);
        assert!(!repos().is_empty(), "every entry of initial_install_repos.txt is a repository");
        assert_eq!(repos().len(), REPOS.split(';').filter(|e| !e.trim().is_empty()).count());
    }
}

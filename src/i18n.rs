
use crate::settings::Language;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

static CURRENT: AtomicU8 = AtomicU8::new(0);

type Table = HashMap<String, Vec<String>>;

static FRENCH: LazyLock<Table> = LazyLock::new(|| parse(include_str!("i18n/fr.json")));
static SPANISH: LazyLock<Table> = LazyLock::new(|| parse(include_str!("i18n/es.json")));
static POLISH: LazyLock<Table> = LazyLock::new(|| parse(include_str!("i18n/pl.json")));

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Entry {
    One(String),
    Forms(Vec<String>),
}

fn parse(json: &str) -> Table {
    let raw: HashMap<String, Entry> = serde_json::from_str(json).expect("translation table is valid JSON");
    raw.into_iter()
        .map(|(key, entry)| match entry {
            Entry::One(text) => (key, vec![text]),
            Entry::Forms(forms) => (key, forms),
        })
        .collect()
}

pub fn set(language: Language) {
    CURRENT.store(Language::ALL.iter().position(|l| *l == language).unwrap_or(0) as u8, Ordering::Relaxed);
}

pub fn current() -> Language {
    Language::ALL.get(CURRENT.load(Ordering::Relaxed) as usize).copied().unwrap_or_default()
}

fn table(language: Language) -> Option<&'static Table> {
    match language {
        Language::English => None,
        Language::French => Some(&FRENCH),
        Language::Spanish => Some(&SPANISH),
        Language::Polish => Some(&POLISH),
    }
}

pub fn tr(english: &'static str) -> &'static str {
    match table(current()).and_then(|t| t.get(english)).and_then(|forms| forms.first()) {
        Some(text) => text,
        None => english,
    }
}

pub fn tr_n(one: &'static str, many: &'static str, n: u64) -> &'static str {
    let language = current();
    let form = plural_form(language, n);
    match table(language).and_then(|t| t.get(many)) {
        Some(forms) if !forms.is_empty() => &forms[form.min(forms.len() - 1)],
        _ if n == 1 => one,
        _ => many,
    }
}

fn plural_form(language: Language, n: u64) -> usize {
    match language {
        Language::English | Language::Spanish => usize::from(n != 1),
        Language::French => usize::from(n > 1),
        Language::Polish if n == 1 => 0,
        Language::Polish if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) => 1,
        Language::Polish => 2,
    }
}

pub fn fill(text: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut out = text.to_owned();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

macro_rules! tr {
    ($english:literal) => {
        $crate::i18n::tr($english)
    };
    ($english:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::fill($crate::i18n::tr($english), &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+])
    };
}

macro_rules! tr_n {
    ($n:expr, $one:literal, $many:literal $(, $name:ident = $value:expr)* $(,)?) => {{
        let n = $n;
        $crate::i18n::fill(
            $crate::i18n::tr_n($one, $many, n as u64),
            &[("n", &n as &dyn std::fmt::Display) $(, (stringify!($name), &$value as &dyn std::fmt::Display))*],
        )
    }};
}

macro_rules! tr_key {
    ($english:literal) => {
        $english
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    fn marked_texts() -> Vec<(String, Option<String>)> {
        let mut texts = Vec::new();
        let mut files = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(path) = files.pop() {
            if path.is_dir() {
                files.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("i18n.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for (macro_name, literals) in [("tr!(", 1), ("tr_key!(", 1), ("tr_n!(", 2)] {
                for (at, _) in source.match_indices(macro_name) {
                    if source[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let mut rest = &source[at + macro_name.len()..];
                    let mut found = Vec::new();
                    while found.len() < literals {
                        let start = rest.find('"').unwrap_or_else(|| panic!("{}: {macro_name} without a text", path.display()));
                        let (text, len) = string_literal(&rest[start..]);
                        found.push(text);
                        rest = &rest[start + len..];
                    }
                    let many = found.pop().unwrap();
                    texts.push((many, found.pop()));
                }
            }
        }
        texts
    }

    fn string_literal(s: &str) -> (String, usize) {
        let mut text = String::new();
        let mut chars = s.char_indices().skip(1);
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => return (text, i + 1),
                '\\' => match chars.next().map(|(_, c)| c) {
                    Some('n') => text.push('\n'),
                    Some('t') => text.push('\t'),
                    Some('\n') => {
                        let rest: String = chars.clone().map(|(_, c)| c).take_while(|c| c.is_whitespace()).collect();
                        for _ in 0..rest.chars().count() {
                            chars.next();
                        }
                    }
                    Some(c) => text.push(c),
                    None => break,
                },
                c => text.push(c),
            }
        }
        panic!("unterminated string literal: {s:.40}");
    }

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{').skip(1).filter_map(|part| part.split_once('}')).map(|(name, _)| name).collect()
    }

    #[test]
    fn every_text_is_translated() {
        let texts = marked_texts();
        assert!(texts.len() > 100, "found only {} texts", texts.len());
        let keys: BTreeSet<&str> = texts.iter().map(|(many, _)| many.as_str()).collect();
        for (code, table) in [("fr", &*FRENCH), ("es", &*SPANISH), ("pl", &*POLISH)] {
            let missing: Vec<_> = keys.iter().filter(|k| !table.contains_key(**k)).collect();
            assert!(missing.is_empty(), "{code}.json lacks {} texts: {missing:#?}", missing.len());
            let unused: Vec<_> = table.keys().filter(|k| !keys.contains(k.as_str())).collect();
            assert!(unused.is_empty(), "{code}.json has texts the code doesn't use: {unused:#?}");
            for (many, one) in &texts {
                let forms = &table[many];
                let plural = one.is_some();
                let wanted = match (plural, code) {
                    (false, _) => 1,
                    (true, "pl") => 3,
                    (true, _) => 2,
                };
                assert_eq!(forms.len(), wanted, "{code}.json: {many:?} needs {wanted} form(s)");
                for form in forms {
                    assert!(!form.trim().is_empty(), "{code}.json: {many:?} is empty");
                    let (mut expected, mut got) = (placeholders(many), placeholders(form));
                    if plural {
                        expected.remove("n");
                        got.remove("n");
                    }
                    assert_eq!(got, expected, "{code}.json: {many:?} → {form:?}");
                }
            }
        }
    }

    #[test]
    fn polish_plurals() {
        let forms: Vec<usize> = [1, 2, 4, 5, 11, 12, 14, 21, 22, 25, 102, 112].map(|n| plural_form(Language::Polish, n)).to_vec();
        assert_eq!(forms, [0, 1, 1, 2, 2, 2, 2, 2, 1, 2, 1, 2]);
    }

    #[test]
    fn fills_values() {
        assert_eq!(fill("Deleted {name} ({n})", &[("name", &"Arial"), ("n", &3)]), "Deleted Arial (3)");
    }
}

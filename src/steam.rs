//! The Steam account: games the user owns and the install state of one game (from its
//! `appmanifest_<appid>.acf`). YASAFF never downloads games itself; Steam does, and this reads how
//! far it got.
//!
//! The owned games come from one of three places, picked in Settings ([`SteamSource`]):
//! - **Sign in**: the user scans a QR code with the Steam mobile app; YASAFF keeps the refresh
//!   token Steam hands out and asks the Web API with it.
//! - **Local files**: a guess from Steam's own files on this computer. No account access, but it
//!   can miss games and list some the account doesn't own.
//! - **API key**: a Steam Web API key the user creates.

use crate::scan::{is_steam_tool, steam_libraries, steam_roots, vdf_strings, vdf_values};
use crate::settings::SteamSource;
use anyhow::{Context as _, Result, bail};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const CACHE_FILE: &str = "steam_owned.json";
/// The Steam sign-in; profile exports leave it out.
pub const SESSION_FILE: &str = "steam_session.json";
const API: &str = "https://api.steampowered.com";
/// `StateFlags` bit Steam sets once every file of the game is on disk.
const FULLY_INSTALLED: u32 = 4;
/// SteamID64 of account number 0; an account's number is its SteamID64 minus this.
const STEAM_ID_BASE: u64 = 76561197960265728;
/// `EAuthTokenPlatformType::MobileApp`: its refresh tokens can be turned into new access tokens.
const PLATFORM_MOBILE: &str = "3";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OwnedGame {
    pub appid: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Install {
    NotInstalled,
    /// Steam has the game queued, is downloading it, or paused it.
    Downloading(Progress),
    Installed,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Progress {
    /// How much is done, `None` until Steam knows the size.
    pub fraction: Option<f32>,
    pub paused: bool,
}

/// The owned list, and why it may be stale or missing.
pub struct Owned {
    pub games: Vec<OwnedGame>,
    pub warning: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    steam_id: String,
    games: Vec<OwnedGame>,
}

/// A Steam sign-in made by scanning the QR code.
#[derive(Serialize, Deserialize, Clone)]
pub struct Session {
    pub steam_id: String,
    pub account_name: String,
    refresh_token: String,
    /// The last access token; Steam's expire after about a day.
    #[serde(default)]
    access_token: String,
}

/// The SteamID64 of the account last logged into Steam on this computer.
pub fn detect_steam_id() -> Option<String> {
    steam_roots().iter().find_map(|root| steam_id_in(&fs::read_to_string(root.join("config/loginusers.vdf")).ok()?))
}

fn steam_id_in(loginusers: &str) -> Option<String> {
    let tokens = vdf_strings(loginusers);
    let is_id = |t: &str| t.len() == 17 && t.starts_with("7656") && t.bytes().all(|b| b.is_ascii_digit());
    let mut current = None;
    let mut first = None;
    for pair in tokens.windows(2) {
        if is_id(&pair[0]) {
            current = Some(pair[0].clone());
            first = first.or_else(|| current.clone());
        } else if pair[0].eq_ignore_ascii_case("MostRecent") && pair[1] == "1" && current.is_some() {
            return current;
        }
    }
    first
}

fn data_file(name: &str) -> PathBuf {
    crate::library::Library::data_dir().join(name)
}

fn read_cache(steam_id: &str) -> Option<Vec<OwnedGame>> {
    let cache: Cache = serde_json::from_str(&fs::read_to_string(data_file(CACHE_FILE)).ok()?).ok()?;
    (cache.steam_id == steam_id).then_some(cache.games)
}

fn write_cache(steam_id: &str, games: &[OwnedGame]) {
    let cache = Cache { steam_id: steam_id.to_owned(), games: games.to_vec() };
    if let Ok(text) = serde_json::to_string(&cache)
        && let Err(e) = crate::fsutil::write_atomic(&data_file(CACHE_FILE), text)
    {
        log::warn!("could not cache the Steam library: {e:#}");
    }
}

/// The account to ask about and how; the scan thread gets this from Settings.
pub struct Account {
    pub source: SteamSource,
    pub api_key: String,
    /// The account logged into Steam here; the sign-in names its own.
    pub detected_id: Option<String>,
}

/// Every game the account owns, without Steam's own tools. When Steam can't be reached the last
/// list fetched is used, and `warning` says so.
pub fn owned_games(account: &Account) -> Owned {
    let fail = |e: anyhow::Error| Owned { games: Vec::new(), warning: Some(tr!("Could not get your Steam library: {error}", error = format!("{e:#}"))) };
    let (steam_id, fetched) = match account.source {
        SteamSource::LocalFiles => {
            return match local_owned(account.detected_id.as_deref()) {
                Ok(games) => Owned { games, warning: None },
                Err(e) => fail(e),
            };
        }
        SteamSource::ApiKey => {
            let Some(id) = account.detected_id.clone() else { return fail(anyhow::anyhow!("no Steam account found; log into Steam on this computer once")) };
            let key = account.api_key.trim();
            if key.is_empty() {
                return fail(anyhow::anyhow!("no Steam Web API key set"));
            }
            let fetched = fetch_owned(&[("key", key)], &id);
            (id, fetched)
        }
        SteamSource::SignIn => {
            let Some(session) = Session::load() else { return fail(anyhow::anyhow!("not signed in; sign in to Steam in Settings")) };
            let fetched = session.access_token().and_then(|token| fetch_owned(&[("access_token", &token)], &session.steam_id));
            (session.steam_id, fetched)
        }
    };
    match fetched {
        Ok(games) => {
            write_cache(&steam_id, &games);
            Owned { games, warning: None }
        }
        Err(e) => {
            log::warn!("fetching the Steam library: {e:#}");
            match read_cache(&steam_id) {
                Some(games) => Owned { games, warning: Some(tr!("Steam library not updated ({error}). Showing the last list.", error = format!("{e:#}"))) },
                None => fail(e),
            }
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_body(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn read_json(response: ureq::http::Response<ureq::Body>) -> Result<serde_json::Value> {
    match response.status().as_u16() {
        200 => {}
        401 | 403 => bail!("Steam refused access"),
        429 => bail!("Steam is rate limiting requests; try again later"),
        code => bail!("Steam answered HTTP {code}"),
    }
    let text = response.into_body().with_config().limit(32 * 1024 * 1024).read_to_string()?;
    Ok(serde_json::from_str(&text)?)
}

fn network(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => anyhow::anyhow!("Steam could not be reached"),
        other => anyhow::anyhow!("{other}"),
    }
}

/// `IPlayerService/GetOwnedGames`, authorized by `auth` (an API key or an access token).
fn fetch_owned(auth: &[(&str, &str)], steam_id: &str) -> Result<Vec<OwnedGame>> {
    let mut request = agent().get(format!("{API}/IPlayerService/GetOwnedGames/v1/"));
    for (name, value) in auth {
        request = request.query(*name, *value);
    }
    let response = request
        .query("steamid", steam_id)
        .query("include_appinfo", "1")
        .query("include_played_free_games", "1")
        .query("format", "json")
        .call()
        .map_err(network)?;
    parse_owned(&read_json(response)?)
}

fn parse_owned(value: &serde_json::Value) -> Result<Vec<OwnedGame>> {
    let response = &value["response"];
    let Some(list) = response["games"].as_array() else {
        if response.get("game_count").is_some() {
            return Ok(Vec::new());
        }
        bail!("Steam returned no games. Set \"Game details\" to Public in your Steam profile's privacy settings");
    };
    let mut games: Vec<OwnedGame> = list
        .iter()
        .filter_map(|g| {
            let appid = g["appid"].as_u64()?.to_string();
            let name = g["name"].as_str()?.trim().to_owned();
            Some(OwnedGame { appid, name })
        })
        .filter(|g| !g.name.is_empty() && !is_steam_tool(&g.appid, &g.name))
        .collect();
    games.sort_by_key(|g| g.name.to_lowercase());
    Ok(games)
}

// ---------------------------------------------------------------------------------------------
// Sign-in with a QR code

impl Session {
    pub fn load() -> Option<Session> {
        serde_json::from_str(&fs::read_to_string(data_file(SESSION_FILE)).ok()?).ok()
    }

    fn save(&self) -> Result<()> {
        crate::fsutil::write_atomic(&data_file(SESSION_FILE), serde_json::to_string(self)?)
    }

    pub fn sign_out() {
        let _ = fs::remove_file(data_file(SESSION_FILE));
    }

    /// An access token for the Web API: the kept one while it is valid, else a new one made from
    /// the refresh token.
    fn access_token(&self) -> Result<String> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        if token_expiry(&self.access_token).is_some_and(|exp| exp > now + 60) {
            return Ok(self.access_token.clone());
        }
        let response = agent()
            .post(format!("{API}/IAuthenticationService/GenerateAccessTokenForApp/v1/"))
            .send_form([("refresh_token", self.refresh_token.as_str()), ("steamid", self.steam_id.as_str())])
            .map_err(network)?;
        let json = read_json(response).context("renewing the Steam sign-in")?;
        json["response"]["access_token"]
            .as_str()
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .context("the Steam sign-in has expired; sign in again in Settings")
    }
}

/// What the sign-in thread tells the QR screen.
pub enum SignIn {
    /// The address to show as a QR code; Steam replaces it now and then.
    Code(String),
    Done(Session),
    Failed(String),
}

/// Runs a QR sign-in until it succeeds, fails or `cancel` is set, reporting through `events`.
/// A code that expires before it is scanned is replaced by a new one.
pub fn sign_in(events: mpsc::Sender<SignIn>, cancel: Arc<AtomicBool>, repaint: impl Fn()) {
    let send = |event| {
        let ok = events.send(event).is_ok();
        repaint();
        ok
    };
    let result = (|| -> Result<Option<Session>> {
        'session: loop {
            let started = begin_qr()?;
            let (mut client_id, request_id, interval) = (started.client_id, started.request_id, started.interval);
            if !send(SignIn::Code(started.url)) {
                return Ok(None);
            }
            loop {
                let wait = Instant::now() + interval;
                while Instant::now() < wait {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(None);
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                match poll_qr(&client_id, &request_id)? {
                    Poll::Waiting => {}
                    Poll::Expired => continue 'session,
                    Poll::NewCode { client_id: id, url } => {
                        client_id = id;
                        if !send(SignIn::Code(url)) {
                            return Ok(None);
                        }
                    }
                    Poll::Approved(session) => return Ok(Some(session)),
                }
            }
        }
    })();
    match result {
        Ok(Some(session)) => {
            let event = match session.save() {
                Ok(()) => SignIn::Done(session),
                Err(e) => SignIn::Failed(format!("{e:#}")),
            };
            send(event);
        }
        Ok(None) => {}
        Err(e) => {
            send(SignIn::Failed(format!("{e:#}")));
        }
    }
}

struct Started {
    client_id: String,
    request_id: String,
    url: String,
    interval: Duration,
}

enum Poll {
    Waiting,
    Expired,
    NewCode { client_id: String, url: String },
    Approved(Session),
}

fn begin_qr() -> Result<Started> {
    let response = agent()
        .post(format!("{API}/IAuthenticationService/BeginAuthSessionViaQR/v1/"))
        .send_form([("device_friendly_name", "YASAFF"), ("platform_type", PLATFORM_MOBILE)])
        .map_err(network)?;
    let json = read_json(response).context("starting the Steam sign-in")?;
    let r = &json["response"];
    let field = |name: &str| r[name].as_str().map(str::to_owned).with_context(|| format!("Steam's answer has no {name}"));
    Ok(Started {
        client_id: field("client_id")?,
        request_id: field("request_id")?,
        url: field("challenge_url")?,
        interval: Duration::from_secs_f64(r["interval"].as_f64().unwrap_or(5.0).clamp(1.0, 30.0)),
    })
}

fn poll_qr(client_id: &str, request_id: &str) -> Result<Poll> {
    let response = agent()
        .post(format!("{API}/IAuthenticationService/PollAuthSessionStatus/v1/"))
        .send_form([("client_id", client_id), ("request_id", request_id)])
        .map_err(network)?;
    // Steam reports an expired or unknown session in this header, with an empty answer.
    let eresult = response.headers().get("x-eresult").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
    if eresult != 1 {
        return Ok(Poll::Expired);
    }
    let json = read_json(response).context("waiting for the Steam sign-in")?;
    Ok(poll_result(&json["response"]))
}

fn poll_result(r: &serde_json::Value) -> Poll {
    let text = |name: &str| r[name].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    if let (Some(refresh_token), Some(access)) = (text("refresh_token"), text("access_token")) {
        let steam_id = token_steam_id(&access).or_else(|| token_steam_id(&refresh_token)).unwrap_or_default();
        let account_name = text("account_name").unwrap_or_default();
        return Poll::Approved(Session { steam_id, account_name, refresh_token, access_token: access });
    }
    match (text("new_client_id"), text("new_challenge_url")) {
        (Some(client_id), Some(url)) => Poll::NewCode { client_id, url },
        _ => Poll::Waiting,
    }
}

/// The claims of a Steam token (a JWT).
fn token_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// When a Steam token stops working, in seconds since 1970: its `exp` claim.
fn token_expiry(token: &str) -> Option<u64> {
    token_claims(token)?["exp"].as_u64()
}

/// The SteamID64 a Steam token was issued to: its `sub` claim.
fn token_steam_id(token: &str) -> Option<String> {
    let claims = token_claims(token)?;
    claims["sub"].as_str().filter(|s| s.bytes().all(|b| b.is_ascii_digit()) && !s.is_empty()).map(str::to_owned)
}

// ---------------------------------------------------------------------------------------------
// The local-files guess

/// Games Steam has seen in this account's library on this computer: apps in the account's
/// `localconfig.vdf` and apps with cached library artwork, named and kept to games by
/// `appinfo.vdf`. A guess: Steam doesn't keep a readable list of what the account owns.
fn local_owned(steam_id: Option<&str>) -> Result<Vec<OwnedGame>> {
    let roots = steam_roots();
    if roots.is_empty() {
        bail!("Steam was not found on this computer");
    }
    let account = steam_id.and_then(|id| id.parse::<u64>().ok()).and_then(|id| id.checked_sub(STEAM_ID_BASE));
    let mut candidates = HashSet::new();
    for root in &roots {
        if let Some(account) = account
            && let Ok(text) = fs::read_to_string(root.join(format!("userdata/{account}/config/localconfig.vdf")))
        {
            candidates.extend(localconfig_apps(&text));
        }
        if let Ok(dir) = fs::read_dir(root.join("appcache/librarycache")) {
            candidates.extend(dir.flatten().filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok()));
        }
    }
    let mut found = HashMap::new();
    for root in &roots {
        if let Ok(data) = fs::read(root.join("appcache/appinfo.vdf")) {
            found.extend(appinfo(&data, &candidates));
        }
    }
    if found.is_empty() {
        bail!("Steam's app list (appinfo.vdf) could not be read");
    }
    let mut games: Vec<OwnedGame> = found
        .into_iter()
        .filter(|(_, app)| app.kind.eq_ignore_ascii_case("game"))
        .map(|(appid, app)| OwnedGame { appid: appid.to_string(), name: app.name })
        .filter(|g| !g.name.is_empty() && !is_steam_tool(&g.appid, &g.name))
        .collect();
    games.sort_by_key(|g| g.name.to_lowercase());
    Ok(games)
}

/// The app ids under `Software/Valve/Steam/apps` in a `localconfig.vdf`.
fn localconfig_apps(text: &str) -> Vec<u32> {
    let mut path: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    let mut apps = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => s.extend(chars.next()),
                        c => s.push(c),
                    }
                }
                // A string after a key is its value; otherwise it's a key.
                pending = if pending.is_some() { None } else { Some(s) };
            }
            '{' => {
                let key = pending.take().unwrap_or_default();
                if path.last().is_some_and(|p| p.eq_ignore_ascii_case("apps"))
                    && path.len() >= 2
                    && path[path.len() - 2].eq_ignore_ascii_case("Steam")
                    && let Ok(id) = key.parse()
                {
                    apps.push(id);
                }
                path.push(key);
            }
            '}' => {
                pending = None;
                path.pop();
            }
            _ => {}
        }
    }
    apps
}

#[derive(Debug, PartialEq)]
struct AppInfo {
    name: String,
    kind: String,
}

/// Name and type (`game`, `tool`, `demo`, …) of each of `wanted` in Steam's binary `appinfo.vdf`
/// (format versions 27 to 29).
fn appinfo(data: &[u8], wanted: &HashSet<u32>) -> HashMap<u32, AppInfo> {
    let mut out = HashMap::new();
    let mut r = Reader { data, pos: 0 };
    let Some(magic) = r.u32() else { return out };
    let version = match magic {
        0x0756_4427 => 27,
        0x0756_4428 => 28,
        0x0756_4429 => 29,
        _ => return out,
    };
    r.u32(); // universe
    let strings = if version >= 29 {
        let Some(offset) = r.u64().and_then(|o| usize::try_from(o).ok()) else { return out };
        let mut t = Reader { data, pos: offset };
        let count = t.u32().unwrap_or(0) as usize;
        (0..count).map_while(|_| t.cstr()).collect()
    } else {
        Vec::new()
    };
    let header = if version >= 28 { 60 } else { 40 };
    while let Some(appid) = r.u32() {
        if appid == 0 {
            break;
        }
        let Some(size) = r.u32() else { break };
        let start = r.pos;
        let end = start.saturating_add(size as usize).min(data.len());
        if wanted.contains(&appid) {
            let mut kv = Reader { data: &data[..end], pos: start + header };
            let mut found = (None, None);
            let mut path = Vec::new();
            kv.object(&strings, version, &mut path, &mut found);
            if let (Some(name), kind) = found {
                out.insert(appid, AppInfo { name, kind: kind.unwrap_or_default() });
            }
        }
        r.pos = end;
    }
    out
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let bytes = self.data.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(bytes)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    fn u64(&mut self) -> Option<u64> {
        self.take(8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }

    fn cstr(&mut self) -> Option<String> {
        let rest = self.data.get(self.pos..)?;
        let len = rest.iter().position(|b| *b == 0)?;
        let s = String::from_utf8_lossy(&rest[..len]).into_owned();
        self.pos += len + 1;
        Some(s)
    }

    /// Walks one binary KeyValues object, keeping `appinfo/common/name` and `…/type`.
    fn object(&mut self, strings: &[String], version: u32, path: &mut Vec<String>, found: &mut (Option<String>, Option<String>)) -> Option<()> {
        loop {
            let kind = self.u8()?;
            if kind == 8 || kind == 11 {
                return Some(());
            }
            let key = if version >= 29 { strings.get(self.u32()? as usize)?.clone() } else { self.cstr()? };
            match kind {
                0 => {
                    path.push(key);
                    self.object(strings, version, path, found)?;
                    path.pop();
                }
                1 => {
                    let value = self.cstr()?;
                    let in_common = path.len() == 2 && path[1].eq_ignore_ascii_case("common");
                    if in_common && key.eq_ignore_ascii_case("name") {
                        found.0 = Some(value);
                    } else if in_common && key.eq_ignore_ascii_case("type") {
                        found.1 = Some(value);
                    }
                }
                2 | 3 | 4 | 6 => {
                    self.take(4)?;
                }
                7 | 10 => {
                    self.take(8)?;
                }
                5 => loop {
                    if self.take(2)? == [0, 0] {
                        break;
                    }
                },
                _ => return None,
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Install state

/// `StateFlags` bit Steam sets while an install is paused.
const UPDATE_PAUSED: u32 = 512;
/// How much of Steam's content log is read: its end has the newest entries.
const LOG_TAIL: u64 = 512 * 1024;

/// The appid's manifest in any of `libraries`, with the library it is in.
fn manifest<'a>(libraries: &'a [PathBuf], appid: &str) -> Option<(&'a PathBuf, String)> {
    libraries.iter().find_map(|l| Some((l, fs::read_to_string(l.join(format!("steamapps/appmanifest_{appid}.acf"))).ok()?)))
}

/// Where Steam is with `appid`, read from the Steam libraries on this computer.
///
/// Steam rewrites the manifest's byte counts only now and then (on pause, stop and finish), so
/// while it downloads, progress is the size of the game's `steamapps/downloading/<appid>` folder
/// against the size its content log gave when the download (re)started.
pub fn install_status(libraries: &[PathBuf], appid: &str) -> Install {
    let Some((library, acf)) = manifest(libraries, appid) else { return Install::NotInstalled };
    let mut status = status_of(&acf);
    if let Install::Downloading(progress) = &mut status {
        let log = steam_roots().iter().find_map(|r| read_tail(&r.join("logs/content_log.txt"), LOG_TAIL)).map(|log| log_state(&log, appid));
        let staging = library.join("steamapps/downloading").join(appid);
        if let Some(log) = log {
            progress.paused |= log.paused;
            if log.stage_total > 0 && staging.is_dir() {
                progress.fraction = Some((dir_size(&staging) as f64 / log.stage_total as f64).clamp(0.0, 1.0) as f32);
            }
        }
    }
    status
}

/// What the manifest alone says: installed, or downloading with its last saved byte counts.
fn status_of(acf: &str) -> Install {
    let tokens = vdf_strings(acf);
    let number = |key: &str| vdf_values(&tokens, key).next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let flags = number("StateFlags") as u32;
    if flags & FULLY_INSTALLED != 0 {
        return Install::Installed;
    }
    let ratio = |done: u64, total: u64| (total > 0).then(|| (done as f64 / total as f64).clamp(0.0, 1.0) as f32);
    let fraction = ratio(number("BytesStaged"), number("BytesToStage")).or_else(|| ratio(number("BytesDownloaded"), number("BytesToDownload")));
    Install::Downloading(Progress { fraction, paused: flags & UPDATE_PAUSED != 0 })
}

/// An app's download as Steam's `content_log.txt` last described it.
#[derive(Debug, Default, PartialEq)]
struct LogState {
    /// Bytes the finished files take, from the last "update started" line.
    stage_total: u64,
    paused: bool,
}

/// Reads lines like `AppID 367520 update started : download 0/1091282960, …, stage 0/5231995691`
/// and `AppID 367520 update canceled : Disabled (Suspended)`.
fn log_state(log: &str, appid: &str) -> LogState {
    let prefix = format!("AppID {appid} ");
    let mut state = LogState::default();
    for line in log.lines() {
        let Some(rest) = line.find(&prefix).map(|i| &line[i + prefix.len()..]) else { continue };
        if let Some(sizes) = rest.strip_prefix("update started :") {
            state.paused = false;
            let total = |name: &str| {
                sizes.split(',').map(str::trim).find_map(|part| part.strip_prefix(name)?.trim().split_once('/')?.1.trim().parse::<u64>().ok())
            };
            state.stage_total = total("stage").filter(|t| *t > 0).or_else(|| total("download")).unwrap_or(0);
        } else if rest.starts_with("update canceled") || rest.contains("(Suspended)") {
            state.paused = true;
        } else if rest.starts_with("App update changed") && rest.contains("Downloading") {
            state.paused = false;
        }
    }
    state
}

/// The last `max` bytes of a file, from the start of a line.
fn read_tail(path: &std::path::Path, max: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(max))).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Some(if len > max { text.split_once('\n').map_or(text.clone(), |(_, rest)| rest.to_owned()) } else { text })
}

/// Bytes the files under `dir` hold.
fn dir_size(dir: &std::path::Path) -> u64 {
    let mut total = 0;
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(current) = dirs.pop() {
        for entry in fs::read_dir(&current).into_iter().flatten().flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => dirs.push(entry.path()),
                Ok(t) if t.is_file() => total += entry.metadata().map_or(0, |m| m.len()),
                _ => {}
            }
        }
    }
    total
}

/// Steam's libraries, when any can be read; without them a missing manifest means "can't tell"
/// rather than "not installed".
pub fn libraries() -> Option<Vec<PathBuf>> {
    Some(steam_libraries()).filter(|l| !l.is_empty())
}

pub fn install_uri(appid: &str) -> String {
    format!("steam://install/{appid}")
}

pub fn run_uri(appid: &str) -> String {
    format!("steam://rungameid/{appid}")
}

/// Steam's own cover art for a game, which works for games that were never installed.
pub fn cdn_art(appid: &str) -> [(&'static str, String); 2] {
    let base = format!("https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{appid}");
    [("cover", format!("{base}/library_600x900.jpg")), ("background", format!("{base}/library_hero.jpg"))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_recent_account_is_picked() {
        let vdf = r#""users"
{
	"76561198000000001" { "AccountName" "old" "MostRecent" "0" "Timestamp" "1600000000" }
	"76561198000000002" { "AccountName" "new" "MostRecent" "1" "Timestamp" "1700000000" }
}"#;
        assert_eq!(steam_id_in(vdf).as_deref(), Some("76561198000000002"));
    }

    #[test]
    fn first_account_without_most_recent() {
        let vdf = r#""users" { "76561198000000001" { "AccountName" "only" } }"#;
        assert_eq!(steam_id_in(vdf).as_deref(), Some("76561198000000001"));
        assert_eq!(steam_id_in(r#""users" { }"#), None);
    }

    #[test]
    fn download_progress() {
        let acf = r#""AppState" { "appid" "570" "StateFlags" "1026" "BytesToDownload" "1000" "BytesDownloaded" "500" "BytesToStage" "4000" "BytesStaged" "1000" }"#;
        assert_eq!(status_of(acf), Install::Downloading(Progress { fraction: Some(0.25), paused: false }));
        let queued = r#""AppState" { "appid" "570" "StateFlags" "1026" }"#;
        assert_eq!(status_of(queued), Install::Downloading(Progress::default()));
        let paused = r#""AppState" { "appid" "570" "StateFlags" "1538" }"#;
        assert!(matches!(status_of(paused), Install::Downloading(Progress { paused: true, .. })));
    }

    #[test]
    fn content_log_progress() {
        let log = "[2026-11-07 11:07:53] AppID 367520 update started : download 0/1091282960, store 0/0, reuse 0/0, delta 0/0, stage 0/5231995691 \n\
            [2026-11-07 11:07:53] AppID 367520 App update changed : Running Update,Downloading,Staging,\n\
            [2026-11-07 11:07:53] AppID 99 update started : download 0/10, stage 0/0\n";
        assert_eq!(log_state(log, "367520"), LogState { stage_total: 5231995691, paused: false });
        assert_eq!(log_state(log, "99").stage_total, 10, "no staging: the download size");
        let paused = format!("{log}[2026-11-07 11:08:54] AppID 367520 update canceled : Disabled (Suspended)\n");
        assert!(log_state(&paused, "367520").paused);
        let resumed = format!("{paused}[2026-11-07 11:20:00] AppID 367520 App update changed : Running Update,Downloading,Staging,\n");
        assert!(!log_state(&resumed, "367520").paused);
        assert_eq!(log_state(log, "1"), LogState::default());
    }

    #[test]
    fn finished_and_needing_update_count_as_installed() {
        assert_eq!(status_of(r#""AppState" { "StateFlags" "4" }"#), Install::Installed);
        assert_eq!(status_of(r#""AppState" { "StateFlags" "6" "BytesToDownload" "10" }"#), Install::Installed);
    }

    #[test]
    fn owned_games_json() {
        let json = serde_json::json!({"response":{"game_count":3,"games":[
            {"appid":570,"name":"Dota 2","playtime_forever":10},
            {"appid":1493710,"name":"Proton Experimental"},
            {"appid":220,"name":"Half-Life 2"}]}});
        let games = parse_owned(&json).unwrap();
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Dota 2", "Half-Life 2"]);
        assert_eq!(games[0].appid, "570");
    }

    #[test]
    fn private_profile_is_explained() {
        assert!(parse_owned(&serde_json::json!({"response":{}})).is_err());
        assert!(parse_owned(&serde_json::json!({"response":{"game_count":0}})).unwrap().is_empty());
    }

    fn jwt(claims: &str) -> String {
        let b64 = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
        format!("{}.{}.sig", b64(r#"{"typ":"JWT","alg":"EdDSA"}"#), b64(claims))
    }

    #[test]
    fn steam_id_from_token() {
        assert_eq!(token_steam_id(&jwt(r#"{"iss":"steam","sub":"76561199833250600","aud":["mobile"]}"#)).as_deref(), Some("76561199833250600"));
        assert_eq!(token_steam_id("not a token"), None);
        assert_eq!(token_expiry(&jwt(r#"{"sub":"1","exp":1800000000}"#)), Some(1800000000));
    }

    #[test]
    fn qr_poll_answers() {
        assert!(matches!(poll_result(&serde_json::json!({"had_remote_interaction": false})), Poll::Waiting));
        let rotated = poll_result(&serde_json::json!({"new_client_id": "2", "new_challenge_url": "https://s.team/q/1/2"}));
        assert!(matches!(rotated, Poll::NewCode { ref client_id, .. } if client_id == "2"));
        let token = jwt(r#"{"sub":"76561198000000002"}"#);
        let approved = poll_result(&serde_json::json!({"refresh_token": token, "access_token": token, "account_name": "ann"}));
        let Poll::Approved(session) = approved else { panic!("not approved") };
        assert_eq!((session.steam_id.as_str(), session.account_name.as_str()), ("76561198000000002", "ann"));
    }

    #[test]
    fn localconfig_lists_library_apps() {
        let vdf = r#""UserLocalConfigStore" { "Software" { "Valve" { "Steam" {
            "apps" { "570" { "LastPlayed" "1" } "220" { "cloud" { "last_sync_state" "synchronized" } } }
            "other" { "999" { } } } } } "apps" { "111" { } } }"#;
        assert_eq!(localconfig_apps(vdf), [570, 220]);
    }

    /// A tiny appinfo.vdf (version 29) with two apps.
    fn appinfo_file() -> Vec<u8> {
        let strings = ["appinfo", "appid", "common", "name", "type"];
        let app = |appid: u32, name: &str, kind: &str| {
            let mut kv = Vec::new();
            let key = |kv: &mut Vec<u8>, t: u8, k: usize| {
                kv.push(t);
                kv.extend((k as u32).to_le_bytes());
            };
            key(&mut kv, 0, 0);
            key(&mut kv, 2, 1);
            kv.extend(appid.to_le_bytes());
            key(&mut kv, 0, 2);
            key(&mut kv, 1, 3);
            kv.extend(name.as_bytes());
            kv.push(0);
            key(&mut kv, 1, 4);
            kv.extend(kind.as_bytes());
            kv.push(0);
            kv.extend([8, 8, 8]);
            let mut entry = appid.to_le_bytes().to_vec();
            entry.extend(((60 + kv.len()) as u32).to_le_bytes());
            entry.extend([0u8; 60]);
            entry.extend(kv);
            entry
        };
        let mut body = app(570, "Dota 2", "Game");
        body.extend(app(228980, "Steamworks Common Redistributables", "Tool"));
        body.extend(0u32.to_le_bytes());
        let mut file = 0x0756_4429u32.to_le_bytes().to_vec();
        file.extend(1u32.to_le_bytes());
        file.extend(((16 + body.len()) as u64).to_le_bytes());
        file.extend(body);
        file.extend((strings.len() as u32).to_le_bytes());
        for s in strings {
            file.extend(s.as_bytes());
            file.push(0);
        }
        file
    }

    #[test]
    fn reads_binary_appinfo() {
        let data = appinfo_file();
        let all = appinfo(&data, &HashSet::from([570, 228980]));
        assert_eq!(all[&570], AppInfo { name: "Dota 2".into(), kind: "Game".into() });
        assert_eq!(all[&228980].kind, "Tool");
        let some = appinfo(&data, &HashSet::from([570]));
        assert_eq!(some.len(), 1);
        assert!(appinfo(b"junk", &HashSet::from([570])).is_empty());
    }
}

#[cfg(test)]
mod this_computer {
    /// Prints the local-files guess for the Steam account on this computer:
    /// `cargo test local_guess -- --ignored --nocapture`.
    /// Prints what YASAFF reads for a game Steam is installing:
    /// `STEAM_APPID=367520 cargo test install_progress -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn install_progress() {
        let appid = std::env::var("STEAM_APPID").unwrap_or_else(|_| "367520".into());
        let libraries = super::libraries().unwrap_or_default();
        println!("{appid}: {:?}", super::install_status(&libraries, &appid));
    }

    #[test]
    #[ignore]
    fn local_guess() {
        let id = super::detect_steam_id();
        let games = super::local_owned(id.as_deref()).unwrap();
        println!("account {id:?}: {} games", games.len());
        for g in &games {
            println!("  {} {}", g.appid, g.name);
        }
    }
}

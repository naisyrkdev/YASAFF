
use crate::library::Library;
use anyhow::{Context as _, Result, bail, ensure};
use egui::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const MANIFEST: &str = "yasaffConfig.json";
const API_KEY_HEADER: &str = "X-Api-Key";
const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ICON_BYTES: u64 = 10 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Manifest {
    pub yasaff: String,
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub author: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub background: Option<String>,
    pub run: BTreeMap<String, RunEntry>,
    pub install: InstallSteps,
    pub git: Option<GitInfo>,
    pub widget: Option<WidgetSpec>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WidgetSpec {
    pub min_height: Option<f32>,
}

pub const ADDON: &str = "addon";
pub const BAR_WIDGET: &str = "bar-widget";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RunEntry {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallSteps {
    pub executables: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GitInfo {
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub tag: Option<String>,
}

impl Manifest {
    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join(MANIFEST);
        let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("{} is not valid", path.display()))
    }

    pub fn run_entry(&self) -> Option<&RunEntry> {
        self.run.get(platform())
    }

    pub fn is_addon(&self) -> bool {
        self.yasaff == ADDON
    }

    pub fn min_height(&self) -> Option<f32> {
        let h = self.widget.as_ref()?.min_height?;
        h.is_finite().then(|| h.clamp(crate::theme::BarStyle::HEIGHT_RANGE.0, crate::theme::BarStyle::HEIGHT_RANGE.1))
    }

    fn validate(&self, root: &Path) -> Result<()> {
        ensure!(matches!(self.yasaff.as_str(), "app" | ADDON), "{MANIFEST}: \"yasaff\" must be \"app\" or \"addon\"");
        ensure!(self.format_version == 1, "{MANIFEST}: format version {} is not supported; update YASAFF", self.format_version);
        ensure!(valid_id(&self.id), "{MANIFEST}: \"{}\" is not a valid id", self.id);
        if self.is_addon() {
            ensure!(self.kind == BAR_WIDGET, "{MANIFEST}: addon kind \"{}\" is not supported; update YASAFF", self.kind);
        }
        let run = self.run_entry().with_context(|| format!("{} can't run on {}", self.name, platform()))?;
        let paths = [Some(&run.command), run.working_dir.as_ref(), self.icon.as_ref(), self.background.as_ref()];
        for path in paths.into_iter().flatten().chain(&self.install.executables) {
            ensure!(safe_relative(path), "{MANIFEST}: {path} is outside the package");
        }
        ensure!(root.join(&run.command).is_file(), "{MANIFEST}: {} is not in the package", run.command);
        Ok(())
    }

    pub fn commit(&self) -> Option<&str> {
        self.git.as_ref()?.commit.as_deref()
    }
}

pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn valid_id(id: &str) -> bool {
    (2..=63).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

fn safe_relative(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

pub fn apps_dir() -> PathBuf {
    Library::data_dir().join("apps")
}

pub fn install_dir(id: &str) -> PathBuf {
    apps_dir().join(id)
}

pub fn addons_dir() -> PathBuf {
    Library::data_dir().join("addons")
}

pub fn addon_dir(id: &str) -> PathBuf {
    addons_dir().join(id)
}

pub fn installed_addons() -> Vec<Manifest> {
    let mut addons: Vec<Manifest> = fs::read_dir(addons_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| Manifest::read(&e.path()).ok())
        .filter(Manifest::is_addon)
        .collect();
    addons.sort_by_key(|m| m.name.to_lowercase());
    addons
}

pub fn uninstall_addon(id: &str) -> Result<()> {
    ensure!(valid_id(id), "invalid addon id {id}");
    let dir = addon_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).with_context(|| format!("deleting {}", dir.display()))?;
    }
    Ok(())
}

pub fn app_data_dir(id: &str) -> PathBuf {
    Library::data_dir().join("appdata").join(id)
}

fn icon_cache_dir() -> PathBuf {
    Library::data_dir().join("cache").join("store")
}

fn preview_cache_dir() -> PathBuf {
    icon_cache_dir().join("themes")
}

fn theme_download_dir() -> PathBuf {
    icon_cache_dir().join("theme-downloads")
}

pub fn installed(id: &str) -> Option<Manifest> {
    Manifest::read(&install_dir(id)).ok()
}

pub fn uninstall(id: &str) -> Result<()> {
    ensure!(valid_id(id), "invalid app id {id}");
    let dir = install_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).with_context(|| format!("deleting {}", dir.display()))?;
    }
    Ok(())
}

pub fn api_base(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        return None;
    }
    let url = if url.contains("://") { url.to_owned() } else { format!("https://{url}") };
    Some(if url.ends_with("/api/v1") { url } else { format!("{url}/api/v1") })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    Fullscreen,
    Window(egui::Rect),
    Free,
}

impl Placement {
    pub fn of(ctx: &Context, fullscreen: bool) -> Self {
        if fullscreen {
            return Placement::Fullscreen;
        }
        ctx.input(|i| i.viewport().inner_rect).filter(|r| r.is_positive()).map_or(Placement::Free, Placement::Window)
    }

    fn words(self) -> String {
        match self {
            Placement::Fullscreen => "fullscreen".into(),
            Placement::Window(r) => format!("window {} {} {} {}", r.left(), r.top(), r.width(), r.height()),
            Placement::Free => "free".into(),
        }
    }
}

pub struct AppProcess {
    pub child: Child,
    to_app: mpsc::Sender<String>,
    home: mpsc::Receiver<()>,
}

impl AppProcess {
    pub fn show(&self, placement: Placement) {
        let _ = self.to_app.send(format!("show {}", placement.words()));
    }

    pub fn wants_home(&self) -> bool {
        self.home.try_iter().count() > 0
    }
}

pub fn launch(ctx: &Context, id: &str, placement: Placement) -> Result<AppProcess> {
    use std::io::BufRead;
    use std::process::Stdio;

    let dir = install_dir(id);
    let manifest = Manifest::read(&dir)?;
    let run = manifest.run_entry().with_context(|| format!("{} can't run on {}", manifest.name, platform()))?;
    let data = app_data_dir(id);
    fs::create_dir_all(&data).with_context(|| format!("creating {}", data.display()))?;
    let mut cmd = Command::new(dir.join(&run.command));
    cmd.args(&run.args)
        .current_dir(run.working_dir.as_ref().map_or(dir.clone(), |w| dir.join(w)))
        .env("YASAFF", "1")
        .env("YASAFF_APP_ID", id)
        .env("YASAFF_APP_VERSION", &manifest.version)
        .env("YASAFF_APP_DATA", &data)
        .env_remove("YASAFF_FULLSCREEN")
        .env_remove("YASAFF_WINDOW")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    match placement {
        Placement::Fullscreen => cmd.env("YASAFF_FULLSCREEN", "1"),
        Placement::Window(_) => cmd.env("YASAFF_WINDOW", placement.words().trim_start_matches("window ")),
        Placement::Free => &mut cmd,
    };
    let mut child = cmd.spawn().with_context(|| format!("starting {}", run.command))?;

    let (to_app, lines) = mpsc::channel::<String>();
    let mut stdin = child.stdin.take().expect("stdin is piped");
    std::thread::spawn(move || {
        for line in lines {
            if writeln!(stdin, "{line}").and_then(|()| stdin.flush()).is_err() {
                break;
            }
        }
    });
    let (home_tx, home) = mpsc::channel();
    let stdout = child.stdout.take().expect("stdout is piped");
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.trim() == "yasaff:home" {
                let _ = home_tx.send(());
                ctx.request_repaint();
            } else {
                println!("{line}");
            }
        }
    });
    Ok(AppProcess { child, to_app, home })
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct StoreInfo {
    pub name: String,
    pub url: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct InfoResponse {
    store: StoreInfo,
    user: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct ListResponse {
    store: StoreInfo,
    apps: Vec<RemoteApp>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct AddonListResponse {
    addons: Vec<RemoteApp>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteApp {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub author: String,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
    pub min_height: Option<f32>,
    pub latest: Release,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
    pub git: Option<GitInfo>,
    pub notes: Option<String>,
    pub package_url: String,
}

impl RemoteApp {
    pub fn is_addon(&self) -> bool {
        self.kind == BAR_WIDGET
    }
}

impl Release {
    pub fn runs_here(&self) -> bool {
        self.platforms.iter().any(|p| p == platform())
    }

    pub fn is_update_for(&self, installed: &Manifest) -> bool {
        if self.version != installed.version {
            return true;
        }
        let remote = self.git.as_ref().and_then(|g| g.commit.as_deref());
        match (remote, installed.commit()) {
            (Some(a), Some(b)) => !(a.starts_with(b) || b.starts_with(a)),
            _ => false,
        }
    }
}

const THEME_SETTINGS: &str = "themeSettings.json";

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct ThemeListResponse {
    themes: Vec<RemoteTheme>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTheme {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub preview_urls: Vec<String>,
    #[serde(default)]
    pub downloads: u64,
    pub latest: ThemeRelease,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ThemeRelease {
    pub version: u32,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
    pub notes: Option<String>,
    pub package_url: String,
}

#[derive(Clone)]
pub struct Endpoint {
    pub base: String,
    pub key: String,
}

pub enum Event {
    Checked(Result<(StoreInfo, String), String>),
    Listed(Result<(StoreInfo, Vec<RemoteApp>), String>),
    Icon(String, PathBuf),
    Progress(String, u64, u64),
    Installed(String, Result<Box<Manifest>, String>),
    Imported(PathBuf, Result<Box<Manifest>, String>),
    ThemesListed(Result<Vec<RemoteTheme>, String>),
    ThemePreview(String, usize, PathBuf),
    ThemeProgress(String, u64, u64),
    ThemeDownloaded(String, Result<PathBuf, String>),
    AddonsListed(Result<Vec<RemoteApp>, String>),
    AddonIcon(String, PathBuf),
}

pub struct Client {
    ctx: Context,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
}

impl Client {
    pub fn new(ctx: Context) -> Self {
        let (tx, rx) = mpsc::channel();
        Client { ctx, tx, rx }
    }

    pub fn poll(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }

    fn spawn(&self, job: impl FnOnce(&ureq::Agent, &dyn Fn(Event)) + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let agent = agent();
            let send = |event| {
                if tx.send(event).is_ok() {
                    ctx.request_repaint();
                }
            };
            job(&agent, &send);
        });
    }

    pub fn check(&self, endpoint: Endpoint) {
        self.spawn(move |agent, send| {
            let result = get_json::<InfoResponse>(agent, &endpoint, &endpoint.base).map(|r| (r.store, r.user));
            send(Event::Checked(result.map_err(|e| format!("{e:#}"))));
        });
    }

    pub fn list(&self, endpoint: Endpoint) {
        self.spawn(move |agent, send| {
            let url = format!("{}/apps?platform={}", endpoint.base, platform());
            let listed = get_json::<ListResponse>(agent, &endpoint, &url);
            let apps = listed.as_ref().map(|r| r.apps.clone()).unwrap_or_default();
            send(Event::Listed(listed.map(|r| (r.store, r.apps)).map_err(|e| format!("{e:#}"))));
            for app in apps {
                if let Some(url) = &app.icon_url
                    && let Ok(path) = cached_icon(agent, &endpoint, url, &app)
                {
                    send(Event::Icon(app.id.clone(), path));
                }
            }
        });
    }

    pub fn list_addons(&self, endpoint: Endpoint) {
        self.spawn(move |agent, send| {
            let url = format!("{}/addons?platform={}", endpoint.base, platform());
            let listed = get_json::<AddonListResponse>(agent, &endpoint, &url).map(|r| r.addons);
            let addons = listed.as_ref().cloned().unwrap_or_default();
            send(Event::AddonsListed(listed.map_err(|e| format!("{e:#}"))));
            for addon in addons {
                if let Some(url) = &addon.icon_url
                    && let Ok(path) = cached_icon(agent, &endpoint, url, &addon)
                {
                    send(Event::AddonIcon(addon.id.clone(), path));
                }
            }
        });
    }

    pub fn import_file(&self, zip: PathBuf, busy: Vec<String>) {
        self.spawn(move |_, send| {
            let result = install_package(&zip, &busy).map(Box::new).map_err(|e| format!("{e:#}"));
            send(Event::Imported(zip, result));
        });
    }

    pub fn install(&self, endpoint: Endpoint, app: RemoteApp) {
        self.spawn(move |agent, send| {
            let result = install(agent, &endpoint, &app, &|done, total| send(Event::Progress(app.id.clone(), done, total)));
            send(Event::Installed(app.id.clone(), result.map(Box::new).map_err(|e| format!("{e:#}"))));
        });
    }

    pub fn list_themes(&self, endpoint: Endpoint) {
        self.spawn(move |agent, send| {
            let url = format!("{}/themes", endpoint.base);
            let listed = get_json::<ThemeListResponse>(agent, &endpoint, &url).map(|r| r.themes);
            let themes = listed.as_ref().cloned().unwrap_or_default();
            send(Event::ThemesListed(listed.map_err(|e| format!("{e:#}"))));
            for theme in themes {
                if let Some(url) = theme.preview_urls.first()
                    && let Ok(path) = cached_preview(agent, &endpoint, &theme.id, url)
                {
                    send(Event::ThemePreview(theme.id.clone(), 0, path));
                }
            }
        });
    }

    pub fn theme_previews(&self, endpoint: Endpoint, theme: RemoteTheme) {
        self.spawn(move |agent, send| {
            for (i, url) in theme.preview_urls.iter().enumerate() {
                match cached_preview(agent, &endpoint, &theme.id, url) {
                    Ok(path) => send(Event::ThemePreview(theme.id.clone(), i, path)),
                    Err(e) => eprintln!("{} preview {i}: {e:#}", theme.id),
                }
            }
        });
    }

    pub fn download_theme(&self, endpoint: Endpoint, theme: RemoteTheme) {
        self.spawn(move |agent, send| {
            let result = download_theme(agent, &endpoint, &theme, &|done, total| send(Event::ThemeProgress(theme.id.clone(), done, total)));
            send(Event::ThemeDownloaded(theme.id.clone(), result.map_err(|e| format!("{e:#}"))));
        });
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn get(agent: &ureq::Agent, endpoint: &Endpoint, url: &str) -> Result<ureq::http::Response<ureq::Body>> {
    let response = agent.get(url).header(API_KEY_HEADER, &endpoint.key).call().map_err(|e| match e {
        ureq::Error::Io(e) => anyhow::anyhow!("{e}"),
        ureq::Error::HostNotFound => anyhow::anyhow!("the store's address was not found"),
        ureq::Error::ConnectionFailed => anyhow::anyhow!("could not connect to the store"),
        other => anyhow::anyhow!("{other}"),
    })?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let text = response.into_body().with_config().limit(64 * 1024).read_to_string().unwrap_or_default();
    let message = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("error")?.as_str().map(str::to_owned));
    match (status.as_u16(), message) {
        (_, Some(m)) => bail!("{m}"),
        (401 | 403, None) => bail!("the store refused the API key"),
        (404, None) => bail!("not found at {url}; check the store address"),
        (code, None) => bail!("the store answered HTTP {code}"),
    }
}

fn get_json<T: serde::de::DeserializeOwned>(agent: &ureq::Agent, endpoint: &Endpoint, url: &str) -> Result<T> {
    let text = get(agent, endpoint, url)?.into_body().with_config().limit(MAX_JSON_BYTES).read_to_string()?;
    serde_json::from_str(&text).context("the address answered, but not like a YASAFF store; check it ends in /api/v1")
}

fn cached_icon(agent: &ureq::Agent, endpoint: &Endpoint, url: &str, app: &RemoteApp) -> Result<PathBuf> {
    let stem = format!("{}-{}", app.id, &hex(&Sha256::digest(format!("{url}\n{}", app.latest.version)))[..12]);
    cached_image(agent, endpoint, url, &icon_cache_dir(), &stem)
}

fn cached_preview(agent: &ureq::Agent, endpoint: &Endpoint, theme_id: &str, url: &str) -> Result<PathBuf> {
    ensure!(valid_id(theme_id), "the store sent an invalid theme id");
    let stem = format!("{theme_id}-{}", &hex(&Sha256::digest(url))[..12]);
    cached_image(agent, endpoint, url, &preview_cache_dir(), &stem)
}

fn cached_image(agent: &ureq::Agent, endpoint: &Endpoint, url: &str, dir: &Path, stem: &str) -> Result<PathBuf> {
    for ext in ["png", "jpg", "webp"] {
        let path = dir.join(format!("{stem}.{ext}"));
        if path.is_file() {
            return Ok(path);
        }
    }
    let bytes = get(agent, endpoint, url)?.into_body().with_config().limit(MAX_ICON_BYTES).read_to_vec()?;
    let ext = match bytes.as_slice() {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [0xFF, 0xD8, 0xFF, ..] => "jpg",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "webp",
        _ => bail!("not an image"),
    };
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{stem}.{ext}"));
    fs::write(&path, bytes)?;
    Ok(path)
}

fn install(agent: &ureq::Agent, endpoint: &Endpoint, app: &RemoteApp, progress: &dyn Fn(u64, u64)) -> Result<Manifest> {
    ensure!(valid_id(&app.id), "the store sent an invalid app id");
    ensure!(app.latest.runs_here(), "{} {} doesn't run on {}", app.name, app.latest.version, platform());
    let root = Library::data_dir();
    fs::create_dir_all(&root)?;
    let zip_path = root.join(format!(".download-{}.zip", app.id));
    let (url, size, sha) = (&app.latest.package_url, app.latest.size, &app.latest.sha256);
    let result = download(agent, endpoint, url, size, sha, &zip_path, progress).and_then(|()| {
        let manifest = install_package(&zip_path, &[])?;
        ensure!(manifest.id == app.id, "the package is for \"{}\", not \"{}\"", manifest.id, app.id);
        ensure!(manifest.is_addon() == (app.kind == BAR_WIDGET), "the store and the package disagree on whether {} is an addon", app.name);
        Ok(manifest)
    });
    let _ = fs::remove_file(&zip_path);
    result
}

fn install_package(zip_path: &Path, busy: &[String]) -> Result<Manifest> {
    install_package_in(&Library::data_dir(), zip_path, busy)
}

fn install_package_in(root: &Path, zip_path: &Path, busy: &[String]) -> Result<Manifest> {
    fs::create_dir_all(root)?;
    let staging = root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        unpack(zip_path, &staging, MANIFEST)?;
        let manifest = Manifest::read(&staging)?;
        manifest.validate(&staging)?;
        ensure!(!busy.contains(&manifest.id), "{} is running; close it first", manifest.name);
        #[cfg(unix)]
        make_executable(&staging, &manifest)?;
        let folder = root.join(if manifest.is_addon() { "addons" } else { "apps" });
        fs::create_dir_all(&folder)?;
        swap_in(&staging, &folder.join(&manifest.id))?;
        Ok(manifest)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

fn download(agent: &ureq::Agent, endpoint: &Endpoint, url: &str, size: u64, sha256: &str, to: &Path, progress: &dyn Fn(u64, u64)) -> Result<()> {
    let response = get(agent, endpoint, url)?;
    let total = response.body().content_length().unwrap_or(size);
    let mut body = response.into_body().into_with_config().limit(MAX_PACKAGE_BYTES).reader();
    let mut file = fs::File::create(to).with_context(|| format!("creating {}", to.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 256 * 1024];
    let (mut done, mut reported) = (0u64, Instant::now());
    progress(0, total);
    loop {
        let n = body.read(&mut buf).context("downloading the package")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        done += n as u64;
        if reported.elapsed() >= Duration::from_millis(100) {
            progress(done, total);
            reported = Instant::now();
        }
    }
    file.flush()?;
    progress(done, total);
    let hash = hex(&hasher.finalize());
    ensure!(sha256.is_empty() || hash.eq_ignore_ascii_case(sha256), "the download is damaged (checksum mismatch); try again");
    Ok(())
}

fn unpack(zip_path: &Path, dir: &Path, marker: &str) -> Result<()> {
    let mut archive = zip::ZipArchive::new(fs::File::open(zip_path)?).context("the package is not a valid .zip")?;
    let names: Vec<PathBuf> = (0..archive.len())
        .map(|i| archive.by_index(i).ok().and_then(|f| f.enclosed_name()).context("the package has an unsafe file path"))
        .collect::<Result<_>>()?;
    let junk = |n: &Path| n.starts_with("__MACOSX") || n.file_name().is_some_and(|f| f == ".DS_Store");
    let prefix = if names.iter().any(|n| n == Path::new(marker)) {
        PathBuf::new()
    } else {
        let tops: std::collections::HashSet<_> = names.iter().filter(|n| !junk(n)).filter_map(|n| n.components().next()).collect();
        match tops.into_iter().next() {
            Some(top) if names.iter().any(|n| *n == Path::new(top.as_os_str()).join(marker)) => PathBuf::from(top.as_os_str()),
            _ => bail!("there is no {marker} in the package"),
        }
    };
    for (i, name) in names.iter().enumerate() {
        if junk(name) {
            continue;
        }
        let Ok(rel) = name.strip_prefix(&prefix) else { continue };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dir.join(rel);
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&out).with_context(|| format!("writing {}", out.display()))?;
        std::io::copy(&mut entry, &mut file)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&out, fs::Permissions::from_mode(mode & 0o755))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(root: &Path, manifest: &Manifest) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let run = manifest.run_entry().map(|r| &r.command);
    for rel in manifest.install.executables.iter().chain(run) {
        let path = root.join(rel);
        if path.is_file() {
            let mode = path.metadata()?.permissions().mode();
            fs::set_permissions(&path, fs::Permissions::from_mode(mode | 0o755))?;
        }
    }
    Ok(())
}

fn download_theme(agent: &ureq::Agent, endpoint: &Endpoint, theme: &RemoteTheme, progress: &dyn Fn(u64, u64)) -> Result<PathBuf> {
    ensure!(valid_id(&theme.id), "the store sent an invalid theme id");
    let root = theme_download_dir();
    fs::create_dir_all(&root)?;
    let token = uuid::Uuid::new_v4();
    let zip_path = root.join(format!("{}-{token}.zip", theme.id));
    let dir = root.join(format!("{}-{token}", theme.id));
    let (url, size, sha) = (&theme.latest.package_url, theme.latest.size, &theme.latest.sha256);
    let result = download(agent, endpoint, url, size, sha, &zip_path, progress)
        .and_then(|()| unpack(&zip_path, &dir, THEME_SETTINGS))
        .and_then(|()| {
            let text = fs::read_to_string(dir.join(THEME_SETTINGS))?;
            serde_json::from_str::<crate::theme::ThemeSettings>(&text).with_context(|| format!("{THEME_SETTINGS} is not one this YASAFF can read"))?;
            Ok(())
        });
    let _ = fs::remove_file(&zip_path);
    if result.is_err() {
        let _ = fs::remove_dir_all(&dir);
    }
    result.map(|()| dir)
}

fn swap_in(staging: &Path, target: &Path) -> Result<()> {
    let old = target.with_file_name(format!(".old-{}", target.file_name().unwrap_or_default().to_string_lossy()));
    let _ = fs::remove_dir_all(&old);
    if target.exists() {
        fs::rename(target, &old).context("the installed version is in use; close the app and try again")?;
    }
    if let Err(e) = fs::rename(staging, target) {
        let _ = fs::rename(&old, target);
        return Err(e).context("moving the new version into place");
    }
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_base_from_what_users_paste() {
        assert_eq!(api_base("store.example.com").as_deref(), Some("https://store.example.com/api/v1"));
        assert_eq!(api_base("http://localhost:5130/").as_deref(), Some("http://localhost:5130/api/v1"));
        assert_eq!(api_base(" http://localhost:5130/api/v1/ ").as_deref(), Some("http://localhost:5130/api/v1"));
        assert_eq!(api_base("  "), None);
    }

    #[test]
    fn updates_by_version_or_commit() {
        let installed = Manifest {
            version: "1.0.0".into(),
            git: Some(GitInfo { commit: Some("abcdef1234567890".into()), ..Default::default() }),
            ..Default::default()
        };
        let release = |version: &str, commit: Option<&str>| Release {
            version: version.into(),
            platforms: vec![],
            size: 0,
            sha256: String::new(),
            git: commit.map(|c| GitInfo { commit: Some(c.into()), ..Default::default() }),
            notes: None,
            package_url: String::new(),
        };
        assert!(release("1.1.0", None).is_update_for(&installed));
        assert!(!release("1.0.0", None).is_update_for(&installed));
        assert!(!release("1.0.0", Some("abcdef1")).is_update_for(&installed));
        assert!(release("1.0.0", Some("0123456")).is_update_for(&installed));
    }

    fn package(dir: &Path, name: &str, manifest: &str, files: &[(&str, &str)]) -> PathBuf {
        use zip::write::SimpleFileOptions;
        let path = dir.join(name);
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (file, text) in [(MANIFEST, manifest)].iter().chain(files) {
            zip.start_file(format!("demo/{file}"), SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn installs_a_package_from_disk() {
        let dir = std::env::temp_dir().join(format!("yasaff-test-{}", uuid::Uuid::new_v4()));
        let root = dir.join("data");
        let apps = root.join("apps");
        fs::create_dir_all(&dir).unwrap();
        let manifest = |version: &str, command: &str| {
            format!(
                r#"{{"yasaff":"app","formatVersion":1,"id":"demo-app","name":"Demo","version":"{version}",
                    "run":{{"{}":{{"command":"{command}"}}}}}}"#,
                platform()
            )
        };
        let script = [("bin/run.sh", "#!/bin/sh\necho hi\n")];

        let v1 = package(&dir, "v1.zip", &manifest("1.0.0", "bin/run.sh"), &script);
        let installed = install_package_in(&root, &v1, &[]).unwrap();
        assert_eq!((installed.id.as_str(), installed.version.as_str()), ("demo-app", "1.0.0"));
        assert!(apps.join("demo-app/bin/run.sh").is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(apps.join("demo-app/bin/run.sh")).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "the command is executable");
        }

        let broken = package(&dir, "broken.zip", &manifest("2.0.0", "bin/missing"), &script);
        assert!(install_package_in(&root, &broken, &[]).is_err());
        let v2 = package(&dir, "v2.zip", &manifest("2.0.0", "bin/run.sh"), &script);
        assert!(install_package_in(&root, &v2, &["demo-app".into()]).is_err());
        assert_eq!(Manifest::read(&apps.join("demo-app")).unwrap().version, "1.0.0");

        assert_eq!(install_package_in(&root, &v2, &[]).unwrap().version, "2.0.0");
        let left: Vec<_> = fs::read_dir(&apps).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left, ["demo-app"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn installs_an_addon_next_to_the_apps() {
        let dir = std::env::temp_dir().join(format!("yasaff-test-{}", uuid::Uuid::new_v4()));
        let root = dir.join("data");
        fs::create_dir_all(&dir).unwrap();
        let manifest = |kind: &str| {
            format!(
                r#"{{"yasaff":"addon","formatVersion":1,"id":"demo-widget","name":"Demo","version":"1.0.0","kind":"{kind}",
                    "widget":{{"minHeight":120}},"run":{{"{}":{{"command":"bin/run.sh"}}}}}}"#,
                platform()
            )
        };
        let script = [("bin/run.sh", "#!/bin/sh\necho hi\n")];
        let unknown = package(&dir, "unknown.zip", &manifest("sidebar"), &script);
        assert!(install_package_in(&root, &unknown, &[]).is_err());
        let widget = package(&dir, "widget.zip", &manifest(BAR_WIDGET), &script);
        let installed = install_package_in(&root, &widget, &[]).unwrap();
        assert!(installed.is_addon());
        assert_eq!(installed.min_height(), Some(120.0));
        assert!(root.join("addons/demo-widget/bin/run.sh").is_file());
        assert!(!root.join("apps").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unpacks_a_theme_zipped_as_a_folder() {
        use zip::write::SimpleFileOptions;
        let dir = std::env::temp_dir().join(format!("yasaff-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("neon.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        for (file, text) in [
            ("Neon/themeSettings.json", r#"{"name":"Neon","background":{"kind":"image","image":"bg.png"}}"#),
            ("Neon/images/bg.png", "png"),
            ("__MACOSX/Neon/._bg.png", "junk"),
        ] {
            zip.start_file(file, SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let out = dir.join("out");
        unpack(&zip_path, &out, THEME_SETTINGS).unwrap();
        assert!(out.join("themeSettings.json").is_file());
        assert!(out.join("images/bg.png").is_file());
        assert!(!out.join("__MACOSX").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn manifest_paths_stay_inside() {
        assert!(safe_relative("bin/linux/app"));
        assert!(!safe_relative("../app"));
        assert!(!safe_relative("/usr/bin/app"));
        assert!(valid_id("wichy-mp3-player"));
        assert!(!valid_id("Wichy"));
        assert!(!valid_id("../x"));
    }
}

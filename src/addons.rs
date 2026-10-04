use crate::appstore::{self, Manifest};
use crate::settings::Settings;
use crate::theme::BarSlot;
use anyhow::{Context as _, Result};
use egui::{Color32, Context};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PREFIX: &str = "yasaff:widget ";
const RESTART_AFTER: Duration = Duration::from_secs(10);

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Frame {
    pub items: Vec<Item>,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Item {
    pub label: String,
    pub value: String,
    pub level: Option<f32>,
    pub color: Option<String>,
    /// When set, the item is drawn as a square dial of these images instead of text.
    pub layers: Vec<Layer>,
}

/// One image of a dial, fitted into the dial's square and turned about its center.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Layer {
    /// Absolute, or relative to the addon's folder.
    pub image: PathBuf,
    /// Clockwise, in degrees.
    pub angle: f32,
}

impl Item {
    pub fn color(&self) -> Option<Color32> {
        crate::theme::parse_hex(self.color.as_deref()?)
    }
}

/// Where a widget is drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Placement {
    Bar(BarSlot),
    /// Anywhere on the screen, at the position in its settings.
    Screen,
}

impl Placement {
    pub const CHOICES: [Option<Placement>; 4] = [None, Some(Self::Bar(BarSlot::Top)), Some(Self::Bar(BarSlot::Bottom)), Some(Self::Screen)];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bar(slot) => slot.label(),
            Self::Screen => tr!("Anywhere on screen"),
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Bar(BarSlot::Top) => "top",
            Self::Bar(BarSlot::Bottom) => "bottom",
            Self::Screen => "screen",
        }
    }
}

pub enum Status<'a> {
    Starting,
    Showing(Frame),
    Failed(&'a str),
}

struct Running {
    child: Child,
    _stdin: ChildStdin,
    place: Placement,
    latest: Arc<Mutex<Option<Frame>>>,
}

struct Stopped {
    at: Instant,
    error: String,
}

pub struct AddonHost {
    ctx: Context,
    pub installed: Vec<Manifest>,
    running: HashMap<String, Running>,
    failed: HashMap<String, Stopped>,
    paused: HashSet<String>,
}

impl AddonHost {
    pub fn new(ctx: Context) -> Self {
        let installed = appstore::installed_addons();
        AddonHost { ctx, installed, running: HashMap::new(), failed: HashMap::new(), paused: HashSet::new() }
    }

    pub fn pause(&mut self, id: &str) {
        self.stop(id);
        self.paused.insert(id.to_owned());
    }

    pub fn resume(&mut self, id: &str) {
        self.paused.remove(id);
    }

    pub fn placement(settings: &Settings, id: &str) -> Option<Placement> {
        let entry = settings.addons.get(id)?;
        if entry.on_screen { Some(Placement::Screen) } else { entry.bar.map(Placement::Bar) }
    }

    pub fn set_placement(settings: &mut Settings, id: &str, place: Option<Placement>) {
        let entry = settings.addons.entry(id.to_owned()).or_default();
        entry.on_screen = place == Some(Placement::Screen);
        if let Some(Placement::Bar(slot)) = place {
            entry.bar = Some(slot);
        } else if place.is_none() {
            entry.bar = None;
        }
    }

    /// The widget's size as a factor of its normal size.
    pub fn scale(settings: &Settings, id: &str) -> f32 {
        settings.addons.get(id).map_or(1.0, |a| a.scale())
    }

    pub fn reload(&mut self, changed: Option<&str>) {
        if let Some(id) = changed {
            self.stop(id);
            self.failed.remove(id);
            self.paused.remove(id);
        }
        self.installed = appstore::installed_addons();
    }

    pub fn get(&self, id: &str) -> Option<&Manifest> {
        self.installed.iter().find(|m| m.id == id)
    }

    pub fn sync(&mut self, settings: &Settings) {
        let exited: Vec<(String, String)> = self
            .running
            .iter_mut()
            .filter_map(|(id, r)| match r.child.try_wait() {
                Ok(Some(status)) => Some((id.clone(), tr!("stopped ({status})", status = status))),
                Ok(None) => None,
                Err(e) => Some((id.clone(), e.to_string())),
            })
            .collect();
        for (id, error) in exited {
            self.running.remove(&id);
            log::warn!("addon {id}: {error}");
            self.failed.insert(id, Stopped { at: Instant::now(), error });
        }

        let wanted: HashMap<String, Placement> =
            self.installed.iter().filter_map(|m| Some((m.id.clone(), Self::placement(settings, &m.id)?))).collect();
        let unwanted: Vec<String> = self.running.iter().filter(|(id, r)| wanted.get(*id) != Some(&r.place)).map(|(id, _)| id.clone()).collect();
        for id in unwanted {
            self.stop(&id);
        }
        for (id, place) in wanted {
            if self.running.contains_key(&id) || self.paused.contains(&id) || self.failed.get(&id).is_some_and(|f| f.at.elapsed() < RESTART_AFTER) {
                continue;
            }
            let Some(manifest) = self.get(&id).cloned() else { continue };
            match self.start(&manifest, place) {
                Ok(running) => {
                    self.failed.remove(&id);
                    self.running.insert(id, running);
                }
                Err(e) => {
                    log::warn!("addon {id}: {e:#}");
                    self.failed.insert(id, Stopped { at: Instant::now(), error: format!("{e:#}") });
                }
            }
        }
        if !self.failed.is_empty() {
            self.ctx.request_repaint_after(RESTART_AFTER);
        }
    }

    pub fn stop(&mut self, id: &str) {
        if let Some(mut r) = self.running.remove(id) {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
    }

    fn start(&self, manifest: &Manifest, place: Placement) -> Result<Running> {
        let dir = appstore::addon_dir(&manifest.id);
        let run = manifest.run_entry().with_context(|| format!("{} can't run on {}", manifest.name, appstore::platform()))?;
        let data = appstore::app_data_dir(&manifest.id);
        std::fs::create_dir_all(&data).with_context(|| format!("creating {}", data.display()))?;
        let mut cmd = Command::new(dir.join(&run.command));
        cmd.args(&run.args)
            .current_dir(run.working_dir.as_ref().map_or(dir.clone(), |w| dir.join(w)))
            .env("YASAFF", "1")
            .env("YASAFF_ADDON_ID", &manifest.id)
            .env("YASAFF_APP_VERSION", &manifest.version)
            .env("YASAFF_APP_DATA", &data)
            .env("YASAFF_WIDGET_BAR", place.word())
            .env_remove("YASAFF_FULLSCREEN")
            .env_remove("YASAFF_WINDOW")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().with_context(|| format!("starting {}", run.command))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let latest: Arc<Mutex<Option<Frame>>> = Arc::default();
        let (shared, ctx, id) = (latest.clone(), self.ctx.clone(), manifest.id.clone());
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                match parse_line(&line, &dir) {
                    Some(Ok(frame)) => {
                        *shared.lock().unwrap() = Some(frame);
                        ctx.request_repaint();
                    }
                    Some(Err(e)) => log::warn!("addon {id}: bad widget line: {e}"),
                    None => log::info!("[{id}] {line}"),
                }
            }
        });
        Ok(Running { child, _stdin: stdin, place, latest })
    }

    pub fn widgets(&self, settings: &Settings, place: Placement) -> Vec<(&Manifest, Status<'_>)> {
        self.installed
            .iter()
            .filter(|m| Self::placement(settings, &m.id) == Some(place))
            .map(|m| {
                let status = match (self.running.get(&m.id), self.failed.get(&m.id)) {
                    (Some(r), _) => r.latest.lock().unwrap().clone().map_or(Status::Starting, Status::Showing),
                    (None, Some(f)) => Status::Failed(&f.error),
                    (None, None) => Status::Starting,
                };
                (m, status)
            })
            .collect()
    }

    /// The tallest bar a widget in `slot` needs at its size, and that widget's name.
    pub fn min_height(&self, settings: &Settings, slot: BarSlot) -> Option<(f32, &str)> {
        let range = crate::theme::BarStyle::HEIGHT_RANGE;
        self.installed
            .iter()
            .filter(|m| Self::placement(settings, &m.id) == Some(Placement::Bar(slot)))
            .filter_map(|m| Some(((m.min_height()? * Self::scale(settings, &m.id)).clamp(range.0, range.1), m.name.as_str())))
            .max_by(|a, b| a.0.total_cmp(&b.0))
    }
}

impl Drop for AddonHost {
    fn drop(&mut self) {
        let ids: Vec<String> = self.running.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }
}

fn parse_line(line: &str, addon_dir: &Path) -> Option<Result<Frame, serde_json::Error>> {
    let json = line.trim().strip_prefix(PREFIX)?;
    Some(serde_json::from_str(json).map(|mut frame: Frame| {
        for layer in frame.items.iter_mut().flat_map(|item| &mut item.layers) {
            if layer.image.is_relative() {
                layer.image = addon_dir.join(&layer.image);
            }
        }
        frame
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_widget_lines() {
        let dir = Path::new("/addons/demo");
        let frame = parse_line(r##"yasaff:widget {"items":[{"label":"CPU","value":"12 %","level":0.12,"color":"#ff0000"}]}"##, dir)
            .unwrap()
            .unwrap();
        assert_eq!(frame.items.len(), 1);
        assert_eq!(frame.items[0].level, Some(0.12));
        assert_eq!(frame.items[0].color(), Some(Color32::from_rgb(255, 0, 0)));
        assert!(frame.items[0].layers.is_empty());
        assert!(parse_line("just logging", dir).is_none());
        assert!(parse_line("yasaff:widget {broken", dir).unwrap().is_err());
    }

    #[test]
    fn places_and_sizes_widgets() {
        let mut settings = Settings::default();
        assert_eq!(AddonHost::placement(&settings, "clock"), None);
        AddonHost::set_placement(&mut settings, "clock", Some(Placement::Bar(BarSlot::Top)));
        assert_eq!(AddonHost::placement(&settings, "clock"), Some(Placement::Bar(BarSlot::Top)));
        AddonHost::set_placement(&mut settings, "clock", Some(Placement::Screen));
        assert_eq!(AddonHost::placement(&settings, "clock"), Some(Placement::Screen));
        AddonHost::set_placement(&mut settings, "clock", None);
        assert_eq!(AddonHost::placement(&settings, "clock"), None);

        let old: crate::settings::AddonSettings = serde_json::from_str(r#"{"bar":"top"}"#).unwrap();
        assert_eq!((old.size, old.x, old.y, old.on_screen), (100, 0.5, 0.5, false));
        assert_eq!(AddonHost::scale(&settings, "clock"), 1.0);
        settings.addons.get_mut("clock").unwrap().size = 1000;
        assert_eq!(AddonHost::scale(&settings, "clock"), 3.0);
    }

    #[test]
    fn resolves_dial_layers_against_the_addon_folder() {
        let (dir, hour) = (Path::new("addons/clock"), std::env::temp_dir().join("hour.png"));
        let item = serde_json::json!({"layers": [{"image": "assets/face.png"}, {"image": hour, "angle": 90.5}]});
        let frame = parse_line(&format!("{PREFIX}{}", serde_json::json!({"items": [item]})), dir).unwrap().unwrap();
        let layers = &frame.items[0].layers;
        assert_eq!(layers[0], Layer { image: dir.join("assets/face.png"), angle: 0.0 });
        assert_eq!(layers[1], Layer { image: hour, angle: 90.5 });
    }
}

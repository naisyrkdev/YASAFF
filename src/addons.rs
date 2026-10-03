use crate::appstore::{self, Manifest};
use crate::settings::Settings;
use crate::theme::BarSlot;
use anyhow::{Context as _, Result};
use egui::{Color32, Context};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::BufRead;
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
}

impl Item {
    pub fn color(&self) -> Option<Color32> {
        crate::theme::parse_hex(self.color.as_deref()?)
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
    slot: BarSlot,
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

    pub fn placement(settings: &Settings, id: &str) -> Option<BarSlot> {
        settings.addons.get(id)?.bar
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
            eprintln!("addon {id}: {error}");
            self.failed.insert(id, Stopped { at: Instant::now(), error });
        }

        let wanted: HashMap<String, BarSlot> =
            self.installed.iter().filter_map(|m| Some((m.id.clone(), Self::placement(settings, &m.id)?))).collect();
        let unwanted: Vec<String> = self.running.iter().filter(|(id, r)| wanted.get(*id) != Some(&r.slot)).map(|(id, _)| id.clone()).collect();
        for id in unwanted {
            self.stop(&id);
        }
        for (id, slot) in wanted {
            if self.running.contains_key(&id) || self.paused.contains(&id) || self.failed.get(&id).is_some_and(|f| f.at.elapsed() < RESTART_AFTER) {
                continue;
            }
            let Some(manifest) = self.get(&id).cloned() else { continue };
            match self.start(&manifest, slot) {
                Ok(running) => {
                    self.failed.remove(&id);
                    self.running.insert(id, running);
                }
                Err(e) => {
                    eprintln!("addon {id}: {e:#}");
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

    fn start(&self, manifest: &Manifest, slot: BarSlot) -> Result<Running> {
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
            .env("YASAFF_WIDGET_BAR", slot_word(slot))
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
                match parse_line(&line) {
                    Some(Ok(frame)) => {
                        *shared.lock().unwrap() = Some(frame);
                        ctx.request_repaint();
                    }
                    Some(Err(e)) => eprintln!("addon {id}: bad widget line: {e}"),
                    None => println!("[{id}] {line}"),
                }
            }
        });
        Ok(Running { child, _stdin: stdin, slot, latest })
    }

    pub fn widgets(&self, settings: &Settings, slot: BarSlot) -> Vec<(&Manifest, Status<'_>)> {
        self.installed
            .iter()
            .filter(|m| Self::placement(settings, &m.id) == Some(slot))
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

    pub fn min_height(&self, settings: &Settings, slot: BarSlot) -> Option<(f32, &str)> {
        self.installed
            .iter()
            .filter(|m| Self::placement(settings, &m.id) == Some(slot))
            .filter_map(|m| Some((m.min_height()?, m.name.as_str())))
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

fn slot_word(slot: BarSlot) -> &'static str {
    match slot {
        BarSlot::Top => "top",
        BarSlot::Bottom => "bottom",
    }
}

fn parse_line(line: &str) -> Option<Result<Frame, serde_json::Error>> {
    let json = line.trim().strip_prefix(PREFIX)?;
    Some(serde_json::from_str(json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_widget_lines() {
        let frame = parse_line(r##"yasaff:widget {"items":[{"label":"CPU","value":"12 %","level":0.12,"color":"#ff0000"}]}"##)
            .unwrap()
            .unwrap();
        assert_eq!(frame.items.len(), 1);
        assert_eq!(frame.items[0].level, Some(0.12));
        assert_eq!(frame.items[0].color(), Some(Color32::from_rgb(255, 0, 0)));
        assert!(parse_line("just logging").is_none());
        assert!(parse_line("yasaff:widget {broken").unwrap().is_err());
    }
}

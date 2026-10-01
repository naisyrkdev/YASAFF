use anyhow::{Context as _, Result};
use egui::{ColorImage, Context, TextureHandle, TextureId, TextureOptions, Vec2};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const EXTS: &[&str] = &["mp4", "webm", "mkv", "mov", "m4v", "avi"];

type Latest = Arc<Mutex<Option<Vec<u8>>>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Format {
    pub size: [usize; 2],
    pub fps: u32,
}

impl Format {
    pub const BUBBLE: Self = Self { size: [480, 480], fps: 30 };
    pub const CRT: Self = Self { size: [480, 360], fps: 30 };
    pub const CRT_IDLE: Self = Self { size: [320, 240], fps: 15 };
}

pub struct VideoPlayer {
    format: Format,
    child: Child,
    latest: Latest,
    texture: Option<TextureHandle>,
}

impl VideoPlayer {
    pub fn start(ctx: &Context, path: &Path, format: Format) -> Result<Self> {
        let [w, h] = format.size;
        let filter = format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},fps={}", format.fps);
        let mut cmd = Command::new(crate::tools::program("ffmpeg"));
        cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-stream_loop", "-1", "-re", "-i"])
            .arg(path)
            .args(["-an", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().context("starting ffmpeg")?;
        let mut stdout = child.stdout.take().context("reading ffmpeg output")?;
        let latest: Latest = Arc::default();
        let shared = latest.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; w * h * 4];
            while stdout.read_exact(&mut buf).is_ok() {
                if let Ok(mut slot) = shared.lock() {
                    *slot = Some(buf.clone());
                }
                ctx.request_repaint();
            }
        });
        Ok(Self { format, child, latest, texture: None })
    }

    pub fn texture(&mut self, ctx: &Context) -> Option<&TextureHandle> {
        if let Some(frame) = self.latest.lock().ok().and_then(|mut slot| slot.take()) {
            let image = ColorImage::from_rgba_unmultiplied(self.format.size, &frame);
            match &mut self.texture {
                Some(texture) => texture.set(image, TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture("video", image, TextureOptions::LINEAR)),
            }
        }
        self.texture.as_ref()
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
pub struct Videos {
    players: HashMap<PathBuf, VideoPlayer>,
    wanted: HashMap<PathBuf, (Format, Instant)>,
    failed: bool,
}

impl Videos {
    pub fn sync(&mut self, ctx: &Context, wanted: &[(PathBuf, Format)], delay: Duration) -> Option<anyhow::Error> {
        self.wanted.retain(|path, (format, _)| wanted.iter().any(|(p, f)| p == path && f == format));
        self.players.retain(|path, player| self.wanted.get(path).is_some_and(|(f, _)| *f == player.format));
        if self.failed {
            return None;
        }
        let mut wait = None::<Duration>;
        for (path, format) in wanted {
            let since = self.wanted.entry(path.clone()).or_insert((*format, Instant::now())).1;
            if self.players.contains_key(path) {
                continue;
            }
            let waited = since.elapsed();
            if waited < delay {
                wait = Some(wait.map_or(delay - waited, |w| w.min(delay - waited)));
                continue;
            }
            match VideoPlayer::start(ctx, path, *format) {
                Ok(player) => {
                    self.players.insert(path.clone(), player);
                }
                Err(e) => {
                    self.failed = true;
                    self.players.clear();
                    return Some(e);
                }
            }
        }
        if let Some(wait) = wait {
            ctx.request_repaint_after(wait);
        }
        None
    }

    pub fn frame(&mut self, ctx: &Context, path: &Path) -> Option<(TextureId, Vec2)> {
        let texture = self.players.get_mut(path)?.texture(ctx)?;
        Some((texture.id(), texture.size_vec2()))
    }
}

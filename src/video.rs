use anyhow::{Context as _, Result};
use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub const EXTS: &[&str] = &["mp4", "webm", "mkv", "mov", "m4v", "avi"];
const FRAME: usize = 480;

type Latest = Arc<Mutex<Option<Vec<u8>>>>;

pub struct VideoPlayer {
    path: PathBuf,
    child: Child,
    latest: Latest,
    texture: Option<TextureHandle>,
}

impl VideoPlayer {
    pub fn start(ctx: &Context, path: &Path) -> Result<Self> {
        let filter = format!("scale={FRAME}:{FRAME}:force_original_aspect_ratio=increase,crop={FRAME}:{FRAME},fps=30");
        let mut cmd = Command::new("ffmpeg");
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
            let mut buf = vec![0u8; FRAME * FRAME * 4];
            while stdout.read_exact(&mut buf).is_ok() {
                if let Ok(mut slot) = shared.lock() {
                    *slot = Some(buf.clone());
                }
                ctx.request_repaint();
            }
        });
        Ok(Self { path: path.to_owned(), child, latest, texture: None })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn texture(&mut self, ctx: &Context) -> Option<&TextureHandle> {
        if let Some(frame) = self.latest.lock().ok().and_then(|mut slot| slot.take()) {
            let image = ColorImage::from_rgba_unmultiplied([FRAME, FRAME], &frame);
            match &mut self.texture {
                Some(texture) => texture.set(image, TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture("bubble-video", image, TextureOptions::LINEAR)),
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

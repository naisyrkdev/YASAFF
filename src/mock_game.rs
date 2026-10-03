use crate::appstore::Manifest;
use egui::{Align2, Color32, Context, FontId, Rect, ViewportCommand, pos2, vec2};
use gilrs::{Button, EventType, Gilrs};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

pub const FLAG: &str = "--mock-game";

#[derive(Clone, Copy, PartialEq)]
enum Placement {
    Fullscreen,
    Window(Rect),
    Free,
}

impl Placement {
    fn parse(words: &str) -> Option<Self> {
        let mut words = words.split_whitespace();
        match words.next()? {
            "fullscreen" => Some(Placement::Fullscreen),
            "free" => Some(Placement::Free),
            "window" => {
                let n: Vec<f32> = words.map(str::parse).collect::<Result<_, _>>().ok()?;
                let [x, y, w, h] = n[..] else { return None };
                (w > 0.0 && h > 0.0).then(|| Placement::Window(Rect::from_min_size(pos2(x, y), vec2(w, h))))
            }
            _ => None,
        }
    }

    fn from_env() -> Self {
        if std::env::var_os("YASAFF_FULLSCREEN").is_some() {
            return Placement::Fullscreen;
        }
        std::env::var("YASAFF_WINDOW").ok().and_then(|w| Placement::parse(&format!("window {w}"))).unwrap_or(Placement::Free)
    }
}

pub fn run(dir: &Path) -> eframe::Result {
    let manifest = Manifest::read(dir).unwrap_or_default();
    let name = if manifest.name.is_empty() { tr!("Mocked game").to_owned() } else { manifest.name.clone() };
    let logo = manifest.artwork.logo.as_ref().or(manifest.icon.as_ref()).map(|l| dir.join(l)).filter(|p| p.is_file());
    let placement = Placement::from_env();
    let mut viewport = egui::ViewportBuilder::default().with_title(&name).with_inner_size([1280.0, 720.0]).with_min_inner_size([640.0, 360.0]);
    viewport = match placement {
        Placement::Fullscreen => viewport.with_fullscreen(true).with_decorations(false),
        Placement::Window(r) => viewport.with_decorations(false).with_resizable(false).with_position(r.min).with_inner_size(r.size()),
        Placement::Free => viewport,
    };
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        &name.clone(),
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(MockGame { name, logo, placement, shows: listen(&cc.egui_ctx), gilrs: Gilrs::new().ok() }))
        }),
    )
}

struct MockGame {
    name: String,
    logo: Option<PathBuf>,
    placement: Placement,
    shows: Option<mpsc::Receiver<Placement>>,
    gilrs: Option<Gilrs>,
}

fn listen(ctx: &Context) -> Option<mpsc::Receiver<Placement>> {
    std::env::var_os("YASAFF")?;
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            let Some(rest) = line.strip_prefix("show") else { continue };
            let placement = if rest.trim().is_empty() { Some(Placement::Free) } else { Placement::parse(rest) };
            if let Some(placement) = placement
                && tx.send(placement).is_ok()
            {
                ctx.request_repaint();
            }
        }
    });
    Some(rx)
}

impl MockGame {
    fn show_again(&mut self, ctx: &Context) {
        let Some(placement) = self.shows.as_ref().and_then(|rx| rx.try_iter().last()) else { return };
        if placement != Placement::Free {
            self.placement = placement;
        }
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.placement == Placement::Fullscreen));
        if let Placement::Window(r) = self.placement {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(r.size()));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(r.min));
        }
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn quit_pressed(&mut self, ctx: &Context) -> bool {
        let focused = ctx.input(|i| i.focused);
        let mut quit = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if let Some(gilrs) = &mut self.gilrs {
            while let Some(event) = gilrs.next_event() {
                quit |= focused && matches!(event.event, EventType::ButtonPressed(Button::East, _));
            }
        }
        quit
    }
}

impl eframe::App for MockGame {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.show_again(ctx);
        if ctx.input(|i| i.viewport().minimized) == Some(true) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.quit_pressed(&ctx) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        let screen = ui.max_rect();
        let s = (screen.height() / 1080.0).max(0.4);
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_rgb(14, 16, 24));

        let logo_box = Rect::from_center_size(screen.center() - vec2(0.0, 90.0 * s), vec2(screen.width() * 0.6, 420.0 * s));
        let below = match &self.logo {
            Some(logo) => {
                let image = egui::Image::new(crate::app::file_uri(logo)).fit_to_exact_size(logo_box.size()).maintain_aspect_ratio(true);
                let size = image.load_and_calc_size(ui, logo_box.size()).unwrap_or(logo_box.size());
                let shown = Rect::from_center_size(logo_box.center(), size);
                image.paint_at(ui, shown);
                shown.bottom()
            }
            None => {
                painter.text(logo_box.center(), Align2::CENTER_CENTER, &self.name, FontId::proportional(72.0 * s), Color32::WHITE);
                logo_box.center().y + 50.0 * s
            }
        };
        let x = screen.center().x;
        painter.text(pos2(x, below + 50.0 * s), Align2::CENTER_TOP, tr!("It's a mocked game, you can delete it now"), FontId::proportional(32.0 * s), Color32::WHITE);
        let hint = tr!("Press B or Esc to close it. Delete it from its Options in YASAFF.");
        painter.text(pos2(x, below + 100.0 * s), Align2::CENTER_TOP, hint, FontId::proportional(20.0 * s), Color32::from_gray(150));

        let every = if ctx.input(|i| i.focused) { 50 } else { 250 };
        ctx.request_repaint_after(Duration::from_millis(every));
    }
}

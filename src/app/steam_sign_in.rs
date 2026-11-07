//! Signing in to Steam by scanning a QR code with the Steam mobile app.

use super::*;
use crate::steam::{self, SignIn};

/// Pixels per QR module in the texture; drawn scaled with nearest filtering, so it stays sharp.
const MODULE_PX: usize = 8;
/// White modules around the code, which phone cameras need to find it.
const QUIET_ZONE: usize = 3;

pub(super) struct SignInScreen {
    events: mpsc::Receiver<SignIn>,
    cancel: Arc<AtomicBool>,
    code: Option<TextureHandle>,
    error: Option<String>,
}

impl Drop for SignInScreen {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl SignInScreen {
    fn start(ctx: &Context) -> Self {
        let (tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (stop, ctx) = (cancel.clone(), ctx.clone());
        std::thread::spawn(move || steam::sign_in(tx, stop, move || ctx.request_repaint()));
        SignInScreen { events, cancel, code: None, error: None }
    }
}

fn qr_image(text: &str) -> Option<egui::ColorImage> {
    let qr = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium).ok()?;
    let modules = qr.size() as usize + 2 * QUIET_ZONE;
    let side = modules * MODULE_PX;
    let mut pixels = vec![Color32::WHITE; side * side];
    for y in 0..side {
        for x in 0..side {
            let (mx, my) = ((x / MODULE_PX) as i32 - QUIET_ZONE as i32, (y / MODULE_PX) as i32 - QUIET_ZONE as i32);
            if qr.get_module(mx, my) {
                pixels[y * side + x] = Color32::BLACK;
            }
        }
    }
    Some(egui::ColorImage::new([side, side], pixels))
}

impl App {
    pub(super) fn open_steam_sign_in(&mut self) {
        self.mode = Mode::SteamSignIn(Box::new(SignInScreen::start(&self.ctx)));
    }

    pub(super) fn poll_steam_sign_in(&mut self) {
        let Mode::SteamSignIn(screen) = &mut self.mode else { return };
        let mut done = None;
        while let Ok(event) = screen.events.try_recv() {
            match event {
                SignIn::Code(url) => match qr_image(&url) {
                    Some(image) => screen.code = Some(self.ctx.load_texture("steam-sign-in", image, TextureOptions::NEAREST)),
                    None => screen.error = Some(tr!("Could not draw the QR code").to_owned()),
                },
                SignIn::Failed(error) => {
                    screen.code = None;
                    screen.error = Some(error);
                }
                SignIn::Done(session) => done = Some(session),
            }
        }
        if let Some(session) = done {
            let name = if session.account_name.is_empty() { session.steam_id } else { session.account_name };
            self.installs.signed_in = Some(name.clone());
            self.mode = self.settings_at(SettingRow::SteamSignIn);
            self.notify(tr!("Signed in to Steam as {name}", name = name));
        }
    }

    pub(super) fn handle_steam_sign_in(&mut self, action: Action) {
        let Mode::SteamSignIn(screen) = &self.mode else { return };
        match action {
            Action::Confirm if screen.error.is_some() => self.open_steam_sign_in(),
            Action::Back => self.mode = self.settings_at(SettingRow::SteamSignIn),
            _ => {}
        }
    }

    pub(super) fn draw_steam_sign_in(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::SteamSignIn(sign_in) = &self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(160));
        let dialog = Rect::from_center_size(screen.center(), vec2(720.0 * s, 600.0 * s).min(screen.size() - vec2(32.0, 32.0)));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        painter.text(dialog.center_top() + vec2(0.0, 44.0 * s), Align2::CENTER_CENTER, tr!("Sign in to Steam"), FontId::proportional(28.0 * s), Color32::WHITE);

        let side = (300.0 * s).min(dialog.height() - 230.0 * s).max(120.0 * s);
        let code = Rect::from_center_size(pos2(dialog.center().x, dialog.top() + 84.0 * s + side / 2.0), vec2(side, side));
        match (&sign_in.code, &sign_in.error) {
            (_, Some(error)) => {
                let text = tr!("Signing in failed: {error}", error = error);
                let galley = painter.layout(text, FontId::proportional(18.0 * s), PAD_B, dialog.width() - 80.0 * s);
                painter.galley(code.center() - galley.size() / 2.0, galley, PAD_B);
            }
            (Some(texture), None) => {
                painter.image(texture.id(), code, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            (None, None) => {
                let size = 48.0 * s;
                egui::Spinner::new().size(size).color(Color32::WHITE).paint_at(ui, Rect::from_center_size(code.center(), vec2(size, size)));
            }
        }

        let help = if sign_in.error.is_some() {
            tr!("Press Enter to try again, or Back to cancel.")
        } else {
            tr!("Open the Steam app on your phone, tap the shield (Steam Guard), then \"Scan a QR code\" and approve. YASAFF only reads which games you own.")
        };
        let mut job = egui::text::LayoutJob::simple(help.to_owned(), FontId::proportional(17.0 * s), Color32::from_gray(190), dialog.width() - 80.0 * s);
        job.halign = egui::Align::Center;
        let galley = painter.layout_job(job);
        painter.galley(pos2(dialog.center().x, code.bottom() + 24.0 * s), galley, Color32::from_gray(190));
    }
}

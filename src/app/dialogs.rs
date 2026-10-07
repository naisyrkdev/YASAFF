//! Native file dialogs, run off the UI thread so the launcher keeps drawing and reading the
//! controller while one is open. The result is handed to a continuation on a later frame.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum Pick {
    File,
    Files,
    Folder,
    Folders,
    Save,
}

/// What to do with the paths the user chose.
type Continuation = Box<dyn FnOnce(&mut App, Vec<PathBuf>)>;

pub(super) struct PendingDialog {
    rx: mpsc::Receiver<Vec<PathBuf>>,
    then: Continuation,
}

impl App {
    /// Opens `dialog` on a worker thread; `then` runs with the chosen paths, or not at all when the
    /// user cancels. Only one dialog is open at a time.
    pub(super) fn ask_paths(&mut self, dialog: rfd::FileDialog, pick: Pick, then: impl FnOnce(&mut App, Vec<PathBuf>) + 'static) {
        if self.dialog.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        let spawned = std::thread::Builder::new().name("file-dialog".into()).spawn(move || {
            let picked = match pick {
                Pick::File => dialog.pick_file().into_iter().collect(),
                Pick::Files => dialog.pick_files().unwrap_or_default(),
                Pick::Folder => dialog.pick_folder().into_iter().collect(),
                Pick::Folders => dialog.pick_folders().unwrap_or_default(),
                Pick::Save => dialog.save_file().into_iter().collect(),
            };
            let _ = tx.send(picked);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.dialog = Some(PendingDialog { rx, then: Box::new(then) }),
            Err(e) => self.notify(tr!("Could not open the file dialog: {error}", error = e)),
        }
    }

    /// `ask_paths` for dialogs that return a single path.
    pub(super) fn ask_path(&mut self, dialog: rfd::FileDialog, pick: Pick, then: impl FnOnce(&mut App, PathBuf) + 'static) {
        self.ask_paths(dialog, pick, move |app, paths| {
            if let Some(path) = paths.into_iter().next() {
                then(app, path);
            }
        });
    }

    pub(super) fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub(super) fn poll_dialog(&mut self) {
        let Some(pending) = &self.dialog else { return };
        let paths = match pending.rx.try_recv() {
            Ok(paths) => paths,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Vec::new(),
        };
        let Some(pending) = self.dialog.take() else { return };
        if !paths.is_empty() {
            (pending.then)(self, paths);
        }
    }

    pub(super) fn draw_dialog_wait(&self, painter: &Painter, screen: Rect, s: f32) {
        if !self.dialog_open() {
            return;
        }
        painter.rect_filled(screen, 0, Color32::from_black_alpha(150));
        painter.text(
            screen.center(),
            Align2::CENTER_CENTER,
            tr!("Finish choosing in the file window…"),
            FontId::proportional(26.0 * s),
            Color32::WHITE,
        );
    }
}

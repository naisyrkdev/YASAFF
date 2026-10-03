//! A minimal logger: every record goes to stderr and to `<data>/logs/yasaff.log`, so problems can be
//! diagnosed on Windows release builds too, where there is no console.

use log::{LevelFilter, Log, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// The current log is moved to `yasaff.old.log` at startup once it grows past this.
const ROTATE_AT: u64 = 2 * 1024 * 1024;

struct Logger {
    file: Option<Mutex<File>>,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{} {:<5} [{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), record.level(), record.target(), record.args());
        eprintln!("{line}");
        if let Some(file) = &self.file {
            let mut file = file.lock().unwrap_or_else(|e| e.into_inner());
            let _ = writeln!(file, "{line}");
        }
    }

    fn flush(&self) {
        if let Some(file) = &self.file {
            let _ = file.lock().unwrap_or_else(|e| e.into_inner()).flush();
        }
    }
}

pub fn dir() -> PathBuf {
    crate::profiles::root().join("logs")
}

pub fn init() {
    let file = open().map_err(|e| eprintln!("can't open the log file: {e}")).ok();
    let level = if cfg!(debug_assertions) { LevelFilter::Debug } else { LevelFilter::Info };
    if log::set_boxed_logger(Box::new(Logger { file: file.map(Mutex::new) })).is_ok() {
        log::set_max_level(level);
    }
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}\n{}", std::backtrace::Backtrace::force_capture());
        log::logger().flush();
    }));
}

fn open() -> std::io::Result<File> {
    let dir = dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join("yasaff.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() > ROTATE_AT) {
        fs::rename(&path, dir.join("yasaff.old.log"))?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

use anyhow::{Context, Result, ensure};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_millis(500);
const CHECK_IN_EVERY: Duration = Duration::from_secs(1);

pub struct GlobalMouse {
    connected: Arc<AtomicBool>,
}

impl GlobalMouse {
    pub fn spawn(ctx: egui::Context) -> Self {
        let connected = Arc::new(AtomicBool::new(false));
        let flag = connected.clone();
        let spawned = std::thread::Builder::new().name("global-mouse".into()).spawn(move || {
            loop {
                let now = check_in().is_ok();
                if flag.swap(now, Ordering::Relaxed) != now {
                    ctx.request_repaint();
                }
                std::thread::sleep(CHECK_IN_EVERY);
            }
        });
        if let Err(e) = spawned {
            eprintln!("can't look for Global Virtual Mouse: {e}");
        }
        Self { connected }
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}

fn port_file() -> PathBuf {
    directories::ProjectDirs::from("", "", "GlobalVirtualMouse")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("GlobalVirtualMouse"))
        .join("daemon.port")
}

fn check_in() -> Result<()> {
    let text = std::fs::read_to_string(port_file())?;
    let (port, token) = text.trim().split_once(' ').context("bad port file")?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port.parse::<u16>()?));
    let stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    (&stream).write_all(format!("{token} yasaff\n").as_bytes())?;
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply)?;
    ensure!(reply.trim() == "ok", "unexpected reply {reply:?}");
    Ok(())
}

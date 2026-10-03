
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq)]
pub struct Burner {
    pub device: PathBuf,
    pub name: String,
    pub writes: Vec<&'static str>,
    pub media: Media,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Media {
    None,
    Blank(String),
    Rewritable(String),
    Used(String),
    ReadOnly(String),
}

impl Media {
    pub fn can_burn(&self) -> bool {
        matches!(self, Media::Blank(_) | Media::Rewritable(_))
    }

    pub fn erases(&self) -> bool {
        matches!(self, Media::Rewritable(_))
    }

    fn kind(&self) -> &str {
        match self {
            Media::None => "",
            Media::Blank(k) | Media::Rewritable(k) | Media::Used(k) | Media::ReadOnly(k) => k,
        }
    }

    fn is_cd(&self) -> bool {
        self.kind().starts_with("CD")
    }
}

impl fmt::Display for Media {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Media::None => f.write_str(tr!("No disc")),
            Media::Blank(k) => f.write_str(&tr!("Blank {kind}", kind = k)),
            Media::Rewritable(k) => f.write_str(&tr!("{kind} with data", kind = k)),
            Media::Used(k) => f.write_str(&tr!("Used {kind}", kind = k)),
            Media::ReadOnly(k) => f.write_str(&tr!("{kind} (can't be written)", kind = k)),
        }
    }
}

pub fn burners() -> Vec<Burner> {
    platform::burners()
}

fn media_from_props(props: &HashMap<String, String>) -> Media {
    let on = |key: &str| props.get(key).is_some_and(|v| v == "1");
    if !on("ID_CDROM_MEDIA") {
        return Media::None;
    }
    const KINDS: [(&str, &str, bool); 16] = [
        ("BD_RE", "BD-RE", true),
        ("BD_R", "BD-R", false),
        ("BD", "BD-ROM", false),
        ("DVD_PLUS_RW_DL", "DVD+RW DL", true),
        ("DVD_PLUS_RW", "DVD+RW", true),
        ("DVD_PLUS_R_DL", "DVD+R DL", false),
        ("DVD_PLUS_R", "DVD+R", false),
        ("DVD_RW_SEQ", "DVD-RW", true),
        ("DVD_RW_RO", "DVD-RW", true),
        ("DVD_RW", "DVD-RW", true),
        ("DVD_RAM", "DVD-RAM", true),
        ("DVD_R", "DVD-R", false),
        ("DVD", "DVD-ROM", false),
        ("CD_RW", "CD-RW", true),
        ("CD_R", "CD-R", false),
        ("CD", "CD-ROM", false),
    ];
    let Some(&(_, kind, rewritable)) = KINDS.iter().find(|(key, ..)| on(&format!("ID_CDROM_MEDIA_{key}"))) else {
        return Media::ReadOnly(tr!("Disc").into());
    };
    let kind = kind.to_owned();
    let pressed = kind.ends_with("-ROM");
    match props.get("ID_CDROM_MEDIA_STATE").map(String::as_str) {
        Some("blank") if !pressed => Media::Blank(kind),
        _ if rewritable => Media::Rewritable(kind),
        _ if pressed => Media::ReadOnly(kind),
        _ => Media::Used(kind),
    }
}

fn writes_from_props(props: &HashMap<String, String>) -> Vec<&'static str> {
    let on = |key: &str| props.get(&format!("ID_CDROM_{key}")).is_some_and(|v| v == "1");
    let mut writes = Vec::new();
    if on("CD_R") || on("CD_RW") {
        writes.push("CD");
    }
    if ["DVD_R", "DVD_RW", "DVD_PLUS_R", "DVD_PLUS_RW", "DVD_RAM"].iter().any(|k| on(k)) {
        writes.push("DVD");
    }
    if on("BD_R") || on("BD_RE") {
        writes.push("BD");
    }
    writes
}

#[derive(Clone, Debug)]
pub struct BurnJob {
    pub device: PathBuf,
    pub label: String,
    pub files: Vec<(String, Vec<u8>)>,
    pub media: Media,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub fraction: Option<f32>,
    pub status: String,
}

impl Progress {
    fn stage(status: &str) -> Self {
        Progress { fraction: None, status: status.into() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tool {
    Xorriso,
    Wodim(&'static str),
    Growisofs,
}

fn installed(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

fn pick_tool(media: &Media) -> Result<Tool, String> {
    if installed("xorriso") {
        return Ok(Tool::Xorriso);
    }
    if media.is_cd() {
        if let Some(p) = ["wodim", "cdrecord"].into_iter().find(|p| installed(p)) {
            return Ok(Tool::Wodim(p));
        }
    } else if installed("growisofs") {
        return Ok(Tool::Growisofs);
    }
    Err(tr!("No disc burning program found. Install xorriso (e.g. sudo apt install xorriso).").into())
}

fn iso_maker() -> Result<&'static str, String> {
    ["genisoimage", "mkisofs"]
        .into_iter()
        .find(|p| installed(p))
        .ok_or_else(|| tr!("Burning a CD with wodim needs genisoimage (or install xorriso instead).").into())
}

pub fn burn(job: &BurnJob, mut progress: impl FnMut(Progress)) -> Result<(), String> {
    if !job.media.can_burn() {
        return Err(tr!("{disc} in the drive can't be burned", disc = job.media));
    }
    let tool = pick_tool(&job.media)?;
    progress(Progress::stage(tr!("Preparing files")));
    let work = std::env::temp_dir().join(format!(
        "yasaff-burn-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
    ));
    let result = burn_in(job, tool, &work, &mut progress);
    let _ = fs::remove_dir_all(&work);
    result
}

fn burn_in(job: &BurnJob, tool: Tool, work: &Path, progress: &mut impl FnMut(Progress)) -> Result<(), String> {
    let root = work.join("root");
    fs::create_dir_all(&root).map_err(|e| tr!("Could not prepare files: {error}", error = e))?;
    for (name, data) in &job.files {
        fs::write(root.join(name), data).map_err(|e| tr!("Could not prepare {file}: {error}", file = name, error = e))?;
    }
    let label: String = job.label.chars().take(32).collect();
    let device = job.device.to_string_lossy().into_owned();
    let root_arg = root.to_string_lossy().into_owned();
    let erase = job.media.erases();
    if erase {
        let _ = Command::new("udisksctl")
            .args(["unmount", "--no-user-interaction", "-b", &device])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let start = if erase { tr!("Erasing the disc") } else { tr!("Starting the burner (calibrating can take a minute)") };
    let mut wrote = false;
    macro_rules! report {
        () => {
            &mut |line: &str| {
                if let Some(p) = parse_progress(line) {
                    wrote |= p.fraction.is_some_and(|f| f > 0.0);
                    progress(p);
                }
            }
        };
    }
    let mut ejects = true;
    match tool {
        Tool::Xorriso => {
            let mut args = vec!["-abort_on", "FAILURE", "-outdev", &device];
            if erase {
                args.extend(["-blank", "as_needed"]);
            }
            args.extend(["-volid", &label, "-joliet", "on", "-map", &root_arg, "/", "-commit_eject", "all"]);
            progress(Progress::stage(start));
            run("xorriso", &args, report!()).map_err(|f| f.explain())?;
        }
        Tool::Wodim(program) => {
            ejects = false;
            progress(Progress::stage(tr!("Making the disc image")));
            let iso = work.join("disc.iso");
            let iso_arg = iso.to_string_lossy().into_owned();
            run(iso_maker()?, &["-quiet", "-V", &label, "-J", "-r", "-o", &iso_arg, &root_arg], &mut |_| {})
                .map_err(|f| f.explain())?;
            let dev = format!("dev={device}");
            let mut args = vec!["-v", &dev, "gracetime=2", "timeout=300", "driveropts=burnfree", "-pad", "-data"];
            if erase {
                args.push("blank=fast");
            }
            args.push(&iso_arg);
            progress(Progress::stage(start));
            if let Err(first) = run(program, &args, report!()) {
                if wrote || !first.calibration() {
                    return Err(first.explain());
                }
                log::warn!("burn: {program} failed before writing; retrying in SAO mode at 4x");
                progress(Progress::stage(tr!("Calibration failed; trying again at a slower speed")));
                let last = args.pop().unwrap();
                args.extend(["-dao", "speed=4", last]);
                run(program, &args, report!()).map_err(|f| f.explain())?;
            }
        }
        Tool::Growisofs => {
            ejects = false;
            progress(Progress::stage(start));
            let args = ["-dvd-compat", "-Z", &device, "-V", &label, "-J", "-r", &root_arg];
            run("growisofs", &args, report!()).map_err(|f| f.explain())?;
        }
    }
    if !ejects {
        progress(Progress::stage(tr!("Ejecting")));
        let _ = Command::new("eject").arg(&device).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    progress(Progress { fraction: Some(1.0), status: tr!("Done").into() });
    Ok(())
}

struct Failure {
    program: String,
    status: String,
    lines: Vec<String>,
}

impl Failure {
    fn says(&self, text: &str) -> bool {
        let text = text.to_ascii_lowercase();
        self.lines.iter().any(|l| l.to_ascii_lowercase().contains(&text))
    }

    fn calibration(&self) -> bool {
        self.says("OPC failed") || self.says("power calibration") || self.says("timeout")
    }

    fn explain(&self) -> String {
        let hint = if self.calibration() {
            tr!(
                "The drive couldn't calibrate its laser for this disc (OPC). The disc is most likely still blank. \
                 Try again, try a disc of another brand, or give a USB drive more power (both plugs of its Y-cable, \
                 or a port straight on the PC)."
            )
        } else if self.says("permission denied") || self.says("cannot open") {
            tr!("No permission to use the drive. Add yourself to the cdrom group and log in again.")
        } else if self.says("no disk") || self.says("medium not present") || self.says("cannot load media") {
            tr!("There's no disc in the drive (or its tray is open).")
        } else if self.says("not blank") || self.says("disk is not empty") || self.says("not writeable") {
            tr!("The disc isn't blank. Insert a blank CD or DVD.")
        } else if self.says("buffer underrun") || self.says("write error") || self.says("input/output error") {
            tr!("Writing failed partway; this disc is probably unusable now. Try a new one at a lower speed.")
        } else {
            ""
        };
        const NOISE: [&str; 9] =
            ["fifo", "burn-free", "writing  time", "writing time", "min fill", "average write speed", "cmd finished", "fixating time", "sense"];
        let detail: Vec<&str> = self
            .lines
            .iter()
            .map(String::as_str)
            .filter(|l| !NOISE.iter().any(|n| l.to_ascii_lowercase().contains(n)))
            .collect();
        let detail = detail[detail.len().saturating_sub(3)..].join("\n");
        let head = tr!("{program} failed ({status})", program = self.program, status = self.status);
        match hint {
            "" => format!("{head}:\n{detail}"),
            hint => format!("{hint}\n\n{head}:\n{detail}"),
        }
    }
}

fn run(program: &str, args: &[&str], on_line: &mut dyn FnMut(&str)) -> Result<(), Failure> {
    let fail = |status: String, lines: Vec<String>| Failure { program: program.to_owned(), status, lines };
    log::info!("burn: {program} {}", args.join(" "));
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| fail(format!("couldn't start: {e}"), Vec::new()))?;
    let (tx, rx) = mpsc::channel::<String>();
    let readers: Vec<_> = [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as _)]
        .into_iter()
        .flatten()
        .map(|mut stream| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut line = Vec::new();
                let mut buf = [0u8; 4096];
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    for &b in &buf[..n] {
                        if b == b'\n' || b == b'\r' {
                            if !line.is_empty() {
                                let _ = tx.send(String::from_utf8_lossy(&line).into_owned());
                                line.clear();
                            }
                        } else {
                            line.push(b);
                        }
                    }
                }
                if !line.is_empty() {
                    let _ = tx.send(String::from_utf8_lossy(&line).into_owned());
                }
            })
        })
        .collect();
    drop(tx);
    let mut tail: Vec<String> = Vec::new();
    for line in rx {
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }
        log::info!("burn: {line}");
        on_line(&line);
        tail.push(line);
        if tail.len() > 40 {
            tail.remove(0);
        }
    }
    for r in readers {
        let _ = r.join();
    }
    match child.wait() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(fail(status.to_string(), tail)),
        Err(e) => Err(fail(e.to_string(), tail)),
    }
}

fn parse_progress(line: &str) -> Option<Progress> {
    let lower = line.to_ascii_lowercase();
    let writing = |f: f32| Some(Progress { fraction: Some(f.clamp(0.0, 1.0)), status: tr!("Writing {percent} %", percent = format!("{:.0}", f * 100.0)) });
    if let Some(rest) = line.split_once("Writing:").map(|(_, r)| r) {
        let percent = rest.split_whitespace().find_map(|w| w.strip_suffix('%')?.parse::<f32>().ok());
        return percent.map_or(Some(Progress::stage(tr!("Writing"))), |p| writing(p / 100.0));
    }
    if let Some((before, _)) = line.split_once(" MB written") {
        let words: Vec<&str> = before.split_whitespace().collect();
        if let [.., done, "of", total] = words[..]
            && let (Ok(done), Ok(total)) = (done.parse::<f32>(), total.parse::<f32>())
            && total > 0.0
        {
            return writing(done / total);
        }
    }
    if let Some((before, _)) = line.split_once("% done") {
        return before.split_whitespace().last()?.parse::<f32>().ok().and_then(|p| writing(p / 100.0));
    }
    if lower.contains("blanking") || lower.contains("formatting") {
        return Some(Progress::stage(tr!("Erasing the disc")));
    }
    if lower.contains("fixating") || lower.contains("closing track") || lower.contains("closing session") || lower.contains("flushing cache") {
        return Some(Progress::stage(tr!("Finishing the disc")));
    }
    if lower.contains("eject") {
        return Some(Progress::stage(tr!("Ejecting")));
    }
    None
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{Burner, media_from_props, writes_from_props};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;

    pub fn burners() -> Vec<Burner> {
        let Ok(entries) = fs::read_dir("/sys/block") else { return Vec::new() };
        let mut found: Vec<Burner> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with("sr") {
                    return None;
                }
                let sys = entry.path();
                let dev = fs::read_to_string(sys.join("dev")).ok()?;
                let props = udev_props(dev.trim());
                let writes = writes_from_props(&props);
                if writes.is_empty() {
                    return None;
                }
                let read = |f: &str| fs::read_to_string(sys.join("device").join(f)).map(|s| s.trim().to_owned()).unwrap_or_default();
                let model = [read("vendor"), read("model")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
                Some(Burner {
                    device: PathBuf::from(format!("/dev/{name}")),
                    name: if model.is_empty() { tr!("Drive {name}", name = name) } else { model },
                    writes,
                    media: media_from_props(&props),
                })
            })
            .collect();
        found.sort_by(|a, b| a.device.cmp(&b.device));
        found
    }

    fn udev_props(dev: &str) -> HashMap<String, String> {
        fs::read_to_string(format!("/run/udev/data/b{dev}"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.strip_prefix("E:")?.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    pub fn burners() -> Vec<super::Burner> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn media_kinds() {
        assert_eq!(media_from_props(&props(&[("ID_CDROM", "1")])), Media::None);
        let blank_cd = props(&[("ID_CDROM_MEDIA", "1"), ("ID_CDROM_MEDIA_CD_R", "1"), ("ID_CDROM_MEDIA_STATE", "blank")]);
        assert_eq!(media_from_props(&blank_cd), Media::Blank("CD-R".into()));
        let dvd_rw = props(&[("ID_CDROM_MEDIA", "1"), ("ID_CDROM_MEDIA_DVD", "1"), ("ID_CDROM_MEDIA_DVD_PLUS_RW", "1"), ("ID_CDROM_MEDIA_STATE", "complete")]);
        assert_eq!(media_from_props(&dvd_rw), Media::Rewritable("DVD+RW".into()));
        let used = props(&[("ID_CDROM_MEDIA", "1"), ("ID_CDROM_MEDIA_DVD_R", "1"), ("ID_CDROM_MEDIA_DVD", "1"), ("ID_CDROM_MEDIA_STATE", "complete")]);
        assert_eq!(media_from_props(&used), Media::Used("DVD-R".into()));
        let pressed = props(&[("ID_CDROM_MEDIA", "1"), ("ID_CDROM_MEDIA_DVD", "1"), ("ID_CDROM_MEDIA_STATE", "complete")]);
        assert_eq!(media_from_props(&pressed), Media::ReadOnly("DVD-ROM".into()));
    }

    #[test]
    fn drive_capabilities() {
        let drive = props(&[("ID_CDROM", "1"), ("ID_CDROM_CD_R", "1"), ("ID_CDROM_DVD_PLUS_R", "1")]);
        assert_eq!(writes_from_props(&drive), ["CD", "DVD"]);
        assert!(writes_from_props(&props(&[("ID_CDROM", "1"), ("ID_CDROM_DVD", "1")])).is_empty());
    }

    #[test]
    fn explains_calibration_failure() {
        let lines = [
            "cmd finished after 62.242s timeout 60s",
            "wodim: OPC failed.",
            "Writing  time:   67.291s",
            "wodim: fifo had 6 puts and 0 gets.",
            "BURN-Free was never needed.",
        ];
        let f = Failure { program: "wodim".into(), status: "exit status: 255".into(), lines: lines.map(String::from).to_vec() };
        assert!(f.calibration());
        let text = f.explain();
        assert!(text.starts_with("The drive couldn't calibrate"), "{text}");
        assert!(text.ends_with("wodim failed (exit status: 255):\nwodim: OPC failed."), "{text}");
    }

    #[test]
    fn progress_lines() {
        let x = parse_progress("xorriso : UPDATE : Writing:    15376s   10.0%   fifo  22%  buf  50%   4.4xD").unwrap();
        assert_eq!(x.fraction, Some(0.1));
        let w = parse_progress("Track 01:    1 of    4 MB written (fifo 100%) [buf  99%]   4.0x.").unwrap();
        assert_eq!(w.fraction, Some(0.25));
        let g = parse_progress(" 45.00% done, estimate finish Sat Sep 27 22:30:00 2026").unwrap();
        assert_eq!(g.fraction, Some(0.45));
        assert_eq!(parse_progress("Fixating...").unwrap().status, "Finishing the disc");
        assert_eq!(parse_progress("Blanking PMA, TOC, pregap").unwrap().status, "Erasing the disc");
        assert!(parse_progress("Starting new track at sector: 0").is_none());
    }
}

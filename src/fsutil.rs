use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;

/// Replaces `path` with `contents` so that a crash or power cut leaves either the old file or the
/// new one, never a half-written one: the data is written and synced to a sibling file first, then
/// renamed over the target.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let name = path.file_name().context("no file name")?.to_string_lossy();
    let tmp = dir.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    let written = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_ref())?;
        file.sync_all()
    })();
    let result = written.and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_file_and_leaves_no_temporary_behind() {
        let dir = std::env::temp_dir().join(format!("yasaff-fsutil-{}", uuid::Uuid::new_v4()));
        let file = dir.join("a.json");
        write_atomic(&file, "one").unwrap();
        write_atomic(&file, "two").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "two");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}

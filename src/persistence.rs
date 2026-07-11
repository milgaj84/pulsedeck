//! Shared crash-resistant filesystem persistence helpers.
//!
//! User data is serialized before this module is called. Writes happen through a
//! uniquely named sibling file, are flushed to disk, and replace the destination
//! only after the temporary file is complete. Existing data is retained as a
//! `.bak` sibling so interrupted or malformed writes can be recovered.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Return the stable backup path for a persisted file.
pub fn backup_path(path: &Path) -> PathBuf {
    sibling_with_suffix(path, ".bak")
}

/// Preserve a damaged file beside the original and return the copy path.
pub fn preserve_corrupt_copy(path: &Path) -> io::Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sequence = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let suffix = format!(".corrupt-{stamp}-{sequence}");
    let copy_path = sibling_with_suffix(path, &suffix);
    fs::copy(path, &copy_path)?;
    Ok(copy_path)
}

/// Atomically replace `path`, retaining the previous valid contents as `.bak`.
///
/// The destination is never opened with truncation. If replacement fails after
/// the old file was moved aside, this function attempts to restore the backup.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let temp_path = create_unique_temp_path(path)?;
    let write_result = write_complete_file(&temp_path, bytes);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    let backup = backup_path(path);
    let had_existing_target = path.exists();

    if had_existing_target {
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        if let Err(error) = fs::rename(path, &backup) {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
    }

    if let Err(replace_error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        if had_existing_target && backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(io::Error::new(
            replace_error.kind(),
            format!("could not replace {}: {replace_error}", path.display()),
        ));
    }

    // Best effort on platforms that allow opening a directory for syncing.
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }

    Ok(())
}

fn write_complete_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

fn create_unique_temp_path(path: &Path) -> io::Result<PathBuf> {
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("persistence path has no file name: {}", path.display()),
        )
    })?;

    for _ in 0..32 {
        let sequence = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = OsString::from(file_name);
        temp_name.push(format!(".tmp-{}-{sequence}", std::process::id()));
        let candidate = path.with_file_name(temp_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("could not allocate temporary file beside {}", path.display()),
    ))
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("pulsedeck-data"));
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let sequence = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "pulsedeck-persistence-{name}-{}-{sequence}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn atomic_write_creates_new_file_without_backup() {
        let dir = TestDir::new("new-file");
        let target = dir.path().join("state.json");

        atomic_write(&target, br#"{"value":1}"#).expect("write new file");

        assert_eq!(fs::read(&target).unwrap(), br#"{"value":1}"#);
        assert!(!backup_path(&target).exists());
    }

    #[test]
    fn atomic_write_replaces_file_and_preserves_previous_contents() {
        let dir = TestDir::new("replace");
        let target = dir.path().join("library.json");
        fs::write(&target, b"old-valid-data").unwrap();

        atomic_write(&target, b"new-valid-data").expect("replace file");

        assert_eq!(fs::read(&target).unwrap(), b"new-valid-data");
        assert_eq!(fs::read(backup_path(&target)).unwrap(), b"old-valid-data");
    }

    #[test]
    fn repeated_replacement_keeps_last_known_good_backup() {
        let dir = TestDir::new("repeat");
        let target = dir.path().join("history.json");

        atomic_write(&target, b"one").unwrap();
        atomic_write(&target, b"two").unwrap();
        atomic_write(&target, b"three").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"three");
        assert_eq!(fs::read(backup_path(&target)).unwrap(), b"two");
    }

    #[test]
    fn preserve_corrupt_copy_does_not_modify_source() {
        let dir = TestDir::new("corrupt-copy");
        let target = dir.path().join("library.json");
        fs::write(&target, b"{broken").unwrap();

        let copy = preserve_corrupt_copy(&target).expect("preserve damaged file");

        assert_eq!(fs::read(&target).unwrap(), b"{broken");
        assert_eq!(fs::read(copy).unwrap(), b"{broken");
    }
}
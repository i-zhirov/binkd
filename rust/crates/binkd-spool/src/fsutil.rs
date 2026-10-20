//! The file system helpers of `tools.c` and `unix/rename.c`.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Creates the directories of a file path.
///
/// The port of `mkpath()`: the last component is not created.
pub fn mkpath(path: &str) {
    if let Some(parent) = Path::new(path).parent() {
        let _ = fs::create_dir_all(parent);
    }
}

/// Creates an empty file with the process id.
///
/// The port of `create_sem_file()`. Returns false when the file exists.
pub fn create_sem_file(path: &str) -> bool {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut f) => {
            let _ = writeln!(f, "{}", std::process::id());
            true
        }
        Err(_) => false,
    }
}

/// Sets the modification time of a file.
///
/// The port of `touch()`.
pub fn touch(path: &str, time: i64) -> bool {
    let Ok(f) = fs::OpenOptions::new().write(true).open(path) else {
        return false;
    };
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(time.max(0) as u64);
    f.set_modified(time).is_ok()
}

/// Renames a file and fails when the target exists.
///
/// The port of `o_rename()` of `unix/rename.c`. The function links the
/// file and removes the source.
pub fn rename_noclobber(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::hard_link(from, to) {
        Ok(()) => {
            let _ = fs::remove_file(from);
            Ok(())
        }
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported
            ) =>
        {
            // A file system without hard links.
            match fs::OpenOptions::new().write(true).create_new(true).open(to) {
                Ok(_) => {}
                Err(e) => return Err(e),
            }
            match fs::rename(from, to) {
                Ok(()) => Ok(()),
                Err(e) => {
                    let _ = fs::remove_file(to);
                    Err(e)
                }
            }
        }
        Err(e) => Err(e),
    }
}

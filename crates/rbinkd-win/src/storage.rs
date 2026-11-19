//! The safe file-system helpers of the Windows platform, the port of the
//! `GetShortPathName()` call of `run_args()` of `srif.c` and of the volume
//! identity of `stat()` that `check_config()` of `readcfg.c` uses.
//!
//! The unsafe Windows calls stay in this crate, the exception of the lint
//! `unsafe_code` of `doc/development.md`. The functions answer a
//! value and never end the process.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, GetShortPathNameW, BY_HANDLE_FILE_INFORMATION,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

/// Returns the short path of a path, the port of the `GetShortPathName()` call
/// of `run_args()` of `srif.c`.
///
/// The C code passes the path of the received file and answers the short name
/// of the Win32 API. The port grows a UTF-16 buffer until the call fits, and it
/// answers the error code of `GetLastError()` when the path does not exist or
/// the call fails.
pub fn short_path(path: &Path) -> Result<String, u32> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buffer = vec![0u16; 260];
    loop {
        let length =
            unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
        if length == 0 {
            return Err(unsafe { GetLastError() });
        }
        let length = length as usize;
        if length < buffer.len() {
            let short = OsString::from_wide(&buffer[..length]);
            return Ok(short.to_string_lossy().into_owned());
        }
        // A small buffer answers the size that the path needs, the null
        // terminator included.
        buffer.resize(length, 0);
    }
}

/// Returns the volume serial number of a path, the volume identity that the C
/// Windows `stat()` answers in `st_dev`.
///
/// The C code uses the identity of the file system to check that `temp-inbound`,
/// `inbound`, `inbound-nonsecure`, and a node `ibox` are on the same partition.
/// The port opens the path with `FILE_FLAG_BACKUP_SEMANTICS`, so a directory is
/// accepted, and reads `dwVolumeSerialNumber` with `GetFileInformationByHandle`.
/// The handle closes on every path. A failed open or call answers `None`.
pub fn volume_serial(path: &Path) -> Option<u32> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .ok()?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    let result = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut information) };
    if result == 0 {
        None
    } else {
        Some(information.dwVolumeSerialNumber)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rbinkd-win-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_short_path_of_a_missing_path_fails() {
        let dir = temp_dir("short-missing");
        let path = dir.join("missing").join("file.txt");
        assert!(
            short_path(&path).is_err(),
            "a missing path answered a short path"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_short_path_of_a_temporary_file_is_a_path() {
        let dir = temp_dir("short-file");
        let path = dir.join("binkd-short-path-file.txt");
        fs::write(&path, b"test").unwrap();
        let short = short_path(&path).expect("a temporary file has no short path");
        assert!(!short.is_empty(), "the short path is empty");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_volume_serial_of_a_temporary_directory() {
        let first = temp_dir("volume-first");
        let second = temp_dir("volume-second");
        let serial = volume_serial(&first).expect("a temporary directory has no volume");
        assert_eq!(
            Some(serial),
            volume_serial(&second),
            "two directories on one volume answered different serials"
        );
        let _ = fs::remove_dir_all(&first);
        let _ = fs::remove_dir_all(&second);
    }

    #[test]
    fn the_volume_serial_of_a_missing_path() {
        let dir = temp_dir("volume-missing");
        assert_eq!(volume_serial(&dir.join("missing")), None);
        let _ = fs::remove_dir_all(&dir);
    }
}

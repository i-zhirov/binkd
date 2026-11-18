//! The `.try`, `.hld`, `.bsy`, and poll files, the port of `ftnq.c`.

use std::fs;
use std::path::{Path, PathBuf};

use binkd_ftn::addr::FtnAddr;
use binkd_ftn::text::safe_atol;

use crate::config::SpoolConfig;
use crate::queue::FLO_FLAVOURS;

/// Returns the `.try` path of an address, or `None` for an unknown domain.
pub fn try_path(fa: &FtnAddr, config: &SpoolConfig) -> Option<PathBuf> {
    let filename = fa.to_filename(&config.domains);
    if filename.is_empty() {
        return None;
    }
    Some(PathBuf::from(format!("{filename}.try")))
}

/// Returns the `.hld` path of an address, or `None` for an unknown domain.
pub fn hld_path(fa: &FtnAddr, config: &SpoolConfig) -> Option<PathBuf> {
    let filename = fa.to_filename(&config.domains);
    if filename.is_empty() {
        return None;
    }
    Some(PathBuf::from(format!("{filename}.hld")))
}

/// Writes the `.try` file of a node.
///
/// The port of `write_try()`. The file holds the number of good and bad
/// attempts and the comment.
pub fn write_try(fa: &FtnAddr, nok: u32, nbad: u32, comment: &str, config: &SpoolConfig) {
    if config.tries == 0 {
        return;
    }
    let Some(path) = try_path(fa, config) else {
        return;
    };
    let mut data = Vec::with_capacity(5 + comment.len());
    data.push((nok & 0xff) as u8);
    data.push(((nok >> 8) & 0xff) as u8);
    data.push((nbad & 0xff) as u8);
    data.push(((nbad >> 8) & 0xff) as u8);
    data.push(comment.len() as u8);
    data.extend_from_slice(comment.as_bytes());
    let _ = fs::write(path, data);
}

/// Reads the `.try` file of a node.
///
/// The port of `read_try()`. Returns `None` when the file name is unknown;
/// the C code keeps the values of the caller in that case.
pub fn read_try(fa: &FtnAddr, config: &SpoolConfig) -> Option<(u32, u32)> {
    let path = try_path(fa, config)?;
    match fs::read(&path) {
        Ok(data) if data.len() >= 4 => Some((
            data[0] as u32 + ((data[1] as u32) << 8),
            data[2] as u32 + ((data[3] as u32) << 8),
        )),
        _ => Some((0, 0)),
    }
}

/// Counts a bad attempt and holds the node when the limit is reached.
///
/// The port of `bad_try()`. `now` replaces `safe_time()`.
pub fn bad_try(fa: &FtnAddr, error: &str, config: &mut SpoolConfig, now: i64) {
    if config.tries == 0 {
        return;
    }
    let (mut nok, mut nbad) = read_try(fa, config).unwrap_or((0, 0));
    nbad += 1;
    if nbad >= config.tries {
        nok = 0;
        nbad = 0;
        hold_node(fa, now + config.hold, config, now);
    }
    write_try(fa, nok, nbad, error, config);
}

/// Counts a good session.
///
/// The port of `good_try()`.
pub fn good_try(fa: &FtnAddr, comment: &str, config: &mut SpoolConfig) {
    if config.tries == 0 {
        return;
    }
    if !config.remove_try_files {
        let (mut nok, _nbad) = read_try(fa, config).unwrap_or((0, 0));
        nok += 1;
        write_try(fa, nok, 0, comment, config);
    } else {
        remove_try(fa, config);
    }
}

/// Removes the `.try` file of a node.
///
/// The port of `remove_try()`.
pub fn remove_try(fa: &FtnAddr, config: &SpoolConfig) {
    if config.tries == 0 {
        return;
    }
    if let Some(path) = try_path(fa, config) {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
}

/// Writes the `.hld` file of a node and sets its hold time.
///
/// The port of `hold_node()`.
pub fn hold_node(fa: &FtnAddr, hold_until: i64, config: &mut SpoolConfig, now: i64) {
    let Some(path) = hld_path(fa, config) else {
        return;
    };
    if fs::write(&path, hold_until.to_string()).is_ok() {
        if let Some(node) = config.nodes.get_mut(fa, now) {
            node.hold_until = hold_until;
        }
    }
}

/// Reads the `.hld` file of a node.
///
/// The port of `process_hld()`. A time in the past clears the hold and
/// removes the file. `now` replaces `safe_time()`.
pub fn process_hld(fa: &FtnAddr, path: &Path, config: &mut SpoolConfig, now: i64) {
    let Some(node) = config.nodes.get_mut(fa, now) else {
        return;
    };

    node.hold_until = match fs::read_to_string(path) {
        Ok(text) => safe_atol(Some(text.trim_start())).unwrap_or(0),
        Err(_) => 0,
    };

    if node.hold_until <= now {
        node.hold_until = 0;
        let _ = fs::remove_file(path);
    }
}

/// Removes an old `.bsy` file or sets the busy state of a node.
///
/// The port of `process_bsy()`. `now` replaces `time(0)`.
pub fn process_bsy(fa: &FtnAddr, path: &Path, config: &mut SpoolConfig, now: i64) {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !ext.eq_ignore_ascii_case("bsy") && !ext.eq_ignore_ascii_case("csy") {
        return;
    }

    if let Ok(meta) = fs::metadata(path) {
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if config.kill_old_bsy != 0 && now - mtime > config.kill_old_bsy {
            let _ = fs::remove_file(path);
            return;
        }
    }

    if let Some(node) = config.nodes.get_mut(fa, now) {
        // The C code takes `s[1]` of the last four characters, so the busy
        // state is `c` for `.csy` and `b` for `.bsy`.
        if node.busy != b'b' {
            node.busy = ext.as_bytes().first().copied().unwrap_or(b'b');
        }
    }
}

/// Creates an empty `.?lo` poll file for a node.
///
/// The port of `create_poll()`. Returns true when the file exists or is
/// created.
pub fn create_poll(fa: &FtnAddr, flvr: u8, config: &SpoolConfig) -> bool {
    let mut ext = ".flo".to_string();
    if flvr != 0 && FLO_FLAVOURS.contains(&flvr) {
        ext.replace_range(1..2, &(flvr.to_ascii_lowercase() as char).to_string());
    }
    let filename = fa.to_filename(&config.domains);
    if filename.is_empty() {
        return false;
    }
    let path = PathBuf::from(format!("{filename}{ext}"));
    if path.exists() {
        return true;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use binkd_ftn::domain::{DomainTable, FtnDomain};

    fn config() -> SpoolConfig {
        let mut config = SpoolConfig::new();
        config.domains = DomainTable::new(vec![FtnDomain {
            name: "fidonet".to_string(),
            path: "/tmp/fido".to_string(),
            dir: "outb".to_string(),
            z: [2, -1],
            idomain: None,
            alias4: None,
        }]);
        config
    }

    #[test]
    fn try_round_trip() {
        let dir = std::env::temp_dir().join("binkd-try-round-trip");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("outb")).unwrap();
        let mut config = config();
        config.domains = DomainTable::new(vec![FtnDomain {
            name: "fidonet".to_string(),
            path: dir.display().to_string(),
            dir: "outb".to_string(),
            z: [2, -1],
            idomain: None,
            alias4: None,
        }]);
        config.tries = 3;

        let fa = FtnAddr::parse("2:5047/100@fidonet", &config.domains).unwrap();
        write_try(&fa, 2, 1, "test", &config);
        assert_eq!(read_try(&fa, &config), Some((2, 1)));
        let _ = fs::remove_dir_all(&dir);
    }
}

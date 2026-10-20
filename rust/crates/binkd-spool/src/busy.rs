//! The busy flags (`.bsy` and `.csy`), the port of `bsy.c`.
//!
//! The C code keeps the list in the `bsy_list` global and warns that the
//! module behaves in very different ways in the fork and the thread builds.
//! The port keeps the list in a value that the daemon owns.

use std::fs;
use std::path::Path;

use binkd_ftn::addr::FtnAddr;
use binkd_ftn::text::last_slash;

use crate::config::SpoolConfig;
use crate::fsutil;

/// The busy flag of a session.
pub const F_BSY: u8 = b'b';
/// The busy flag of a call.
pub const F_CSY: u8 = b'c';
/// Touch the flags again after this many seconds.
pub const BSY_TOUCH_DELAY: i64 = 60;

/// One busy flag.
#[derive(Clone, Debug)]
pub struct BusyEntry {
    /// The address, or `None` for a free cell.
    pub fa: Option<FtnAddr>,
    /// The kind of the flag.
    pub bt: u8,
}

/// The list of our busy flags.
#[derive(Clone, Debug, Default)]
pub struct BusyList {
    cells: Vec<BusyEntry>,
    /// The time of the last touch.
    pub last_touch: i64,
}

impl BusyList {
    /// Creates an empty list.
    pub fn new() -> BusyList {
        BusyList::default()
    }

    /// Returns the entries of the list.
    pub fn entries(&self) -> &[BusyEntry] {
        &self.cells
    }

    /// Returns the index of a free cell.
    fn free_cell(&mut self) -> usize {
        for (i, cell) in self.cells.iter().enumerate() {
            if cell.fa.is_none() {
                return i;
            }
        }
        self.cells.push(BusyEntry {
            fa: None,
            bt: F_BSY,
        });
        self.cells.len() - 1
    }

    /// Creates the busy file of the address.
    ///
    /// The port of `bsy_add()`. Returns false when the file exists.
    pub fn add(&mut self, fa: &FtnAddr, bt: u8, config: &SpoolConfig) -> bool {
        let filename = fa.to_filename(&config.domains);
        if filename.is_empty() {
            return false;
        }
        let path = format!("{filename}{}", flag_ext(bt));
        fsutil::mkpath(&path);
        if !fsutil::create_sem_file(&path) {
            return false;
        }
        let index = self.free_cell();
        self.cells[index].fa = Some(fa.clone());
        self.cells[index].bt = bt;
        true
    }

    /// Tests the busy file of the address.
    ///
    /// The port of `bsy_test()`. Returns true when the flag is free.
    pub fn test(&self, fa: &FtnAddr, bt: u8, config: &SpoolConfig) -> bool {
        let filename = fa.to_filename(&config.domains);
        if filename.is_empty() {
            return false;
        }
        let path = format!("{filename}{}", flag_ext(bt));
        fsutil::mkpath(&path);
        !Path::new(&path).exists()
    }

    /// Removes the busy file of the address.
    ///
    /// The port of `bsy_remove()`.
    pub fn remove(&mut self, fa: &FtnAddr, bt: u8, config: &SpoolConfig) {
        let filename = fa.to_filename(&config.domains);
        if filename.is_empty() {
            return;
        }
        let path = format!("{filename}{}", flag_ext(bt));

        let Some(index) = self.cells.iter().position(|cell| {
            cell.fa
                .as_ref()
                .is_some_and(|cell_fa| cell_fa.compare(fa) == 0)
                && cell.bt == bt
        }) else {
            return;
        };

        let _ = fs::remove_file(&path);

        if config.deletedirs {
            let mut dir = Path::new(&path);
            // Remove an empty point directory.
            if fa.p != 0 {
                if let Some(parent) = dir.parent() {
                    let _ = fs::remove_dir(parent);
                    dir = parent;
                }
            }
            // Remove an empty zone directory.
            let domain = config.domains.get(&fa.domain);
            if let Some(domain) = domain {
                if fa.z != domain.z[0] {
                    if let Some(parent) = dir.parent() {
                        let _ = fs::remove_dir(parent);
                    }
                }
            }
        }

        self.cells[index].fa = None;
    }

    /// Removes every busy file. The port of `bsy_remove_all()`.
    pub fn remove_all(&mut self, config: &SpoolConfig) {
        for cell in &mut self.cells {
            let Some(fa) = cell.fa.clone() else {
                continue;
            };
            let filename = fa.to_filename(&config.domains);
            if filename.is_empty() {
                continue;
            }
            let path = format!("{filename}{}", flag_ext(cell.bt));
            let _ = fs::remove_file(&path);

            // Remove an empty point directory.
            if config.deletedirs && fa.p != 0 {
                if let Some(parent) = Path::new(&path).parent() {
                    let _ = fs::remove_dir(parent);
                }
            }
            cell.fa = None;
        }
        self.last_touch = 0;
    }

    /// Touches the busy files when the delay passed.
    ///
    /// The port of `bsy_touch()`. `now` replaces `time(0)`.
    pub fn touch(&mut self, config: &SpoolConfig, now: i64) {
        if now - self.last_touch <= BSY_TOUCH_DELAY {
            return;
        }
        for cell in &self.cells {
            let Some(fa) = &cell.fa else {
                continue;
            };
            let filename = fa.to_filename(&config.domains);
            if filename.is_empty() {
                continue;
            }
            let path = format!("{filename}{}", flag_ext(cell.bt));
            fsutil::touch(&path, now);
        }
        self.last_touch = now;
    }
}

/// Returns the extension of a busy flag.
pub fn flag_ext(bt: u8) -> &'static str {
    if bt == F_CSY {
        ".csy"
    } else {
        ".bsy"
    }
}

/// Returns the last directory separator of the path.
#[allow(dead_code)]
fn _last_slash(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    last_slash(bytes).map(|s| std::str::from_utf8(s).unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use binkd_ftn::domain::{DomainTable, FtnDomain};

    fn config(dir: &Path) -> SpoolConfig {
        let mut config = SpoolConfig::new();
        config.domains = DomainTable::new(vec![FtnDomain {
            name: "fidonet".to_string(),
            path: dir.display().to_string(),
            dir: "outb".to_string(),
            z: [2, -1],
            idomain: None,
            alias4: None,
        }]);
        config
    }

    #[test]
    fn add_and_remove() {
        let dir = std::env::temp_dir().join("binkd-busy-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("outb")).unwrap();
        let config = config(&dir);
        let fa = FtnAddr::parse("2:5047/100@fidonet", &config.domains).unwrap();

        let mut list = BusyList::new();
        assert!(list.add(&fa, F_BSY, &config));
        assert!(!list.add(&fa, F_BSY, &config));
        assert!(!list.test(&fa, F_BSY, &config));
        assert!(list.test(&fa, F_CSY, &config));
        list.remove(&fa, F_BSY, &config);
        assert!(list.test(&fa, F_BSY, &config));
        let _ = fs::remove_dir_all(&dir);
    }
}

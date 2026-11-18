//! The link status files of the ND mode, the port of the `ND_set_status()` part
//! of `protocol.c`.
//!
//! The C code keeps the status of a link in `<outbound>/<address>.stc`. The file
//! holds the name, the size, and the time of the file that the peer received in
//! the last session. The outbound scanner reads the file as the queue type `s`
//! (`binkd-spool::queue`), so a sender offers the file again at the next
//! session, and an empty status removes the file.

use std::path::PathBuf;

use binkd_ftn::addr::FtnAddr;

use binkd_ftn::domain::DomainTable;

/// Returns the path of the status file of an address, the port of
/// `ftnaddress_to_filename()` with the `.stc` extension of `ND_set_status()`.
///
/// `None` means that the domain of the address is unknown.
pub fn status_path(domains: &DomainTable, fa: &FtnAddr) -> Option<PathBuf> {
    let name = fa.to_filename(domains);
    if name.is_empty() {
        return None;
    }
    Some(PathBuf::from(format!("{name}.stc")))
}

/// Sets the status of a link, the port of `ND_set_status()`.
///
/// The status holds the name, the size, and the time of the file that the peer
/// received, in the form that the queue reads. An empty status removes the file.
/// An address without a domain does nothing, as in the C code.
pub fn set_status(domains: &DomainTable, fa: &FtnAddr, status: &str) -> std::io::Result<()> {
    let Some(path) = status_path(domains, fa) else {
        return Ok(());
    };
    if status.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    } else {
        std::fs::write(&path, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SpoolConfig;
    use binkd_ftn::domain::{DomainTable, FtnDomain};
    use std::path::Path;

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
    fn path() {
        let dir = std::env::temp_dir().join("binkd-status-path");
        let config = config(&dir);
        let fa = FtnAddr::parse("2:5047/100@fidonet", &config.domains).unwrap();
        let path = status_path(&config.domains, &fa).unwrap();
        assert!(path.to_string_lossy().ends_with("13b70064.stc"));
        // An address of an unknown domain holds no status file.
        let other = FtnAddr::parse("1:1/1@other", &config.domains).unwrap();
        assert!(status_path(&config.domains, &other).is_none());
    }

    #[test]
    fn set_and_read() {
        let dir = std::env::temp_dir().join("binkd-status-set");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("outb")).unwrap();
        let mut config = config(&dir);
        let fa = FtnAddr::parse("2:5047/100@fidonet", &config.domains).unwrap();
        // The scanner of the C code adds a file only for a known node.
        config
            .nodes
            .add(&fa, &binkd_ftn::node::NodeSpec::default(), 0);

        set_status(&config.domains, &fa, "13b70064.pkt 300 1000000000").unwrap();
        // The outbound scanner reads the file as the queue type `s`, and the
        // scan of the client manager gives its flavour to the node.
        let queue = crate::queue::Queue::scan(&mut config, 0);
        let entry = queue
            .entries
            .iter()
            .find(|e| e.kind == crate::queue::EntryKind::Status);
        let entry = entry.expect("no status entry");
        assert_eq!(entry.path.to_string_lossy(), "13b70064.pkt");
        assert_eq!(entry.size, 300);
        assert_eq!(entry.time, 1_000_000_000);
        crate::queue::Queue::scan_listed(&mut config, 0);
        let node = config.nodes.get(&fa, 0).unwrap();
        assert_eq!(node.files_flvr, b'h' as i32);

        // An empty status removes the file.
        set_status(&config.domains, &fa, "").unwrap();
        assert!(!status_path(&config.domains, &fa).unwrap().is_file());
        // A second removal is not an error.
        set_status(&config.domains, &fa, "").unwrap();
    }
}

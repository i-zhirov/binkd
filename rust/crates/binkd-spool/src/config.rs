//! The part of the config that the spool layer needs.
//!
//! The C code reads the whole `BINKD_CONFIG` in `readcfg.c`. The spool layer
//! uses a slice of it, so the parser of phase 3 fills this value.

use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::DomainTable;

use binkd_ftn::node::NodeTable;

/// A shared AKA chain of the `share` directive.
#[derive(Clone, Debug)]
pub struct Share {
    /// The main address of the chain.
    pub sha: FtnAddr,
    /// The addresses that receive the mail of the main address.
    pub addrs: Vec<FtnAddr>,
}

/// The config values of the spool layer.
#[derive(Clone, Debug, Default)]
pub struct SpoolConfig {
    /// The domain table.
    pub domains: DomainTable,
    /// The node table.
    pub nodes: NodeTable,
    /// Our addresses.
    pub aka: Vec<FtnAddr>,
    /// The `share` chains.
    pub shares: Vec<Share>,
    /// `send-if-pwd`: send mail only to nodes with a password.
    pub send_if_pwd: bool,
    /// A `defnode` entry is defined.
    pub havedefnode: bool,
    /// The `try` directive: the number of attempts.
    pub tries: u32,
    /// The `hold` directive in seconds.
    pub hold: i64,
    /// `remove-try-files`: remove the `.try` file after a good session.
    pub remove_try_files: bool,
    /// `kill-old-bsy`: remove a `.bsy` file older than this many seconds.
    pub kill_old_bsy: i64,
    /// `deletebox`: remove an empty filebox.
    pub deleteablebox: bool,
    /// The `filebox` directory of the T-Mail style.
    pub tfilebox: String,
    /// The `brakebox` directory of the Brake! style.
    pub bfilebox: String,
}

impl SpoolConfig {
    /// Creates a config with the defaults of `lock_config_structure()`.
    pub fn new() -> SpoolConfig {
        SpoolConfig::default()
    }

    /// Adds a node to the table.
    ///
    /// The port of `add_node()`.
    pub fn add_node(&mut self, fa: &FtnAddr, spec: &binkd_ftn::node::NodeSpec, now: i64) -> usize {
        self.nodes.add(fa, spec, now)
    }

    /// Returns the node by address.
    ///
    /// The port of `get_node_info()`.
    pub fn node(&mut self, fa: &FtnAddr, now: i64) -> Option<&binkd_ftn::node::FtnNode> {
        self.nodes.get(fa, now)
    }

    /// Returns the node by address for change.
    pub fn node_mut(&mut self, fa: &FtnAddr, now: i64) -> Option<&mut binkd_ftn::node::FtnNode> {
        self.nodes.get_mut(fa, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let config = SpoolConfig::new();
        assert_eq!(config.tries, 0);
        assert_eq!(config.hold, 0);
        assert!(!config.send_if_pwd);
    }
}

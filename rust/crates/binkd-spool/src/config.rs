//! The part of the config that the spool layer needs.
//!
//! The C code reads the whole `BINKD_CONFIG` in `readcfg.c`. The spool layer
//! uses a slice of it, so the parser of phase 3 fills this value.

use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::DomainTable;
use binkd_ftn::pattern::pmatch_ncase;
pub use binkd_ftn::text::InbCase;

use binkd_ftn::node::NodeTable;

/// The rename style of an inbound file that exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RenameStyle {
    /// Append a number after the name: `file.ext.1`.
    #[default]
    Postfix,
    /// Change the extension: `file.ex0`.
    Extension,
    /// Change the name, not the extension.
    Body,
}

/// The address type of the `check-pkthdr` directive.
pub mod addrtype {
    /// Every address.
    pub const A_ALL: i32 = -1;
    /// A listed address.
    pub const A_LST: i32 = 1;
    /// An unlisted address.
    pub const A_UNLST: i32 = 2;
    /// A secure session.
    pub const A_PROT: i32 = 4;
    /// A non-secure session.
    pub const A_UNPROT: i32 = 8;
}

/// Returns the mask that matches the name, or `None`.
///
/// The port of `mask_test()` of `readcfg.c`.
pub fn mask_test<'a>(s: &str, masks: &'a [String]) -> Option<&'a str> {
    masks
        .iter()
        .find(|m| pmatch_ncase(m.as_bytes(), s.as_bytes()))
        .map(|m| m.as_str())
}

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
    /// `deletedirs`: remove an empty point and zone directory.
    pub deletedirs: bool,
    /// The inbound directory of a non-secure session.
    pub inbound_nonsecure: String,
    /// `temp-inbound`: the directory of the partial files.
    pub temp_inbound: String,
    /// `minfree`: the free space of a secure session, in kilobytes.
    pub minfree: i64,
    /// `minfree-nonsecure`: the free space of a non-secure session.
    pub minfree_nonsecure: i64,
    /// `kill-dup-partial-files`: remove a duplicate partial file.
    pub kill_dup_partial_files: bool,
    /// `kill-old-partial-files`: remove a partial file older than this.
    pub kill_old_partial_files: i64,
    /// `rename-style`: the style of a name that exists.
    pub renamestyle: RenameStyle,
    /// `inboundcase`: the case style of an inbound name.
    pub inboundcase: InbCase,
    /// `check-pkthdr`: the address type to check.
    pub pkthdr_type: i32,
    /// `check-pkthdr`: the extension of a packet with a bad address.
    pub pkthdr_bad: Option<String>,
    /// `overwrite`: the masks of the names to overwrite.
    pub overwrite: Vec<String>,
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

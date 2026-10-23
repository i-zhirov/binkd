//! The node table, the port of `ftnnode.c`.
//!
//! The C code resolves an unknown node from the `defnode` entry with DNS
//! (`get_defnode_info()`). That part needs the network and lives in the network
//! layer; the table here keeps the data.

use crate::addr::FtnAddr;

/// The node is unlisted: binkd added it at run time.
pub const NL_UNLISTED: i32 = 0;
/// The node is listed in the config.
pub const NL_NODE: i32 = 1;
/// The node is listed in the passwords file.
pub const NL_PASSWORDS: i32 = 2;

/// `NR_USE_OLD`, `ND_USE_OLD`, `RIP_USE_OLD`, `NP_USE_OLD`, `AF_USE_OLD`:
/// keep the old value of the flag.
pub const USE_OLD: i32 = -1;

/// `AF_UNSPEC`: the address family of the system is not fixed.
pub const AF_UNSPEC: i32 = 0;

/// `NR_ON`.
pub const NR_ON: i32 = 1;
/// `NR_OFF`.
pub const NR_OFF: i32 = 0;
/// `ND_ON`.
pub const ND_ON: i32 = 1;
/// `ND_OFF`.
pub const ND_OFF: i32 = 0;
/// `MD_ON`.
pub const MD_ON: i32 = 1;
/// `MD_OFF`.
pub const MD_OFF: i32 = -1;
/// `MD_USE_OLD`, the default value.
pub const MD_USE_OLD: i32 = 0;
/// `RIP_ON`.
pub const RIP_ON: i32 = 1;
/// `RIP_OFF`.
pub const RIP_OFF: i32 = 0;
/// `RIP_SIP`: the strict IP check of `-sip`.
pub const RIP_SIP: i32 = 2;
/// `HC_ON`.
pub const HC_ON: i32 = 1;
/// `HC_OFF`.
pub const HC_OFF: i32 = -1;
/// `HC_USE_OLD`, the default value.
pub const HC_USE_OLD: i32 = 0;
/// `NP_ON`.
pub const NP_ON: i32 = 1;
/// `NP_OFF`.
pub const NP_OFF: i32 = 0;

/// The node resolution is repeated after this many seconds.
pub const RESOLVE_TTL: i64 = 3600;

/// The value of a password of a node.
///
/// The C code keeps a pointer to the main password for a node without a
/// separate packet or outbound password. The enum states that case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodePassword {
    /// The node has no password of this kind.
    None,
    /// The node uses its main password.
    Main,
    /// The node has a separate password.
    Value(String),
}

/// A link of the node table.
#[derive(Clone, Debug)]
pub struct FtnNode {
    /// The node is listed in the config or added at run time.
    pub listed: i32,
    /// The host list: `host1:port1,host2:port2,*`.
    pub hosts: Option<String>,
    /// The FTN address of the node.
    pub fa: FtnAddr,
    /// The main password. `-` means no password.
    pub pwd: String,
    /// The packet password.
    pub pkt_pwd: NodePassword,
    /// The outbound password.
    pub out_pwd: NodePassword,
    /// The flavour of the outbound filebox.
    pub obox_flvr: u8,
    /// The outbound filebox of the node.
    pub obox: Option<String>,
    /// The inbound filebox of the node.
    pub ibox: Option<String>,
    /// The command to start instead of a socket.
    pub pipe: Option<String>,
    pub nr_flag: i32,
    pub nd_flag: i32,
    pub md_flag: i32,
    pub hc_flag: i32,
    /// 0 is off, 1 is `-ip`, 2 is `-sip`.
    pub restrict_ip: i32,
    pub np_flag: i32,
    /// The node is held until this time.
    pub hold_until: i64,
    /// 0 is free, `c` is a call, `b` is a session.
    pub busy: u8,
    /// 0 is none, -1 is no mail, other is a flavour.
    pub mail_flvr: i32,
    /// 0 is none, -1 is no files, other is a flavour.
    pub files_flvr: i32,
    pub ip_afamily: i32,
    /// The time of the next DNS resolution.
    pub recheck: i64,
}

impl FtnNode {
    /// Reports whether the node asks for the NR mode, the port of
    /// `to->NR_flag == NR_ON` of `protocol.c`.
    pub fn wants_nr(&self) -> bool {
        self.nr_flag == NR_ON
    }

    /// Reports whether the node asks for the ND mode, the port of
    /// `to->ND_flag == ND_ON` of `protocol.c`.
    pub fn wants_nd(&self) -> bool {
        self.nd_flag == ND_ON
    }

    /// Creates a node with the defaults of `add_node_nolock()`.
    pub fn new(fa: &FtnAddr) -> FtnNode {
        FtnNode {
            listed: NL_UNLISTED,
            hosts: None,
            fa: fa.clone(),
            pwd: "-".to_string(),
            pkt_pwd: NodePassword::None,
            out_pwd: NodePassword::None,
            obox_flvr: b'f',
            obox: None,
            ibox: None,
            pipe: None,
            nr_flag: 0,
            nd_flag: 0,
            md_flag: 0,
            hc_flag: 0,
            restrict_ip: 0,
            np_flag: 0,
            hold_until: 0,
            busy: 0,
            mail_flvr: 0,
            files_flvr: 0,
            ip_afamily: 0,
            recheck: 0,
        }
    }
}

/// The settings of `add_node()` for a node.
///
/// `None` means "keep the old value", the `USE_OLD` sentinel of the C code.
#[derive(Clone, Debug, Default)]
pub struct NodeSpec {
    pub hosts: Option<String>,
    pub pwd: Option<String>,
    pub pkt_pwd: Option<String>,
    pub out_pwd: Option<String>,
    pub obox_flvr: Option<u8>,
    pub obox: Option<String>,
    pub ibox: Option<String>,
    pub nr_flag: Option<i32>,
    pub nd_flag: Option<i32>,
    pub md_flag: Option<i32>,
    pub restrict_ip: Option<i32>,
    pub hc_flag: Option<i32>,
    pub np_flag: Option<i32>,
    pub pipe: Option<String>,
    pub ip_afamily: Option<i32>,
}

/// The node table.
#[derive(Clone, Debug, Default)]
pub struct NodeTable {
    nodes: Vec<FtnNode>,
    sorted: bool,
}

impl NodeTable {
    /// Creates an empty table.
    pub fn new() -> NodeTable {
        NodeTable::default()
    }

    /// Returns the nodes.
    pub fn nodes(&self) -> &[FtnNode] {
        &self.nodes
    }

    /// Returns the nodes for change.
    pub fn nodes_mut(&mut self) -> &mut [FtnNode] {
        &mut self.nodes
    }

    /// Sorts the nodes by address. The C code sorts on the first search.
    pub fn sort(&mut self) {
        self.nodes.sort_by(|a, b| a.fa.compare(&b.fa).cmp(&0));
        self.sorted = true;
    }

    /// Adds a node or edits the settings of a known node.
    ///
    /// The port of `add_node_nolock()`. `now` replaces `safe_time()`.
    pub fn add(&mut self, fa: &FtnAddr, spec: &NodeSpec, now: i64) -> usize {
        let index = match self.nodes.iter().position(|n| n.fa.compare(fa) == 0) {
            Some(i) => i,
            None => {
                let node = FtnNode::new(fa);
                self.nodes.push(node);
                self.sorted = false;
                self.nodes.len() - 1
            }
        };

        let node = &mut self.nodes[index];
        node.recheck = now.wrapping_add(RESOLVE_TTL);

        if let Some(v) = spec.md_flag {
            if v != USE_OLD {
                node.md_flag = v;
            }
        }
        if let Some(v) = spec.restrict_ip {
            if v != USE_OLD {
                node.restrict_ip = v;
            }
        }
        if let Some(v) = spec.nr_flag {
            if v != USE_OLD {
                node.nr_flag = v;
            }
        }
        if let Some(v) = spec.nd_flag {
            if v != USE_OLD {
                node.nd_flag = v;
            }
        }
        if let Some(v) = spec.np_flag {
            if v != USE_OLD {
                node.np_flag = v;
            }
        }
        if let Some(v) = spec.hc_flag {
            if v != USE_OLD {
                node.hc_flag = v;
            }
        }
        if let Some(v) = spec.ip_afamily {
            if v != USE_OLD {
                node.ip_afamily = v;
            }
        }

        if let Some(pipe) = &spec.pipe {
            node.pipe = Some(pipe.clone());
        }

        if let Some(hosts) = &spec.hosts {
            if !hosts.is_empty() {
                node.hosts = Some(hosts.clone());
            }
        }

        // The password is set only when no password of the node is set yet.
        if let Some(pwd) = &spec.pwd {
            if pwd != "-"
                && node.pwd == "-"
                && !matches!(node.pkt_pwd, NodePassword::Value(_))
                && !matches!(node.out_pwd, NodePassword::Value(_))
            {
                node.pwd = pwd.clone();
                match &spec.pkt_pwd {
                    Some(v) if v != "-" => node.pkt_pwd = NodePassword::Value(v.clone()),
                    Some(_) => node.pkt_pwd = NodePassword::None,
                    None => node.pkt_pwd = NodePassword::Main,
                }
                match &spec.out_pwd {
                    Some(v) if v != "-" => node.out_pwd = NodePassword::Value(v.clone()),
                    Some(_) => node.out_pwd = NodePassword::None,
                    None => node.out_pwd = NodePassword::Main,
                }
            }
        }

        if let Some(flvr) = spec.obox_flvr {
            if flvr != b'-' {
                self.nodes[index].obox_flvr = flvr;
            }
        }
        if let Some(obox) = &spec.obox {
            self.nodes[index].obox = Some(obox.clone());
        }
        if let Some(ibox) = &spec.ibox {
            self.nodes[index].ibox = Some(ibox.clone());
        }

        index
    }

    /// Returns the node by address, or `None`.
    ///
    /// The port of `get_node_info_nolock()`. The C code resolves an unknown
    /// node from `defnode` with DNS; the port leaves that to the network
    /// layer. `now` replaces `safe_time()`.
    pub fn get(&mut self, fa: &FtnAddr, now: i64) -> Option<&FtnNode> {
        if !self.sorted {
            self.sort();
        }
        let index = self.nodes.iter().position(|n| n.fa.compare(fa) == 0)?;
        let node = &mut self.nodes[index];
        // The C code repeats the DNS resolution after RESOLVE_TTL seconds.
        let _ = now;
        if node.hosts.is_none() {
            node.hosts = Some("-".to_string());
        }
        Some(&self.nodes[index])
    }

    /// Returns the node by address for change.
    pub fn get_mut(&mut self, fa: &FtnAddr, now: i64) -> Option<&mut FtnNode> {
        self.get(fa, now)?;
        let index = self.nodes.iter().position(|n| n.fa.compare(fa) == 0)?;
        Some(&mut self.nodes[index])
    }

    /// Calls `f` for every node while `f` returns `false`.
    ///
    /// The port of `foreach_node()`. The C code copies the node array, so a
    /// call can add a node.
    pub fn foreach<F>(&mut self, now: i64, mut f: F)
    where
        F: FnMut(&mut FtnNode) -> bool,
    {
        if !self.sorted {
            self.sort();
        }
        let count = self.nodes.len();
        for i in 0..count {
            let stop = if self.nodes[i].hosts.is_none() {
                // The C code looks the node up again, which can add a node.
                let fa = self.nodes[i].fa.clone();
                match self.get_mut(&fa, now) {
                    Some(node) => f(node),
                    None => false,
                }
            } else {
                f(&mut self.nodes[i])
            };
            if stop {
                break;
            }
        }
    }

    /// Resets the state of every node.
    ///
    /// The port of `q_free(SCAN_LISTED, config)`.
    pub fn reset_state(&mut self) {
        for node in &mut self.nodes {
            node.hold_until = 0;
            node.mail_flvr = 0;
            node.files_flvr = 0;
            node.busy = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(s: &str) -> FtnAddr {
        let table = crate::domain::DomainTable::new(vec![]);
        FtnAddr::parse(s, &table).unwrap()
    }

    #[test]
    fn add_defaults() {
        let mut table = NodeTable::new();
        let index = table.add(&addr("2:5047/100@fidonet"), &NodeSpec::default(), 0);
        let node = &table.nodes()[index];
        assert_eq!(node.pwd, "-");
        assert_eq!(node.obox_flvr, b'f');
        assert_eq!(node.recheck, RESOLVE_TTL);
        assert_eq!(node.listed, NL_UNLISTED);
    }

    #[test]
    fn node_modes() {
        let mut table = NodeTable::new();
        let spec = NodeSpec {
            nr_flag: Some(NR_ON),
            nd_flag: Some(ND_ON),
            ..NodeSpec::default()
        };
        let index = table.add(&addr("2:5047/100@fidonet"), &spec, 0);
        let node = &table.nodes()[index];
        assert!(node.wants_nr());
        assert!(node.wants_nd());
    }

    #[test]
    fn node_modes_default_off() {
        let mut table = NodeTable::new();
        let index = table.add(&addr("2:5047/100@fidonet"), &NodeSpec::default(), 0);
        let node = &table.nodes()[index];
        assert!(!node.wants_nr());
        assert!(!node.wants_nd());
    }

    #[test]
    fn add_password() {
        let mut table = NodeTable::new();
        let spec = NodeSpec {
            pwd: Some("secret".to_string()),
            ..NodeSpec::default()
        };
        let index = table.add(&addr("2:5047/100@fidonet"), &spec, 0);
        let node = &table.nodes()[index];
        assert_eq!(node.pwd, "secret");
        assert_eq!(node.pkt_pwd, NodePassword::Main);
    }

    #[test]
    fn max_flavour() {
        assert_eq!(crate::flavour::max(b'I' as i32, b'f' as i32), b'I' as i32);
        assert_eq!(crate::flavour::max(b'f' as i32, 0), b'f' as i32);
    }
}

//! The BSO queue, the port of `ftnq.c`.

use std::fs;
use std::path::{Path, PathBuf};

use binkd_ftn::addr::FtnAddr;
use binkd_ftn::text::{last_slash, parse_args, safe_atol};

use crate::config::SpoolConfig;
use crate::tryfile::{process_bsy, process_hld};
use binkd_ftn::flavour::max;

pub use binkd_ftn::flavour::PRIO;
/// The flavour letters of a flow file.
pub const FLO_FLAVOURS: &[u8] = b"icdfhICDFH";
/// The flavour letters of a mail packet.
pub const OUT_FLAVOURS: &[u8] = b"icdohICDOH";

/// The kind of a queue entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntryKind {
    /// A mail packet (`.?ut`).
    Mail,
    /// A flow file (`.?lo`).
    Flo,
    /// A status file (`.stc`).
    Status,
    /// A request (`.req`).
    Request,
    /// A file from a filebox.
    Filebox,
}

impl EntryKind {
    /// Returns the letter of the C code. A filebox file has the value 0.
    pub fn as_char(self) -> u8 {
        match self {
            EntryKind::Mail => b'm',
            EntryKind::Flo => b'l',
            EntryKind::Status => b's',
            EntryKind::Request => b'r',
            EntryKind::Filebox => 0,
        }
    }
}

/// A file in the queue.
#[derive(Clone, Debug)]
pub struct QueueEntry {
    /// The address of the receiver.
    pub fa: FtnAddr,
    /// The flavour letter.
    pub flvr: u8,
    /// 0 is none, `d` is delete, `t` is truncate, `s` and `a` are session.
    pub action: u8,
    /// The kind of the file.
    pub kind: EntryKind,
    /// The path of the file.
    pub path: PathBuf,
    /// The size in bytes. 0 means unknown.
    pub size: u64,
    /// The time of the file.
    pub time: i64,
    /// The file is selected for the session.
    pub sent: bool,
}

/// The queue of files to send.
#[derive(Clone, Debug, Default)]
pub struct Queue {
    /// The entries in the order of the C list.
    pub entries: Vec<QueueEntry>,
}

/// Returns the modification time of a path, or 0.
fn mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Returns the size of a path, or 0.
fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Returns the names of a directory in the order of the system.
///
/// The C code uses the `readdir()` order. The order of two entries with the
/// same sort key follows it in both implementations. The corpus avoids such
/// entries, because the order is not the same on every file system.
fn dir_names(path: &Path) -> Vec<String> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Returns true for a name that is not a directory.
fn is_file(path: &Path) -> bool {
    fs::metadata(path).map(|m| !m.is_dir()).unwrap_or(false)
}

impl Queue {
    /// Creates an empty queue.
    pub fn new() -> Queue {
        Queue::default()
    }

    /// Scans the outbound and the fileboxes.
    ///
    /// The port of `q_scan()`. `now` replaces `safe_time()`.
    pub fn scan(config: &mut SpoolConfig, now: i64) -> Queue {
        let mut queue = Queue::new();
        queue.scan_into(config, now, false);
        queue
    }

    /// Scans the spool for the client manager, the port of
    /// `q_scan(SCAN_LISTED, config)` of `ftnq.c`.
    ///
    /// The C code scans the spool with `SCAN_LISTED`, so `q_add_file()` holds
    /// no queue entry and gives the flavours to the node of every file that the
    /// scan reads. The caller of the port calls this before `next_node()`.
    pub fn scan_listed(config: &mut SpoolConfig, now: i64) {
        let mut queue = Queue::new();
        queue.scan_into(config, now, true);
    }

    fn scan_into(&mut self, config: &mut SpoolConfig, now: i64, listed: bool) {
        let domains: Vec<(String, String, String, i32)> = config
            .domains
            .domains()
            .iter()
            .filter(|d| d.alias4.is_none())
            .map(|d| (d.name.clone(), d.path.clone(), d.dir.clone(), d.z[0]))
            .collect();

        for (name, path, dir, z0) in domains {
            let mut outb_path = path.clone();
            if outb_path.ends_with(':') {
                outb_path.push('/');
            }
            if fs::read_dir(&outb_path).is_err() {
                continue;
            }

            let dir_bytes = dir.as_bytes();
            for entry in dir_names(Path::new(&path)) {
                let name_bytes = entry.as_bytes();
                if !starts_with_ignore_case(name_bytes, dir_bytes) {
                    continue;
                }
                let suffix = &name_bytes[dir_bytes.len()..];
                let is_zone_dir = suffix.first() == Some(&b'.')
                    && suffix.get(1).is_some_and(|c| c.is_ascii_hexdigit());
                if !(suffix.is_empty() || is_zone_dir) {
                    continue;
                }

                let mut fa = FtnAddr::new();
                fa.z = if is_zone_dir {
                    parse_hex(&suffix[1..])
                } else {
                    z0
                };
                if suffix.is_empty() || fa.z != z0 {
                    fa.domain = name.clone();
                    let dir_path = Path::new(&path).join(&entry);
                    self.add_dir(&dir_path, &fa, config, now, listed);
                }
            }
        }

        let count = config.nodes.nodes().len();
        for i in 0..count {
            let fa = config.nodes.nodes()[i].fa.clone();
            self.scan_boxes(&[fa], true, config, now, listed);
        }
    }

    /// Adds the files of the addresses to the queue.
    ///
    /// The port of `q_scan_addrs()`.
    pub fn scan_addrs(
        &mut self,
        fa: &[FtnAddr],
        to: bool,
        config: &mut SpoolConfig,
        now: i64,
        listed: bool,
    ) {
        for addr in fa {
            if !to && config.send_if_pwd {
                // Do not give unsecure mail even to a secure link.
                match config.node(addr, now) {
                    Some(node) if node.pwd != "-" => {}
                    _ => continue,
                }
            }
            let filename = addr.to_filename(&config.domains);
            if filename.is_empty() {
                continue;
            }
            if let Some(suffix) = last_slash(filename.as_bytes()) {
                let dir = PathBuf::from(&filename[..filename.len() - suffix.len()]);
                self.add_dir(&dir, addr, config, now, listed);
            }
        }
        self.scan_boxes(fa, to, config, now, listed);
    }

    /// Scans the fileboxes of the addresses.
    ///
    /// The port of `q_scan_boxes()`.
    pub fn scan_boxes(
        &mut self,
        fa: &[FtnAddr],
        to: bool,
        config: &mut SpoolConfig,
        now: i64,
        listed: bool,
    ) {
        for addr in fa {
            let node = config.node(addr, now).cloned();
            if !to && config.send_if_pwd {
                match &node {
                    Some(node) if node.pwd != "-" => {}
                    _ => continue,
                }
            }

            if let Some(node) = &node {
                if let Some(obox) = &node.obox {
                    self.scan_box(
                        Path::new(obox),
                        addr,
                        node.obox_flvr,
                        false,
                        config,
                        now,
                        listed,
                    );
                }
            }

            let bfilebox = config.bfilebox.clone();
            if !bfilebox.is_empty() {
                for (ext, flv) in brake_ext() {
                    let mut path = Path::new(&bfilebox).to_path_buf();
                    path.push(format!(
                        "{}.{}.{}.{}.{}.",
                        addr.domain, addr.z, addr.net, addr.node, addr.p
                    ));
                    let base = path.as_os_str().len();
                    let _ = base;
                    path.push(ext);
                    self.scan_box(&path, addr, flv, config.deleteablebox, config, now, listed);
                }
            }

            let tfilebox = config.tfilebox.clone();
            if !tfilebox.is_empty() {
                let path = Path::new(&tfilebox)
                    .join(format!("{}.{}.{}.{}", addr.z, addr.net, addr.node, addr.p));
                self.scan_box(&path, addr, b'f', config.deleteablebox, config, now, listed);
                let upper = PathBuf::from(format!("{}.H", path.display()));
                let lower = if upper.is_dir() {
                    upper
                } else {
                    PathBuf::from(format!("{}.h", path.display()))
                };
                self.scan_box(
                    &lower,
                    addr,
                    b'h',
                    config.deleteablebox,
                    config,
                    now,
                    listed,
                );

                let path = Path::new(&tfilebox).join(format!(
                    "{}{}{}{}{}{}{}{}.{}{}",
                    to32(addr.z / 32),
                    to32(addr.z % 32),
                    to32(addr.net / 1024),
                    to32((addr.net / 32) % 32),
                    to32(addr.net % 32),
                    to32(addr.node / 1024),
                    to32((addr.node / 32) % 32),
                    to32(addr.node % 32),
                    to32(addr.p / 32),
                    to32(addr.p % 32)
                ));
                self.scan_box(&path, addr, b'f', config.deleteablebox, config, now, listed);
                let upper = PathBuf::from(format!("{}H", path.display()));
                let lower = if upper.is_dir() {
                    upper
                } else {
                    PathBuf::from(format!("{}h", path.display()))
                };
                self.scan_box(
                    &lower,
                    addr,
                    b'h',
                    config.deleteablebox,
                    config,
                    now,
                    listed,
                );
            }
        }
    }

    /// Scans one filebox.
    ///
    /// The port of `q_scan_box()`.
    #[allow(clippy::too_many_arguments)]
    fn scan_box(
        &mut self,
        boxpath: &Path,
        fa: &FtnAddr,
        flvr: u8,
        delete_empty: bool,
        config: &mut SpoolConfig,
        now: i64,
        listed: bool,
    ) {
        let mut count = 0usize;
        if !boxpath.is_dir() {
            return;
        }
        for name in dir_names(boxpath) {
            if name.starts_with('.') {
                continue;
            }
            let path = boxpath.join(&name);
            if !is_file(&path) {
                continue;
            }
            self.add_file(
                &path,
                fa,
                flvr,
                b'd',
                EntryKind::Filebox,
                config,
                now,
                listed,
            );
            count += 1;
        }
        if count == 0 && delete_empty {
            let _ = fs::remove_dir(boxpath);
        }
    }

    /// Adds the files of an outbound directory.
    ///
    /// The port of `q_add_dir()`. The C code takes the file name apart as
    /// `<8 hex net><8 hex node>.<3 chars>`. A name in a `.pnt` directory
    /// holds the point.
    fn add_dir(
        &mut self,
        dir: &Path,
        fa1: &FtnAddr,
        config: &mut SpoolConfig,
        now: i64,
        listed: bool,
    ) {
        for name in dir_names(dir) {
            let s = name.as_bytes();
            if !is_bso_name(s) {
                continue;
            }
            let mut fa2 = fa1.clone();
            if fa1.node != -1 && fa1.p != 0 {
                fa2.p = parse_hex(&s[..8]);
            } else {
                fa2.net = parse_hex(&s[..4]);
                fa2.node = parse_hex(&s[4..8]);
            }
            // Add the file when the wildcard (fa1) matches the address (fa2).
            if fa1.node != -1 && fa1.p != -1 && fa1.compare(&fa2) != 0 {
                continue;
            }

            let path = dir.join(&name);
            let ext = &s[9..];

            if ext.eq_ignore_ascii_case(b"pnt") && fa2.p == -1 {
                if path.is_dir() {
                    self.add_dir(&path, &fa2, config, now, listed);
                }
                continue;
            }
            if fa2.p == -1 {
                fa2.p = 0;
            }

            if ext.eq_ignore_ascii_case(b"bsy") || ext.eq_ignore_ascii_case(b"csy") {
                process_bsy(&fa2, &path, config, now);
            }

            if !config.havedefnode && config.node(&fa2, now).is_none() && !is5d(fa1) {
                continue;
            }

            if ext.len() == 3
                && OUT_FLAVOURS.contains(&s[9])
                && s[10].eq_ignore_ascii_case(&b'u')
                && s[11].eq_ignore_ascii_case(&b't')
            {
                // Add a `*.?ut` mail packet.
                self.add_file(
                    &path,
                    &fa2,
                    s[9],
                    b'd',
                    EntryKind::Mail,
                    config,
                    now,
                    listed,
                );
            } else if ext.eq_ignore_ascii_case(b"req") {
                // Add a `*.req` request.
                self.add_file(
                    &path,
                    &fa2,
                    b'h',
                    b's',
                    EntryKind::Request,
                    config,
                    now,
                    listed,
                );
            } else if ext.eq_ignore_ascii_case(b"hld") {
                process_hld(&fa2, &path, config, now);
            } else if ext.len() == 3
                && FLO_FLAVOURS.contains(&s[9])
                && s[10].eq_ignore_ascii_case(&b'l')
                && s[11].eq_ignore_ascii_case(&b'o')
            {
                // Add a `*.?lo` flow file.
                self.add_file(&path, &fa2, s[9], b'd', EntryKind::Flo, config, now, listed);
            } else if ext.eq_ignore_ascii_case(b"stc") {
                // Add a `*.stc` status file.
                self.add_file(&path, &fa2, b'h', 0, EntryKind::Status, config, now, listed);
            }
        }
    }

    /// Adds a file to the queue.
    ///
    /// The port of `q_add_file()`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_file(
        &mut self,
        filename: &Path,
        fa1: &FtnAddr,
        flvr: u8,
        action: u8,
        kind: EntryKind,
        config: &mut SpoolConfig,
        now: i64,
        listed: bool,
    ) {
        // A file for a shared address is added for every address of the chain.
        let mut shared: Vec<FtnAddr> = Vec::new();
        for share in &config.shares {
            if share.sha.compare(fa1) == 0 {
                shared.extend(share.addrs.iter().cloned());
            }
        }
        for fa in &shared {
            self.add_file(filename, fa, flvr, action, kind, config, now, listed);
        }

        if !listed {
            let mut entry = QueueEntry {
                fa: fa1.clone(),
                flvr,
                action,
                kind,
                path: filename.to_path_buf(),
                size: 0,
                time: 0,
                sent: false,
            };

            if kind == EntryKind::Status {
                let Ok(data) = fs::read(filename) else {
                    return;
                };
                let line = match data.split(|&b| b == b'\n').next() {
                    Some(line) => line,
                    None => return,
                };
                if line.first().is_some_and(|c| is_space(*c)) {
                    return;
                }
                let line = trim_end(line);
                let Some(args) = parse_args(3, line) else {
                    return;
                };
                entry.size = parse_u64(args[1]);
                entry.time = safe_atol(Some(&String::from_utf8_lossy(args[2]))).unwrap_or(0);
                entry.path = PathBuf::from(String::from_utf8_lossy(args[0]).into_owned());
            }

            self.entries.push(entry);
        } else if let Some(node) = config.nodes.get_mut(fa1, now) {
            if kind == EntryKind::Mail {
                node.mail_flvr = max(flvr as i32, node.mail_flvr);
            } else {
                node.files_flvr = max(flvr as i32, node.files_flvr);
            }
        }
    }

    /// Adds a file to the end of the queue.
    ///
    /// The port of `q_add_last_file()`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_last_file(
        &mut self,
        filename: &Path,
        fa1: &FtnAddr,
        flvr: u8,
        action: u8,
        kind: EntryKind,
        config: &mut SpoolConfig,
        now: i64,
        listed: bool,
    ) {
        let mut tail = Queue::new();
        tail.add_file(filename, fa1, flvr, action, kind, config, now, listed);
        self.entries.append(&mut tail.entries);
    }

    /// Sorts the queue.
    ///
    /// The port of `q_sort()`.
    pub fn sort(&mut self, aka: &[FtnAddr]) {
        let mut sorted: Vec<QueueEntry> = Vec::with_capacity(self.entries.len());
        for entry in self.entries.drain(..) {
            let pos = sorted
                .iter()
                .position(|cur| cmp(cur, &entry, aka) > 0)
                .unwrap_or(sorted.len());
            sorted.insert(pos, entry);
        }
        self.entries = sorted;
    }

    /// Marks the next unsent file and returns it.
    ///
    /// The port of `select_next_file()`.
    pub fn select_next_file(&mut self, _fa: &FtnAddr, _aka: &[FtnAddr]) -> Option<usize> {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if !entry.sent {
                entry.sent = true;
                return Some(i);
            }
        }
        None
    }

    /// Returns the size of the netmail and of the files.
    ///
    /// The port of `q_get_sizes()`.
    pub fn get_sizes(&mut self) -> (u64, u64) {
        let mut netsize = 0u64;
        let mut filessize = 0u64;

        for entry in &mut self.entries {
            match entry.kind {
                EntryKind::Flo => {
                    if entry.size != 0 {
                        filessize += entry.size;
                    } else if let Ok(data) = fs::read(&entry.path) {
                        entry.size = 0;
                        entry.time = 0;
                        for line in data.split(|&b| b == b'\n') {
                            if line.first() == Some(&b'~') || line.first() == Some(&b'$') {
                                continue;
                            }
                            let name = line
                                .strip_prefix(b"#")
                                .or_else(|| line.strip_prefix(b"^"))
                                .unwrap_or(line);
                            let name = PathBuf::from(String::from_utf8_lossy(name).into_owned());
                            if let Ok(meta) = fs::metadata(&name) {
                                filessize += meta.len();
                                entry.size += meta.len();
                                let time = mtime(&name);
                                if time > entry.time {
                                    entry.time = time;
                                }
                            }
                        }
                    }
                }
                EntryKind::Status => filessize += entry.size,
                EntryKind::Mail | EntryKind::Request | EntryKind::Filebox => {
                    if entry.size == 0 {
                        entry.size = file_size(&entry.path);
                        entry.time = mtime(&entry.path);
                    }
                    if entry.kind == EntryKind::Mail {
                        netsize += entry.size;
                    } else {
                        filessize += entry.size;
                    }
                }
            }
        }
        (netsize, filessize)
    }

    /// Returns the number of requests in the queue.
    ///
    /// The port of `q_freq_num()`.
    pub fn freq_num(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == EntryKind::Request)
            .count()
    }

    /// Returns the next node with mail, or `None`.
    ///
    /// The port of `q_not_empty()`. `now` replaces `safe_time()`.
    pub fn next_node(config: &mut SpoolConfig, now: i64) -> Option<&mut binkd_ftn::node::FtnNode> {
        let mut best: Option<usize> = None;
        let mut maxflvr = 0i32;
        let count = config.nodes.nodes().len();
        for i in 0..count {
            let node = &config.nodes.nodes()[i];
            if node.busy != 0 || node.hosts.as_deref() == Some("-") || node.hold_until >= now {
                continue;
            }
            let flvr = max(node.mail_flvr, node.files_flvr);
            if maxflvr != max(node.mail_flvr, max(node.files_flvr, maxflvr)) {
                maxflvr = flvr;
                best = Some(i);
            }
        }
        let index = best?;
        if maxflvr == 0 || (maxflvr as u8).eq_ignore_ascii_case(&b'h') {
            return None;
        }
        let node = &mut config.nodes.nodes_mut()[index];
        node.mail_flvr = 0;
        node.files_flvr = 0;
        node.busy = b'c';
        Some(node)
    }
}

/// Returns the flavour of a brake box extension.
fn brake_ext() -> [(&'static str, u8); 10] {
    [
        ("immediate", b'i'),
        ("crash", b'c'),
        ("direct", b'd'),
        ("normal", b'f'),
        ("hold", b'h'),
        ("Immediate", b'i'),
        ("Crash", b'c'),
        ("Direct", b'd'),
        ("Normal", b'f'),
        ("Hold", b'h'),
    ]
}

/// The port of `to32()` of the brake box code.
fn to32(n: i32) -> char {
    if (0..=9).contains(&n) {
        (b'0' + n as u8) as char
    } else if (10..32).contains(&n) {
        (b'A' + n as u8 - 10) as char
    } else {
        'Z'
    }
}

/// Returns the weight of a file name for the filebox order.
fn weight_by_name(s: &[u8]) -> i32 {
    if binkd_ftn::text::ispkt(s) {
        return 100;
    }
    if binkd_ftn::text::isarcmail(s) {
        return 50;
    }
    if binkd_ftn::text::istic(s) {
        return -100;
    }
    0
}

/// Compares two files of a filebox.
fn cmp_filebox_files(a: &QueueEntry, b: &QueueEntry) -> i32 {
    let wa = weight_by_name(a.path.to_string_lossy().as_bytes());
    let wb = weight_by_name(b.path.to_string_lossy().as_bytes());
    if wa - wb == 0 {
        (a.time - b.time) as i32
    } else {
        wb - wa
    }
}

/// Compares two queue entries.
///
/// The port of `q_cmp()`.
pub fn cmp(a: &QueueEntry, b: &QueueEntry, aka: &[FtnAddr]) -> i32 {
    // 1. Sent files go to the end of the queue.
    if a.sent || b.sent {
        return b.sent as i32 - a.sent as i32;
    }
    // 2. Compare the AKA.
    if a.fa.compare(&b.fa) == 0 {
        if fa_is_null(&a.fa) {
            return -1;
        }
        if fa_is_null(&b.fa) {
            return 1;
        }
        for addr in aka {
            if a.fa.compare(addr) == 0 {
                return -1;
            }
            if b.fa.compare(addr) == 0 {
                return 1;
            }
        }
    }
    // 3. Compare the status.
    if a.kind != b.kind {
        let order = [
            EntryKind::Status,
            EntryKind::Request,
            EntryKind::Mail,
            EntryKind::Flo,
        ];
        for kind in order {
            if a.kind == kind {
                return -1;
            }
            if b.kind == kind {
                return 1;
            }
        }
    }
    // 4. Compare the files of a filebox.
    if a.kind == EntryKind::Filebox && b.kind == EntryKind::Filebox {
        return cmp_filebox_files(a, b);
    }
    0
}

/// Returns true when all the address fields are unknown.
fn fa_is_null(fa: &FtnAddr) -> bool {
    fa.domain.is_empty() && fa.z == -1 && fa.net == -1 && fa.node == -1 && fa.p == -1
}

/// Returns true for a 5D address.
fn is5d(fa: &FtnAddr) -> bool {
    fa.z != -1 && fa.node != -1 && fa.net != -1 && fa.p != -1 && !fa.domain.is_empty()
}

/// Returns true for the `xxxxxxxx.xxx` name of a BSO file.
fn is_bso_name(s: &[u8]) -> bool {
    if s.len() != 12 || s[8] != b'.' {
        return false;
    }
    if !s[..8].iter().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    !s[9..].contains(&b'.')
}

/// Returns true when the name starts with the prefix, ignoring case.
fn starts_with_ignore_case(name: &[u8], prefix: &[u8]) -> bool {
    name.len() >= prefix.len() && name[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// Parses a hex number of the leading digits.
fn parse_hex(s: &[u8]) -> i32 {
    let text: String = String::from_utf8_lossy(s)
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    i32::from_str_radix(&text, 16).unwrap_or(0)
}

/// Parses a decimal number of the leading digits.
fn parse_u64(s: &[u8]) -> u64 {
    let text: String = String::from_utf8_lossy(s)
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    text.parse().unwrap_or(0)
}

/// Returns true for the whitespace of `isspace()`.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Removes the trailing whitespace.
fn trim_end(s: &[u8]) -> &[u8] {
    let mut end = s.len();
    while end > 0 && is_space(s[end - 1]) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bso_names() {
        assert!(is_bso_name(b"13b7000d.pkt"));
        assert!(!is_bso_name(b"13b7000d.pktx"));
        assert!(!is_bso_name(b"13b7000d.p.kt"));
        assert!(!is_bso_name(b"13b7000g.pkt"));
    }

    #[test]
    fn hex() {
        assert_eq!(parse_hex(b"13b7"), 0x13b7);
        assert_eq!(parse_hex(b"002"), 2);
        assert_eq!(parse_hex(b""), 0);
    }
}

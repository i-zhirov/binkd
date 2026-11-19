//! The differential test of the spool layer against the golden corpus.
//!
//! The upstream C implementation is checked by the C↔Rust interop jobs; the
//! corpus is the Rust value oracle. A case sets up a spool, runs the
//! operations, and compares the output with the lines after `---`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rbinkd_corpus::{decode_field, encode_field, hex_encode};
use rbinkd_ftn::addr::FtnAddr;
use rbinkd_ftn::domain::FtnDomain;
use rbinkd_ftn::node::{NodeSpec, NL_NODE};
use rbinkd_spool::busy::BusyList;
use rbinkd_spool::config::{Share, SpoolConfig};
use rbinkd_spool::inbound::{Inbound, TFile};
use rbinkd_spool::queue::Queue;
use rbinkd_spool::tryfile;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The state of one case.
struct Runner {
    root: PathBuf,
    config: SpoolConfig,
    queue: Queue,
    busy: BusyList,
    session: Inbound,
    out: Vec<String>,
}

impl Runner {
    fn new(root: PathBuf) -> Runner {
        Runner {
            root,
            config: SpoolConfig::new(),
            queue: Queue::new(),
            busy: BusyList::new(),
            session: Inbound {
                dir: String::new(),
                secure: false,
                fa: Vec::new(),
                nfa: 0,
                skip_all_flag: false,
            },
            out: Vec::new(),
        }
    }

    fn emit(&mut self, line: String) {
        self.out.push(line);
    }

    /// Returns the path relative to the case root.
    fn rel(&self, path: &Path) -> String {
        let root = self.root.to_string_lossy();
        let text = path.to_string_lossy();
        match text.strip_prefix(root.as_ref()) {
            Some(rest) => rest.trim_start_matches(['/', '\\']).replace('\\', "/"),
            None => text.replace('\\', "/"),
        }
    }

    fn parse_addr(&self, s: &str) -> FtnAddr {
        FtnAddr::parse(s, &self.config.domains).expect("bad case address")
    }

    fn mkfile(&self, path: &str, content: Option<&[u8]>, size: usize, mtime: i64) {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let data = match content {
            Some(data) => data.to_vec(),
            None => vec![0u8; size],
        };
        let _ = fs::write(&full, data);
        if mtime > 0 {
            let time = UNIX_EPOCH + Duration::from_secs(mtime as u64);
            if let Ok(f) = fs::OpenOptions::new().write(true).open(&full) {
                let _ = f.set_modified(time);
            }
        }
    }

    /// Lists the files of a directory and its subdirectories, sorted.
    fn walk(&mut self, dir: &Path, prefix: &str) {
        let mut names: Vec<String> = match fs::read_dir(dir) {
            Ok(entries) => entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => return,
        };
        names.sort();
        for name in names {
            let full = dir.join(&name);
            let sub = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            match fs::metadata(&full) {
                Ok(meta) if meta.is_dir() => self.walk(&full, &sub),
                Ok(meta) => self.emit(format!("file\t{sub}\t{}", meta.len())),
                Err(_) => {}
            }
        }
    }

    fn setup(&mut self, fields: &[String]) {
        match fields[1].as_str() {
            "dir" => {
                let _ = fs::create_dir_all(self.root.join(&fields[2]));
            }
            "file" => {
                let size = fields[3].parse().unwrap_or(0);
                let mtime = fields[4].parse().unwrap_or(0);
                self.mkfile(&fields[2], None, size, mtime);
            }
            "text" => {
                let content = decode_field(&fields[3]).expect("bad case text");
                self.mkfile(&fields[2], Some(&content), 0, 0);
            }
            "binary" => {
                let content = rbinkd_corpus::hex_decode(fields[3].as_bytes()).expect("bad hex");
                self.mkfile(&fields[2], Some(&content), 0, 0);
            }
            "session" => match fields[2].as_str() {
                "secure" => {
                    self.session.secure = true;
                    if fields.len() > 3 {
                        self.session.dir =
                            self.root.join(&fields[3]).to_string_lossy().into_owned();
                    }
                }
                "nonsecure" => {
                    self.session.secure = false;
                    if fields.len() > 3 {
                        self.session.dir =
                            self.root.join(&fields[3]).to_string_lossy().into_owned();
                    }
                }
                "inbound" => {
                    self.session.dir = self.root.join(&fields[3]).to_string_lossy().into_owned();
                }
                "aka" => {
                    let fa = self.parse_addr(&fields[3]);
                    self.session.fa.push(fa);
                }
                "nfa" => self.session.nfa = fields[3].parse().unwrap_or(0),
                "skip_all" => self.session.skip_all_flag = true,
                _ => {}
            },
            _ => {}
        }
    }

    fn config(&mut self, fields: &[String]) {
        match fields[1].as_str() {
            "domain" => {
                let mut domains = self.config.domains.domains().to_vec();
                domains.push(FtnDomain {
                    name: fields[2].clone(),
                    path: self.root.to_string_lossy().into_owned(),
                    dir: fields[3].clone(),
                    z: [fields[4].parse().unwrap_or(0), -1],
                    idomain: None,
                    alias4: None,
                });
                self.config.domains = rbinkd_ftn::domain::DomainTable::new(domains);
            }
            "node" => {
                let fa = self.parse_addr(&fields[2]);
                let mut spec = NodeSpec::default();
                if fields[3] != "<null>" {
                    spec.hosts = Some(fields[3].clone());
                }
                if fields[4] != "-" {
                    spec.obox = Some(self.root.join(&fields[4]).to_string_lossy().into_owned());
                }
                let index = self.config.add_node(&fa, &spec, now());
                self.config.nodes.nodes_mut()[index].listed = NL_NODE;
            }
            "aka" => {
                let fa = self.parse_addr(&fields[2]);
                self.config.aka.push(fa);
            }
            "share" => {
                let sha = self.parse_addr(&fields[2]);
                let addrs = fields[3..].iter().map(|a| self.parse_addr(a)).collect();
                self.config.shares.push(Share { sha, addrs });
            }
            "scalar" => {
                let value = &fields[3];
                match fields[2].as_str() {
                    "tries" => self.config.tries = value.parse().unwrap_or(0),
                    "hold" => self.config.hold = value.parse().unwrap_or(0),
                    "remove_try_files" => self.config.remove_try_files = value != "0",
                    "send_if_pwd" => self.config.send_if_pwd = value != "0",
                    "havedefnode" => self.config.havedefnode = value != "0",
                    "kill_old_bsy" => self.config.kill_old_bsy = value.parse().unwrap_or(0),
                    "deleteablebox" => self.config.deleteablebox = value != "0",
                    "tfilebox" => {
                        self.config.tfilebox = self.root.join(value).to_string_lossy().into_owned()
                    }
                    "bfilebox" => {
                        self.config.bfilebox = self.root.join(value).to_string_lossy().into_owned()
                    }
                    "pkthdr_type" => self.config.pkthdr_type = value.parse().unwrap_or(0),
                    "pkthdr_bad" => self.config.pkthdr_bad = Some(value.clone()),
                    "renamestyle" => {
                        self.config.renamestyle = match value.as_str() {
                            "extension" => rbinkd_spool::config::RenameStyle::Extension,
                            "body" => rbinkd_spool::config::RenameStyle::Body,
                            _ => rbinkd_spool::config::RenameStyle::Postfix,
                        }
                    }
                    "inboundcase" => {
                        self.config.inboundcase = match value.as_str() {
                            "upper" => rbinkd_ftn::text::InbCase::Upper,
                            "lower" => rbinkd_ftn::text::InbCase::Lower,
                            "mixed" => rbinkd_ftn::text::InbCase::Mixed,
                            _ => rbinkd_ftn::text::InbCase::Save,
                        }
                    }
                    "minfree" => self.config.minfree = value.parse().unwrap_or(0),
                    "minfree_nonsecure" => {
                        self.config.minfree_nonsecure = value.parse().unwrap_or(0)
                    }
                    "kill_dup_partial_files" => self.config.kill_dup_partial_files = value != "0",
                    "kill_old_partial_files" => {
                        self.config.kill_old_partial_files = value.parse().unwrap_or(0)
                    }
                    "deletedirs" => self.config.deletedirs = value != "0",
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn op(&mut self, args: &[String]) {
        match args[0].as_str() {
            "scan" => self.queue = Queue::scan(&mut self.config, now()),
            "sort" => {
                let aka = self.config.aka.clone();
                self.queue.sort(&aka);
            }
            "select" => {
                let fa = self.parse_addr(&args[1]);
                let aka = self.config.aka.clone();
                match self.queue.select_next_file(&fa, &aka) {
                    Some(index) => {
                        let path = self.queue.entries[index].path.clone();
                        let line = self.rel(&path);
                        self.emit(format!("select\t{line}"));
                    }
                    None => self.emit("select\t-".to_string()),
                }
            }
            "sizes" => {
                let (netsize, filessize) = self.queue.get_sizes();
                self.emit(format!("sizes\t{netsize}\t{filessize}"));
            }
            "freq" => {
                let n = self.queue.freq_num();
                self.emit(format!("freq\t{n}"));
            }
            "dump" => {
                let mut lines = Vec::new();
                for entry in &self.queue.entries {
                    let kind = entry.kind.as_char();
                    lines.push(format!(
                        "queue\t{}\t{}\t{}\t{}\t{}\t{}",
                        self.rel(&entry.path),
                        entry.flvr as char,
                        if entry.action != 0 {
                            entry.action as char
                        } else {
                            '-'
                        },
                        if kind != 0 { kind as char } else { '-' },
                        entry.size,
                        if entry.sent { '+' } else { '-' }
                    ));
                }
                for line in lines {
                    self.emit(line);
                }
            }
            "nodes" => {
                let time = now();
                let mut lines = Vec::new();
                self.config.nodes.foreach(time, |node| {
                    let hold = if node.hold_until == 0 {
                        "0".to_string()
                    } else if node.hold_until <= time {
                        "past".to_string()
                    } else {
                        "future".to_string()
                    };
                    lines.push(format!(
                        "node\t{}\t{}\t{}\t{}\t{}",
                        node.fa.format(false),
                        if node.mail_flvr != 0 {
                            node.mail_flvr as u8 as char
                        } else {
                            '-'
                        },
                        if node.files_flvr != 0 {
                            node.files_flvr as u8 as char
                        } else {
                            '-'
                        },
                        if node.busy != 0 {
                            node.busy as char
                        } else {
                            '-'
                        },
                        hold
                    ));
                    false
                });
                for line in lines {
                    self.emit(line);
                }
            }
            "files" => {
                let root = self.root.clone();
                self.walk(&root, "");
            }
            "hex" | "text" | "exists" => {
                let full = self.root.join(&args[1]);
                let data = fs::read(&full).ok();
                match args[0].as_str() {
                    "exists" => self.emit(format!("exists\t{}\t{}", args[1], data.is_some() as u8)),
                    "hex" => match data {
                        Some(data) => self.emit(format!("hex\t{}\t{}", args[1], hex_encode(&data))),
                        None => self.emit(format!("hex\t{}\t-", args[1])),
                    },
                    _ => match data {
                        Some(data) => {
                            self.emit(format!("text\t{}\t{}", args[1], encode_field(&data)))
                        }
                        None => self.emit(format!("text\t{}\t-", args[1])),
                    },
                }
            }
            "read_try" => {
                let fa = self.parse_addr(&args[1]);
                let (nok, nbad) = tryfile::read_try(&fa, &self.config).unwrap_or((0, 0));
                self.emit(format!("try\t{}\t{nok}\t{nbad}", args[1]));
            }
            "write_try" => {
                let fa = self.parse_addr(&args[1]);
                let nok = args[2].parse().unwrap_or(0);
                let nbad = args[3].parse().unwrap_or(0);
                let comment = args.get(4).cloned().unwrap_or_default();
                tryfile::write_try(&fa, nok, nbad, &comment, &self.config);
            }
            "bad_try" => {
                let fa = self.parse_addr(&args[1]);
                tryfile::bad_try(&fa, &args[2], &mut self.config, now());
            }
            "good_try" => {
                let fa = self.parse_addr(&args[1]);
                let comment = args.get(2).cloned().unwrap_or_default();
                tryfile::good_try(&fa, &comment, &mut self.config);
            }
            "remove_try" => {
                let fa = self.parse_addr(&args[1]);
                tryfile::remove_try(&fa, &self.config);
            }
            "hold" => {
                let fa = self.parse_addr(&args[1]);
                let until = args[2].parse().unwrap_or(0);
                tryfile::hold_node(&fa, until, &mut self.config, now());
            }
            "busy_add" | "busy_test" => {
                let fa = self.parse_addr(&args[1]);
                let bt = args[2].as_bytes()[0];
                let ok = if args[0] == "busy_add" {
                    self.busy.add(&fa, bt, &self.config)
                } else {
                    self.busy.test(&fa, bt, &self.config)
                };
                self.emit(format!("{}\t{}\t{}", args[0], args[1], ok as u8));
            }
            "busy_remove" => {
                let fa = self.parse_addr(&args[1]);
                self.busy.remove(&fa, args[2].as_bytes()[0], &self.config);
            }
            "busy_remove_all" => self.busy.remove_all(&self.config),
            "busy_touch" => self.busy.touch(&self.config, now()),
            "mtime" => {
                let full = self.root.join(&args[1]);
                let recent = fs::metadata(&full)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64 > now() - 10)
                    .unwrap_or(false);
                self.emit(format!(
                    "mtime\t{}\t{}",
                    args[1],
                    if recent { "now" } else { "old" }
                ));
            }
            "set_mtime" => {
                let full = self.root.join(&args[1]);
                let time = UNIX_EPOCH + Duration::from_secs(args[2].parse().unwrap_or(0));
                if let Ok(f) = fs::OpenOptions::new().write(true).open(&full) {
                    let _ = f.set_modified(time);
                }
            }
            "setup_partial" => {
                let hr = self
                    .root
                    .join(&self.session.dir)
                    .join(format!("{}.hr", args[1]));
                let dt = self
                    .root
                    .join(&self.session.dir)
                    .join(format!("{}.dt", args[1]));
                let node = self
                    .session
                    .fa
                    .first()
                    .map(|fa| fa.format(false))
                    .unwrap_or_else(|| "0:0/0.0@unknown".to_string());
                let line = format!("{} {} {} {}\n", args[2], args[3], args[4], node);
                if let Some(parent) = hr.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&hr, line);
                let size: usize = args[3].parse().unwrap_or(0);
                let _ = fs::write(&dt, vec![0u8; size]);
                let time = UNIX_EPOCH + Duration::from_secs(args[4].parse().unwrap_or(0));
                if let Ok(f) = fs::OpenOptions::new().write(true).open(&dt) {
                    let _ = f.set_modified(time);
                }
            }
            "setup_real" => {
                let full = self.root.join(&self.session.dir).join(&args[1]);
                let size: usize = args[2].parse().unwrap_or(0);
                if let Some(parent) = full.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&full, vec![0u8; size]);
            }
            "set_scalar" => {
                if args[1] == "pkthdr_type" {
                    self.config.pkthdr_type = args[2].parse().unwrap_or(0);
                }
            }
            "set_session" => {
                self.session.secure = args[1] == "secure";
            }
            "inb_test" => {
                let size: u64 = args[2].parse().unwrap_or(0);
                let t: i64 = args[3].parse().unwrap_or(0);
                let style = self.config.renamestyle;
                match self.session.inb_test(&args[1], size, t, style) {
                    Some(path) => {
                        let rel = self.rel(Path::new(&path));
                        self.emit(format!("inb_test\t{}\t{rel}", args[1]));
                    }
                    None => self.emit(format!("inb_test\t{}\t-", args[1])),
                }
            }
            "inb_fopen" => {
                let file = TFile {
                    netname: args[1].clone(),
                    size: args[2].parse().unwrap_or(0),
                    time: args[3].parse().unwrap_or(0),
                    start: 0,
                };
                // The upstream C implementation reads the real free space; the test supplies
                // enough for the non-negative `minfree` cases.
                let free = rbinkd_spool::inbound::FreeSpace {
                    inbound: u64::MAX,
                    temp_inbound: u64::MAX,
                };
                let mut rnd = || 0u32;
                let ok = self
                    .session
                    .inb_fopen(&file, &self.config, free, now(), &mut rnd)
                    .is_some();
                self.emit(format!("inb_fopen\t{}\t{}", args[1], ok as u8));
            }
            "inb_reject" => {
                let file = TFile {
                    netname: args[1].clone(),
                    size: args[2].parse().unwrap_or(0),
                    time: args[3].parse().unwrap_or(0),
                    start: 0,
                };
                let mut rnd = || 0u32;
                let ok = self
                    .session
                    .inb_reject(&file, &self.config, now(), &mut rnd);
                self.emit(format!("inb_reject\t{}\t{}", args[1], ok as u8));
            }
            "inb_done" => {
                let file = TFile {
                    netname: args[1].clone(),
                    size: args[2].parse().unwrap_or(0),
                    time: args[3].parse().unwrap_or(0),
                    start: 0,
                };
                let mut rnd = || 0u32;
                let ok = self
                    .session
                    .inb_done(&file, &mut self.config, now(), &mut rnd)
                    .is_some();
                self.emit(format!("inb_done\t{}\t{}", args[1], ok as u8));
            }
            "check_pkthdr" => {
                let (ok, name) = self.session.check_pkthdr(
                    &args[1],
                    &self.root.join(&args[2]).to_string_lossy(),
                    &args[3],
                    &mut self.config,
                    now(),
                );
                self.emit(format!("check_pkthdr\t{}\t{}\t{}", args[1], ok as u8, name));
            }
            "process_hld" => {
                let fa = self.parse_addr(&args[1]);
                if let Some(path) = tryfile::hld_path(&fa, &self.config) {
                    tryfile::process_hld(&fa, &path, &mut self.config, now());
                }
            }
            _ => panic!("unknown op {}", args[0]),
        }
    }
}

fn run_case(path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let mut expected = lines.len();
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("---") {
            expected = i + 1;
            break;
        }
    }

    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "rbinkd-spool-rust-{}",
        std::process::id() as u64 + expected as u64 * 7919
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let mut runner = Runner::new(dir.clone());

    for line in &lines[..expected] {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<String> = line.split('\t').map(|f| f.to_string()).collect();
        match fields[0].as_str() {
            "setup" => runner.setup(&fields),
            "config" => runner.config(&fields),
            "op" => runner.op(&fields[1..]),
            _ => {}
        }
    }

    let want: Vec<&String> = lines[expected..].iter().filter(|l| !l.is_empty()).collect();
    let got: Vec<String> = runner.out.clone();
    if got != want.iter().map(|l| l.to_string()).collect::<Vec<_>>() {
        let mut report = format!("{}: FAIL\n", path.display());
        for i in 0..got.len().max(want.len()) {
            let g = got.get(i).cloned().unwrap_or_else(|| "-".to_string());
            let w = want
                .get(i)
                .map(|l| l.to_string())
                .unwrap_or_else(|| "-".to_string());
            if g != w {
                report.push_str(&format!("  line {}: got \"{g}\", want \"{w}\"\n", i + 1));
            }
        }
        let _ = fs::remove_dir_all(&dir);
        return Err(report);
    }

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn spool_corpus() {
    let dir = rbinkd_corpus::golden_dir().join("spool");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("cannot read the spool corpus")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("tsv"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no spool corpus cases");

    let mut checks = 0;
    let mut failures = Vec::new();
    for file in &files {
        match run_case(file) {
            Ok(()) => checks += 1,
            Err(report) => failures.push(report),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n{}",
        failures.len(),
        checks + failures.len(),
        failures.join("\n")
    );
    eprintln!("ok: rbinkd-spool corpus: {checks} cases passed");
}

#[test]
fn cases_exist() {
    let dir = rbinkd_corpus::golden_dir().join("spool");
    assert!(dir.is_dir());
}

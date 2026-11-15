//! Inbound file handling, the port of the inbound part of `inbound.c`.
//!
//! The C code keeps the metadata of a partial file in a `xxxxxxxx.hr` file
//! and the data in the `xxxxxxxx.dt` file. A complete file is renamed to
//! its real name. The protocol layer owns the session counters and the
//! events; the port returns the name of the file and leaves that work to the
//! caller.

use std::fs;
use std::io::Write;
use std::path::Path;

use binkd_ftn::addr::FtnAddr;
use binkd_ftn::node::HC_OFF;
use binkd_ftn::pkt::pkt_getaddr;
use binkd_ftn::text::{
    isarcmail, ispkt, isreq, istic, makeinboundcase, safe_atol, strdequote, strwipe,
};
use binkd_ftn::token::{getwordx2, GWX_NOESC};

use crate::config::{addrtype, mask_test, RenameStyle, SpoolConfig};
use crate::fsutil;

/// The maximum number of the renamed files of `MAX_INB_RENAME`.
pub const MAX_INB_RENAME: i32 = 10000;

/// A file in transfer, the part that the inbound code needs.
#[derive(Clone, Debug)]
pub struct TFile {
    /// The name of the file on the wire.
    pub netname: String,
    /// The size in bytes.
    pub size: u64,
    /// The time of the file.
    pub time: i64,
    /// The start of the transfer.
    pub start: i64,
}

/// The session state that the inbound code needs.
#[derive(Clone, Debug)]
pub struct Inbound {
    /// The inbound directory of the session.
    pub dir: String,
    /// The session is secure.
    pub secure: bool,
    /// Our addresses of the session.
    pub fa: Vec<FtnAddr>,
    /// The number of our addresses that are not busy.
    pub nfa: usize,
    /// The `skip` directive holds the flag.
    pub skip_all_flag: bool,
}

/// The free space of the inbound directories, in kilobytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct FreeSpace {
    /// The free space of the inbound directory.
    pub inbound: u64,
    /// The free space of the temporary inbound directory.
    pub temp_inbound: u64,
}

/// The result of `find_tmp_name()`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TmpName {
    /// The C code returns 0.
    Error,
    /// The C code found the partial file.
    Found(String),
    /// The C code created the partial file.
    Created(String),
}

fn mtime_of(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Removes the `.hr` file and its `.dt` file.
fn remove_hr(path: &str) {
    let base = path
        .strip_suffix(".hr")
        .or_else(|| path.strip_suffix(".dt"))
        .unwrap_or(path);
    let _ = fs::remove_file(format!("{base}.dt"));
    let _ = fs::remove_file(format!("{base}.hr"));
}

/// Returns true when a partial file is old or holds another size.
///
/// The port of `to_be_deleted()`.
fn to_be_deleted(
    hr_path: &str,
    _netname: &str,
    filesize: u64,
    config: &SpoolConfig,
    now: i64,
) -> bool {
    if config.kill_old_partial_files == 0 {
        return false;
    }
    let Ok(hr) = fs::metadata(hr_path) else {
        return false;
    };
    let base = hr_path.strip_suffix(".hr").unwrap_or(hr_path);
    let dt_path = format!("{base}.dt");
    let (size, mtime) = match fs::metadata(&dt_path) {
        Ok(dt) => (dt.len(), mtime_of(&dt)),
        Err(_) => (hr.len(), mtime_of(&hr)),
    };
    size != filesize && now - mtime > config.kill_old_partial_files
}

/// Returns the next possible real name of an inbound file.
///
/// The port of `next_inb_filename()`. The C code keeps a pointer into the
/// name; the port keeps the index of the character that the pointer points
/// at.
fn next_inb_filename(name: &mut Vec<u8>, pos: &mut usize, style: RenameStyle) -> bool {
    if style == RenameStyle::Postfix {
        if *pos >= name.len() {
            name.extend_from_slice(b".1");
            return true;
        }
        let digits: String = String::from_utf8_lossy(&name[*pos + 1..])
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let num: i64 = digits.parse().unwrap_or(0);
        if num <= 0 || num > MAX_INB_RENAME as i64 {
            return false;
        }
        let next = (num + 1).to_string();
        name.truncate(*pos + 1);
        name.extend_from_slice(next.as_bytes());
        return true;
    }

    if (style == RenameStyle::Extension && *pos >= name.len())
        || (style == RenameStyle::Body && name.get(*pos) == Some(&b'.'))
    {
        // The first change.
        if *pos == 0 {
            return false;
        }
        *pos -= 1;
        name[*pos] = b'0';
        return true;
    }

    let mut p = *pos;
    while p + 1 < name.len() && name[p + 1].is_ascii_alphanumeric() {
        p += 1;
    }
    loop {
        if name[p] == b'9' {
            name[p] = b'a';
        } else if (style == RenameStyle::Extension && name[p] == b'z')
            || (style == RenameStyle::Body && name[p] == b'f')
        {
            name[p] = b'0';
            if p == 0 {
                return false;
            }
            p -= 1;
            if name[p] == b'/' || name[p] == b'\\' || name[p] == b'.' {
                return false;
            }
            if p + 1 != *pos {
                continue;
            }
            *pos = p;
            name[p] = b'0';
            return true;
        } else {
            name[p] += 1;
        }
        break;
    }
    true
}

/// Returns the number of the leading decimal digits.
fn parse_u64(s: &[u8]) -> u64 {
    let text: String = String::from_utf8_lossy(s)
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    text.parse().unwrap_or(0)
}

/// The names of a complete inbound file before the rename, the port of the
/// `tmp_name` and `real_name` values of `inb_done()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboundDone {
    /// The partial file, the port of `$tmpfile`.
    pub tmp: String,
    /// The real name, the port of `$file`.
    pub real: String,
}

impl Inbound {
    /// Returns the directory that holds the partial files.
    fn partial_dir<'a>(&'a self, config: &'a SpoolConfig) -> &'a str {
        if config.temp_inbound.is_empty() {
            &self.dir
        } else {
            &config.temp_inbound
        }
    }

    /// Creates the partial files of a new transfer.
    ///
    /// The port of `creat_tmp_name()`. `rnd` supplies the random names.
    fn creat_tmp_name(
        &self,
        file: &TFile,
        from: Option<&FtnAddr>,
        dir: &str,
        rnd: &mut dyn FnMut() -> u32,
    ) -> Option<String> {
        loop {
            let path = format!("{dir}/{:08x}.hr", rnd());
            if !fsutil::create_sem_file(&path) {
                continue;
            }
            let node = match from {
                Some(fa) => fa.format(false),
                None => "0:0/0.0@unknown".to_string(),
            };
            let line = format!("{} {} {} {}\n", file.netname, file.size, file.time, node);
            let result = fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path)
                .and_then(|mut f| f.write_all(line.as_bytes()));
            if result.is_err() {
                let _ = fs::remove_file(&path);
                return None;
            }
            return Some(path);
        }
    }

    /// Searches for the partial file of the transfer.
    ///
    /// The port of `find_tmp_name()`.
    fn find_tmp_name(
        &self,
        file: Option<&TFile>,
        config: &SpoolConfig,
        now: i64,
        rnd: &mut dyn FnMut() -> u32,
    ) -> TmpName {
        let inbound = self.partial_dir(config);
        let names: Vec<String> = match fs::read_dir(inbound) {
            Ok(entries) => entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => return TmpName::Error,
        };

        let mut found = false;
        let mut hr_path = String::new();
        for name in names {
            let bytes = name.as_bytes();
            if bytes.len() < 8
                || !bytes[..8].iter().all(|c| c.is_ascii_hexdigit())
                || !name[8..].eq_ignore_ascii_case(".hr")
            {
                continue;
            }
            hr_path = format!("{inbound}/{name}");
            let content = match fs::read_to_string(&hr_path) {
                Ok(content) => content,
                Err(_) => continue,
            };
            let Some(line) = content.lines().next() else {
                // The `.hr` file is empty: check the age of the partial.
                if to_be_deleted(&hr_path, "unknown", u64::MAX, config, now) {
                    remove_hr(&hr_path);
                }
                continue;
            };

            let words: Vec<Vec<u8>> = (0..4)
                .map(|i| {
                    getwordx2(
                        Some(line.as_bytes()),
                        i + 1,
                        GWX_NOESC,
                        b" \t\n\r",
                        b" \t\n\r",
                    )
                    .unwrap_or_default()
                })
                .collect();

            if words[3].is_empty() {
                // The `.hr` file holds garbage.
                if to_be_deleted(&hr_path, "unknown", u64::MAX, config, now) {
                    remove_hr(&hr_path);
                }
                continue;
            }

            let address = String::from_utf8_lossy(&words[3]).into_owned();
            let index = FtnAddr::parse(&address, &config.domains)
                .and_then(|fa| self.fa.iter().position(|own| fa.compare(own) == 0));

            match file {
                None => {
                    if index.is_some() && !self.skip_all_flag {
                        remove_hr(&hr_path);
                    }
                }
                Some(file) => {
                    let netname = String::from_utf8_lossy(&words[0]).into_owned();
                    let size = parse_u64(&words[1]);
                    let time = safe_atol(Some(&String::from_utf8_lossy(&words[2]))).unwrap_or(0);
                    if index.is_some() && netname == file.netname {
                        if file.size == size
                            && (file.time & !1) == (time & !1)
                            && index.is_some_and(|i| i < self.fa.len())
                        {
                            // A skip file from a busy AKA: keep the partial.
                            if index.is_some_and(|i| i >= self.nfa) {
                                return TmpName::Error;
                            }
                            found = true;
                        } else if config.kill_dup_partial_files && index.is_some() {
                            remove_hr(&hr_path);
                        }
                    } else if to_be_deleted(&hr_path, &netname, size, config, now) {
                        remove_hr(&hr_path);
                    }
                }
            }

            if found {
                break;
            }
        }

        if file.is_none() {
            return TmpName::Error;
        }
        if !found {
            let Some(path) = self.creat_tmp_name(file.unwrap(), self.fa.first(), inbound, rnd)
            else {
                return TmpName::Error;
            };
            let base = path.strip_suffix(".hr").unwrap_or(&path);
            return TmpName::Created(format!("{base}.dt"));
        }
        let base = hr_path.strip_suffix(".hr").unwrap_or(&hr_path);
        TmpName::Found(format!("{base}.dt"))
    }

    /// Removes every partial file of the remote.
    ///
    /// The port of `inb_remove_partial()`.
    pub fn inb_remove_partial(&self, config: &SpoolConfig, now: i64, rnd: &mut dyn FnMut() -> u32) {
        let _ = self.find_tmp_name(None, config, now, rnd);
    }

    /// Opens the partial file of the transfer for appending.
    ///
    /// The port of `inb_fopen()`. The free space comes from the platform
    /// layer; the policy stays here.
    pub fn inb_fopen(
        &self,
        file: &TFile,
        config: &SpoolConfig,
        free: FreeSpace,
        now: i64,
        rnd: &mut dyn FnMut() -> u32,
    ) -> Option<fs::File> {
        let path = match self.find_tmp_name(Some(file), config, now, rnd) {
            TmpName::Found(path) | TmpName::Created(path) => path,
            TmpName::Error => return None,
        };

        loop {
            let Ok(f) = fs::OpenOptions::new()
                .read(true)
                .append(true)
                .create(true)
                .open(&path)
            else {
                return None;
            };
            let Ok(meta) = f.metadata() else {
                return Some(f);
            };

            let req_free = if self.secure {
                config.minfree
            } else {
                config.minfree_nonsecure
            };
            let freespace2 = free.inbound;
            let freespace =
                if !config.temp_inbound.is_empty() && path.starts_with(&config.temp_inbound) {
                    free.temp_inbound.min(freespace2)
                } else {
                    freespace2
                };

            if meta.len() > file.size {
                // The partial file is longer than the file: delete it.
                drop(f);
                let _ = fs::remove_file(&path);
                continue;
            }

            if req_free >= 0 {
                let need = (file.size - meta.len()).div_ceil(1024) + req_free as u64;
                if freespace < need {
                    return None;
                }
            }
            return Some(f);
        }
    }

    /// Removes the partial file of the transfer.
    ///
    /// The port of `inb_reject()`.
    pub fn inb_reject(
        &self,
        file: &TFile,
        config: &SpoolConfig,
        now: i64,
        rnd: &mut dyn FnMut() -> u32,
    ) -> bool {
        match self.find_tmp_name(Some(file), config, now, rnd) {
            TmpName::Found(path) => {
                remove_hr(&path);
                true
            }
            _ => false,
        }
    }

    /// Finds the partial file and the real name before the rename, the first
    /// part of `inb_done()`.
    ///
    /// The hook of `after_recv()` runs between this call and
    /// [`Inbound::inb_done_prepared`], the port of the `perl_after_recv()`
    /// call of `inb_done()`.
    pub fn inb_prepare(
        &self,
        file: &TFile,
        config: &SpoolConfig,
        now: i64,
        rnd: &mut dyn FnMut() -> u32,
    ) -> Option<InboundDone> {
        let tmp = match self.find_tmp_name(Some(file), config, now, rnd) {
            TmpName::Found(path) => path,
            _ => return None,
        };

        let netname = file.netname.clone();
        let mut base = strdequote(netname.as_bytes());
        makeinboundcase(&mut base, config.inboundcase);
        strwipe(&mut base);
        let real = format!("{}/{}", self.dir, String::from_utf8_lossy(&base));
        Some(InboundDone { tmp, real })
    }

    /// Removes the partial file of a file that the hook took over, the port of
    /// the `sdelete()` calls of `inb_done()`.
    pub fn inb_discard(&self, done: &InboundDone) {
        remove_hr(&done.tmp);
    }

    /// Renames a prepared complete file to its real name.
    ///
    /// The port of the second part of `inb_done()`. Returns the real name, or
    /// `None` when the rename failed. The caller runs the events and the
    /// counters.
    pub fn inb_done_prepared(
        &mut self,
        done: InboundDone,
        file: &TFile,
        config: &mut SpoolConfig,
        now: i64,
    ) -> Option<String> {
        let InboundDone {
            tmp: tmp_name,
            real: mut real_name,
        } = done;
        let netname = file.netname.clone();

        if mask_test(&netname, &config.overwrite).is_some()
            && !ispkt(netname.as_bytes())
            && !isarcmail(netname.as_bytes())
        {
            // The name is in the `overwrite` masks: replace the file.
            let mut renamed = false;
            for i in 0..=10 {
                let _ = fs::remove_file(&real_name);
                match fsutil::rename_noclobber(Path::new(&tmp_name), Path::new(&real_name)) {
                    Ok(()) => {
                        renamed = true;
                        break;
                    }
                    Err(e) => {
                        if !matches!(
                            e.kind(),
                            std::io::ErrorKind::AlreadyExists
                                | std::io::ErrorKind::PermissionDenied
                                | std::io::ErrorKind::WouldBlock
                        ) || i == 10
                        {
                            return None;
                        }
                    }
                }
            }
            if !renamed {
                return None;
            }
        } else {
            // Check the packet header.
            if ispkt(netname.as_bytes()) {
                let (_, checked) = self.check_pkthdr(&netname, &tmp_name, &real_name, config, now);
                real_name = checked;
            }

            let mut name = real_name.into_bytes();
            let mut pos = name.len();
            let mut style = config.renamestyle;
            // For a packet, arc mail, TIC, and request the C code changes
            // the name, not the extension.
            if ispkt(netname.as_bytes())
                || istic(netname.as_bytes())
                || isarcmail(netname.as_bytes())
                || isreq(netname.as_bytes())
            {
                if pos >= 4 {
                    pos -= 4;
                }
                style = RenameStyle::Body;
            }

            fsutil::touch(&tmp_name, file.time);

            loop {
                let current = String::from_utf8_lossy(&name).into_owned();
                match fsutil::rename_noclobber(Path::new(&tmp_name), Path::new(&current)) {
                    Ok(()) => break,
                    Err(e) => {
                        if !matches!(
                            e.kind(),
                            std::io::ErrorKind::AlreadyExists
                                | std::io::ErrorKind::PermissionDenied
                                | std::io::ErrorKind::WouldBlock
                        ) {
                            return None;
                        }
                        if !next_inb_filename(&mut name, &mut pos, style) {
                            if style == RenameStyle::Extension {
                                // The extension ran out: use the postfix style.
                                style = RenameStyle::Postfix;
                                let mut base = strdequote(netname.as_bytes());
                                makeinboundcase(&mut base, config.inboundcase);
                                strwipe(&mut base);
                                name = format!("{}/{}", self.dir, String::from_utf8_lossy(&base))
                                    .into_bytes();
                                pos = name.len();
                                continue;
                            }
                            return None;
                        }
                    }
                }
            }
            real_name = String::from_utf8_lossy(&name).into_owned();
        }

        // Replace the `.dt` extension with `.hr` and remove the partial.
        let base = tmp_name.strip_suffix(".dt").unwrap_or(&tmp_name);
        let _ = fs::remove_file(format!("{base}.hr"));

        Some(real_name)
    }

    /// Renames a complete file to its real name.
    ///
    /// The port of `inb_done()`. Returns the real name, or `None` when the
    /// rename failed. The caller runs the events and the counters.
    pub fn inb_done(
        &mut self,
        file: &TFile,
        config: &mut SpoolConfig,
        now: i64,
        rnd: &mut dyn FnMut() -> u32,
    ) -> Option<String> {
        let done = self.inb_prepare(file, config, now, rnd)?;
        self.inb_done_prepared(done, file, config, now)
    }

    /// Returns true when the packet header holds one of our addresses.
    ///
    /// The port of `check_pkthdr()`. The second value is the real name; the
    /// C code changes the extension of a packet with a bad address.
    pub fn check_pkthdr(
        &self,
        _netname: &str,
        tmp_name: &str,
        real_name: &str,
        config: &mut SpoolConfig,
        now: i64,
    ) -> (bool, String) {
        let mut real_name = real_name.to_string();
        let Some(bad) = config.pkthdr_bad.clone() else {
            return (true, real_name);
        };

        let mut listed = false;
        let mut check = false;
        for addr in &self.fa {
            if let Some(node) = config.nodes.get(addr, now) {
                if node.fa.z > 0 {
                    listed = true;
                }
                if node.hc_flag == HC_OFF {
                    return (true, real_name);
                }
                if node.hc_flag == binkd_ftn::node::HC_ON {
                    check = true;
                }
            }
        }

        if !check
            && (config.pkthdr_type == 0
                || (config.pkthdr_type == addrtype::A_PROT && !self.secure)
                || (config.pkthdr_type == addrtype::A_UNPROT && self.secure)
                || (config.pkthdr_type == addrtype::A_LST && !listed)
                || (config.pkthdr_type == addrtype::A_UNLST && listed))
        {
            return (true, real_name);
        }

        // Parse the packet header.
        let data = fs::read(tmp_name).unwrap_or_default();
        let mut parsed = None;
        if data.len() >= 58 {
            parsed = pkt_getaddr(&data[..58]);
        }
        if let Some(v) = parsed {
            for addr in &self.fa {
                if (v[0] < 0 || addr.z == v[0] as i32)
                    && (v[1] < 0 || addr.net == v[1] as i32)
                    && addr.node == v[2] as i32
                    && (v[3] < 0 || addr.p == v[3] as i32)
                {
                    return (true, real_name);
                }
            }
        }

        // Change the extension to the bad one.
        if let Some(dot) = real_name.rfind('.') {
            let ext_len = real_name.len() - dot - 1;
            if dot > 0 {
                let mut len = bad.len() + 1;
                if len > ext_len {
                    len = ext_len;
                }
                let copy = len.min(bad.len());
                real_name.truncate(dot + 1);
                real_name.push_str(&bad[..copy]);
            }
        }
        (false, real_name)
    }

    /// Tests a file of the inbound directory.
    ///
    /// The port of `inb_test()`. Returns the real name when the file with
    /// the same size and time exists.
    pub fn inb_test(
        &self,
        filename: &str,
        size: u64,
        t: i64,
        ren_style: RenameStyle,
    ) -> Option<String> {
        let mut base = strdequote(filename.as_bytes());
        strwipe(&mut base);
        let mut name = base;
        let mut pos = name.len();
        let mut style = ren_style;
        if ispkt(filename.as_bytes())
            || istic(filename.as_bytes())
            || isarcmail(filename.as_bytes())
            || isreq(filename.as_bytes())
        {
            if pos >= 4 {
                pos -= 4;
            }
            style = RenameStyle::Body;
        }

        loop {
            let path = format!("{}/{}", self.dir, String::from_utf8_lossy(&name));
            let Ok(meta) = fs::metadata(&path) else {
                return None;
            };
            if meta.len() == size && (mtime_of(&meta) & !1) == (t & !1) {
                return Some(path);
            }
            if !next_inb_filename(&mut name, &mut pos, style) {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postfix_names() {
        let mut name = b"file.ext".to_vec();
        let mut pos = name.len();
        assert!(next_inb_filename(&mut name, &mut pos, RenameStyle::Postfix));
        assert_eq!(name, b"file.ext.1");
        assert!(next_inb_filename(&mut name, &mut pos, RenameStyle::Postfix));
        assert_eq!(name, b"file.ext.2");
    }

    #[test]
    fn extension_names() {
        let mut name = b"file.ext".to_vec();
        let mut pos = name.len();
        assert!(next_inb_filename(
            &mut name,
            &mut pos,
            RenameStyle::Extension
        ));
        assert_eq!(name, b"file.ex0");
        assert!(next_inb_filename(
            &mut name,
            &mut pos,
            RenameStyle::Extension
        ));
        assert_eq!(name, b"file.ex1");
    }

    #[test]
    fn body_names() {
        let mut name = b"file.pkt".to_vec();
        let mut pos = name.len() - 4;
        assert!(next_inb_filename(&mut name, &mut pos, RenameStyle::Body));
        assert_eq!(name, b"fil0.pkt");
    }
}

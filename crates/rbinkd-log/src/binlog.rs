//! The binary session logs, the port of `binlog.c`.
//!
//! The C code writes a record of the statistics of a session into the T-Mail
//! file (`binlog`) and into the FrontDoor files (`fdinhist`, `fdouthist`). The
//! records are little-endian and hold no session bytes.

use rbinkd_ftn::addr::FtnAddr;

/// The fields of a session that the binary logs write, the port of the `STATE`
/// fields that `binlog.c` reads.
#[derive(Clone, Debug)]
pub struct SessionStat {
    /// The node that we called, the port of `state->to`, or `None` for a session
    /// that the peer called.
    pub to: Option<FtnAddr>,
    /// Our address of the session, the port of `state->fa`, or `None`.
    pub fa: Option<FtnAddr>,
    /// The start of the session, the port of `state->start_time`.
    pub start_time: i64,
    /// The time of the record, the port of `safe_time()`.
    pub now: i64,
    /// The bytes that we received, the port of `state->bytes_rcvd`.
    pub bytes_received: u64,
    /// The bytes that we sent, the port of `state->bytes_sent`.
    pub bytes_sent: u64,
    /// The files that we received, the port of `state->files_rcvd`.
    pub files_received: i32,
    /// The files that we sent, the port of `state->files_sent`.
    pub files_sent: i32,
    /// The name of our system, the port of `state->sysname`.
    pub sysname: String,
    /// Our location, the port of `state->location`.
    pub location: String,
    /// The session failed, the port of the `status` argument of
    /// `BinLogStat()`.
    pub failed: bool,
}

/// The settings of the binary logs, the port of the `binlog`, `fdinhist`,
/// `fdouthist`, and `tzoff` directives.
#[derive(Clone, Debug, Default)]
pub struct BinLogConfig {
    /// The path of the T-Mail file, or empty.
    pub binlog: String,
    /// The path of the FrontDoor file of an incoming session, or empty.
    pub fdinhist: String,
    /// The path of the FrontDoor file of an outgoing session, or empty.
    pub fdouthist: String,
    /// The offset in seconds, or -1 for the system local offset.
    pub tzoff: i64,
}

/// Writes the records of a session, the port of `BinLogStat()` of `binlog.c`.
///
/// A path that is empty writes no file, as the C code does. An explicit `tzoff`
/// value of seconds selects the offset in whole minutes, truncated toward zero;
/// `-1` selects the system local offset that the caller supplies in minutes,
/// the port of `tz_off()`.
pub fn write(
    config: &BinLogConfig,
    stat: &SessionStat,
    system_offset_minutes: i64,
) -> std::io::Result<()> {
    let tz_minutes = if config.tzoff == -1 {
        system_offset_minutes
    } else {
        rbinkd_ftn::text::tz_off(config.tzoff as i32)
    };
    if !config.binlog.is_empty() {
        let record = t_mail_record(stat, tz_minutes);
        append(&config.binlog, &record)?;
    }
    // The C code returns when the address of the peer is unknown, or when the
    // path of the direction is empty.
    if let Some(fa) = &stat.fa {
        let path = if stat.to.is_some() {
            &config.fdouthist
        } else {
            &config.fdinhist
        };
        if !path.is_empty() {
            let record = frontdoor_record(stat, fa, tz_minutes);
            append(path, &record)?;
        }
    }
    Ok(())
}

/// Returns the record of the T-Mail file, the port of `TLogStat()`.
///
/// The record is 28 bytes, little-endian. The status holds the direction, and a
/// failed session adds 3 to it.
pub fn t_mail_record(stat: &SessionStat, tz_minutes: i64) -> Vec<u8> {
    let (zone, net, node, point, mut status) = match (&stat.to, &stat.fa) {
        // The node that we called.
        (Some(fa), _) => (fa.z, fa.net, fa.node, fa.p, 1),
        // The peer that called us.
        (None, Some(fa)) => (fa.z, fa.net, fa.node, fa.p, 2),
        (None, None) => (0, 0, 0, 0, 0),
    };
    if stat.failed {
        status |= 3;
    }
    let mut record = Vec::with_capacity(28);
    put16(&mut record, zone as u16);
    put16(&mut record, net as u16);
    put16(&mut record, node as u16);
    put16(&mut record, point as u16);
    put32(&mut record, (stat.start_time + tz_minutes * 60) as u32);
    put32(&mut record, (stat.now - stat.start_time) as u32);
    put32(&mut record, stat.bytes_received as u32);
    put32(&mut record, stat.bytes_sent as u32);
    record.push(stat.files_received as u8);
    record.push(stat.files_sent as u8);
    put16(&mut record, status as u16);
    record
}

/// Returns the record of a FrontDoor file, the port of `FDLogStat()`.
///
/// The record is 120 bytes, little-endian. The caller selects the file by the
/// direction of the session, and the record holds the address of the peer.
pub fn frontdoor_record(stat: &SessionStat, fa: &FtnAddr, tz_minutes: i64) -> Vec<u8> {
    let mut record = Vec::with_capacity(120);
    put16(&mut record, fa.z as u16);
    put16(&mut record, fa.net as u16);
    put16(&mut record, fa.node as u16);
    put16(&mut record, fa.p as u16);
    put_string(&mut record, &fa.domain, 16);
    put32(&mut record, (stat.start_time + tz_minutes * 60) as u32);
    put32(&mut record, (stat.now + tz_minutes * 60) as u32);
    put_string(&mut record, &stat.sysname, 32);
    put_string(&mut record, &stat.location, 40);
    put32(&mut record, stat.bytes_received as u32);
    put32(&mut record, stat.bytes_sent as u32);
    // The C code writes the cost as zero: "Let it be free :)"
    put32(&mut record, 0);
    record
}

/// Appends a record to a file, the port of the `fopen(..., "ab")` of
/// `binlog.c`.
fn append(path: &str, record: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;
    file.write_all(record)
}

/// Writes a little-endian 16 bit value, the port of `fput16()`.
fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Writes a little-endian 32 bit value, the port of `fput32()`.
fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Writes a string of a fixed size with NUL padding, the port of the
/// `fwrite()` of a `char` array of `binlog.c`.
fn put_string(out: &mut Vec<u8>, value: &str, size: usize) {
    let bytes = value.as_bytes();
    let count = bytes.len().min(size);
    out.extend_from_slice(&bytes[..count]);
    out.resize(out.len() + size - count, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(s: &str) -> FtnAddr {
        // The address holds its domain, as the C code reads it.
        let table = rbinkd_ftn::domain::DomainTable::new(vec![rbinkd_ftn::domain::FtnDomain {
            name: "fidonet".to_string(),
            path: String::new(),
            dir: String::new(),
            z: [2, -1],
            idomain: None,
            alias4: None,
        }]);
        FtnAddr::parse(s, &table).unwrap()
    }

    fn stat() -> SessionStat {
        SessionStat {
            to: Some(address("2:5020/79")),
            fa: Some(address("2:5020/1")),
            start_time: 1_600_991_232,
            now: 1_600_991_262,
            bytes_received: 0,
            bytes_sent: 4096,
            files_received: 0,
            files_sent: 1,
            sysname: "binkd".to_string(),
            location: "Moscow".to_string(),
            failed: false,
        }
    }

    #[test]
    fn t_mail_outgoing() {
        let record = t_mail_record(&stat(), 0);
        assert_eq!(record.len(), 28);
        // The record of the example of the plan: the node 2:5020/79, the
        // session of 30 seconds, 4096 bytes sent, and one file.
        assert_eq!(&record[0..8], &[2, 0, 0x9c, 0x13, 79, 0, 0, 0]);
        assert_eq!(&record[8..12], &1_600_991_232u32.to_le_bytes());
        assert_eq!(&record[12..16], &30u32.to_le_bytes());
        assert_eq!(&record[16..20], &0u32.to_le_bytes());
        assert_eq!(&record[20..24], &4096u32.to_le_bytes());
        assert_eq!(&record[24..26], &[0, 1]);
        assert_eq!(&record[26..28], &[1, 0]);
    }

    #[test]
    fn t_mail_incoming() {
        let mut stat = stat();
        stat.to = None;
        let record = t_mail_record(&stat, 0);
        // The status of an incoming session is 2.
        assert_eq!(&record[26..], &[2, 0]);
    }

    #[test]
    fn t_mail_failed() {
        let mut stat = stat();
        stat.failed = true;
        let record = t_mail_record(&stat, 0);
        // A failed session adds 3 to the status, so the direction is lost.
        assert_eq!(&record[26..], &[3, 0]);
    }

    #[test]
    fn t_mail_time_offset() {
        let record = t_mail_record(&stat(), 180);
        // The start of the session moves by the offset of three hours.
        assert_eq!(&record[8..12], &(1_600_991_232u32 + 10_800).to_le_bytes());
    }

    #[test]
    fn frontdoor() {
        let stat = stat();
        let fa = address("2:5020/79@fidonet");
        let record = frontdoor_record(&stat, &fa, 0);
        // The record is 4 two byte values, the domain of 16 bytes, two times,
        // the name of 32 bytes, the location of 40 bytes, and three values.
        assert_eq!(record.len(), 116);
        assert_eq!(&record[0..8], &[2, 0, 0x9c, 0x13, 79, 0, 0, 0]);
        assert_eq!(&record[8..24], b"fidonet\0\0\0\0\0\0\0\0\0");
        assert_eq!(&record[24..28], &1_600_991_232u32.to_le_bytes());
        assert_eq!(&record[28..32], &1_600_991_262u32.to_le_bytes());
        assert_eq!(&record[32..40], b"binkd\0\0\0");
        assert_eq!(&record[64..72], b"Moscow\0\0");
        assert_eq!(&record[104..108], &0u32.to_le_bytes());
        assert_eq!(&record[108..112], &4096u32.to_le_bytes());
        // The cost is zero.
        assert_eq!(&record[112..116], &0u32.to_le_bytes());
    }

    #[test]
    fn write_a_file() {
        let dir = std::env::temp_dir().join(format!("binkd-binlog-{}", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let path = dir.display().to_string();
        let config = BinLogConfig {
            binlog: path.clone(),
            fdinhist: String::new(),
            fdouthist: String::new(),
            tzoff: -1,
        };
        write(&config, &stat(), 0).unwrap();
        // A second session appends to the file.
        write(&config, &stat(), 0).unwrap();
        assert_eq!(std::fs::read(&dir).unwrap().len(), 56);
        let _ = std::fs::remove_file(&dir);
    }

    fn write_record(name: &str, tzoff: i64, system_offset_minutes: i64) -> Vec<u8> {
        let path =
            std::env::temp_dir().join(format!("binkd-binlog-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_file(&path);
        let config = BinLogConfig {
            binlog: path.display().to_string(),
            tzoff,
            ..BinLogConfig::default()
        };
        write(&config, &stat(), system_offset_minutes).unwrap();
        let record = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        record
    }

    #[test]
    fn write_uses_the_explicit_offset() {
        // The tzoff value is in seconds, and the start time moves by three
        // hours, whatever the system offset is.
        let record = write_record("explicit", 10800, -240);
        assert_eq!(
            &record[8..12],
            &(stat().start_time as u32 + 10_800).to_le_bytes()
        );
    }

    #[test]
    fn write_uses_the_system_offset() {
        // A tzoff of -1 selects the system offset, here two hours.
        let record = write_record("system", -1, 120);
        assert_eq!(
            &record[8..12],
            &(stat().start_time as u32 + 7_200).to_le_bytes()
        );
    }

    #[test]
    fn write_truncates_an_explicit_offset() {
        // The explicit offset takes whole minutes, so 90 seconds is one minute.
        let record = write_record("truncated", 90, -240);
        assert_eq!(
            &record[8..12],
            &(stat().start_time as u32 + 60).to_le_bytes()
        );
    }
}

//! The XS helpers of the port, the port of the `XS()` functions of
//! `perlhooks.c`.
//!
//! The cases run a script against the helpers and hold the result in a file or
//! in a recording sink. The crate is optional: the cases run with
//! `cargo test -p binkd-perl`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::{DomainTable, FtnDomain};
use binkd_perl::{Hooks, LogSink, MessageSink, Options};

/// A log sink that holds the lines of a test.
#[derive(Clone, Default)]
struct RecordingLog(Arc<Mutex<Vec<(i32, String)>>>);

impl RecordingLog {
    fn entries(&self) -> Vec<(i32, String)> {
        self.0.lock().unwrap().clone()
    }
}

impl LogSink for RecordingLog {
    fn log(&self, level: i32, message: &str) {
        self.0.lock().unwrap().push((level, message.to_string()));
    }
}

/// A message sink that holds the messages of a test.
#[derive(Clone, Default)]
struct RecordingSender(Arc<Mutex<Vec<(u8, String)>>>);

impl RecordingSender {
    fn entries(&self) -> Vec<(u8, String)> {
        self.0.lock().unwrap().clone()
    }
}

impl MessageSink for RecordingSender {
    fn send(&self, kind: u8, text: &str) {
        self.0.lock().unwrap().push((kind, text.to_string()));
    }
}

/// Returns the domain table of the test, the port of the config of the oracle.
fn domains() -> DomainTable {
    DomainTable::new(vec![FtnDomain {
        name: "fidonet".to_string(),
        path: "/tmp/fido".to_string(),
        dir: "outb".to_string(),
        z: [2, -1],
        idomain: None,
        alias4: None,
    }])
}

/// Returns the main address of the test, the port of the `address` line.
fn aka(domains: &DomainTable) -> Vec<FtnAddr> {
    vec![FtnAddr::parse("2:5047/100@fidonet", domains).unwrap()]
}

/// Returns the options of the test with the sinks and the config.
fn options(log: Option<Arc<dyn LogSink>>, sender: Option<Arc<dyn MessageSink>>) -> Options {
    let domains = domains();
    let aka = aka(&domains);
    Options {
        log,
        domains,
        aka,
        sender,
        config: None,
    }
}

/// Writes a script to a directory of the test and returns the two paths.
fn script(name: &str, body: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("binkd-perl-xs-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hooks.pl");
    fs::write(&path, body).unwrap();
    (dir, path)
}

/// Loads a script with the options of the test.
fn load(path: &Path, options: Options) -> bool {
    Hooks::load_with(&path.display().to_string(), options).is_some()
}

#[test]
fn the_log_helper_logs_at_the_level() {
    let recorder = RecordingLog::default();
    let (dir, path) = script(
        "log",
        r#"
Log("hello");
Log(1, "error");
1;
"#,
    );
    let log: Arc<dyn LogSink> = Arc::new(recorder.clone());
    assert!(load(&path, options(Some(log), None)));
    let entries = recorder.entries();
    assert!(entries.contains(&(3, "hello".to_string())), "{entries:?}");
    assert!(entries.contains(&(1, "error".to_string())), "{entries:?}");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_aeq_helper_compares_addresses_and_masks() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-xs-aeq-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let result = dir.join("result.txt");
    let body = format!(
        r#"
my $out = "";
$out .= aeq("2:5047/100@fidonet", "2:5047/100@fidonet") ? "1" : "0";
$out .= aeq("2:5047/100", "2:5047/999@fidonet") ? "1" : "0";
$out .= aeq("2:5047/*", "2:5047/100@fidonet") ? "1" : "0";
$out .= aeq("2:5047/100@fidonet", "2:5047/13@fidonet", "2:5047/100@fidonet") ? "1" : "0";
$out .= defined(my $u = aeq("bad", "2:5047/100@fidonet")) ? ($u ? "1" : "0") : "u";
open(my $fh, ">", "{}") or die "open: $!";
print $fh $out;
close($fh);
1;
"#,
        result.display()
    );
    let path = dir.join("hooks.pl");
    fs::write(&path, body).unwrap();
    assert!(load(&path, options(None, None)));
    let text = fs::read_to_string(&result).unwrap();
    assert_eq!(text, "1011u");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_arm_helper_removes_addresses() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-xs-arm-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let result = dir.join("result.txt");
    let body = format!(
        r#"
my @a = ("2:5047/100@fidonet", "2:5047/13@fidonet", "bad", "");
my $n = arm(\@a, "2:5047/13@fidonet", "2:5047/*");
open(my $fh, ">", "{}") or die "open: $!";
print $fh join("|", @a), " ", $n;
close($fh);
1;
"#,
        result.display()
    );
    let path = dir.join("hooks.pl");
    fs::write(&path, body).unwrap();
    assert!(load(&path, options(None, None)));
    let text = fs::read_to_string(&result).unwrap();
    assert_eq!(text, "bad| 2");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_msg_send_helper_queues_messages() {
    let sender = RecordingSender::default();
    let (dir, path) = script(
        "msg-send",
        r#"
msg_send(0, "hello");
msg_send(3, "file");
1;
"#,
    );
    let sink: Arc<dyn MessageSink> = Arc::new(sender.clone());
    assert!(load(&path, options(None, Some(sink))));
    let entries = sender.entries();
    assert!(entries.contains(&(0, "hello".to_string())), "{entries:?}");
    assert!(entries.contains(&(3, "file".to_string())), "{entries:?}");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_warn_hook_logs_the_lines() {
    let recorder = RecordingLog::default();
    let (dir, path) = script(
        "warn",
        r#"
warn "oops\nsecond";
eval { die "bad\nline" };
1;
"#,
    );
    let log: Arc<dyn LogSink> = Arc::new(recorder.clone());
    assert!(load(&path, options(Some(log), None)));
    let entries = recorder.entries();
    // Perl adds the file and the line to a warning and to a die, so the case
    // holds the start of the line.
    for line in [
        "Perl error: oops",
        "Perl error: second",
        "Perl error: bad",
        "Perl error: line",
    ] {
        assert!(
            entries
                .iter()
                .any(|(level, text)| *level == 1 && text.starts_with(line)),
            "{line}: {entries:?}"
        );
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_helpers_log_the_wrong_parameters() {
    let recorder = RecordingLog::default();
    let (dir, path) = script(
        "wrong",
        r#"
Log();
aeq();
arm();
arm("not an array", "2:5047/100");
msg_send(0);
1;
"#,
    );
    let log: Arc<dyn LogSink> = Arc::new(recorder.clone());
    assert!(load(&path, options(Some(log), None)));
    let entries = recorder.entries();
    for line in [
        "wrong params number to Log (need 1 or 2, exist 0)",
        "aeq() requires 2 or more parameters, 0 exist",
        "arm() requires 2 or more parameters, 0 exist",
        "first parameter to arm() should be array reference",
        "wrong params number to msg_send (needs 2, exist 1)",
    ] {
        assert!(
            entries.contains(&(1, line.to_string())),
            "{line}: {entries:?}"
        );
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_msg_send_helper_needs_a_session() {
    let recorder = RecordingLog::default();
    let (dir, path) = script("msg-send-no-session", "msg_send(0, \"hello\");\n1;\n");
    let log: Arc<dyn LogSink> = Arc::new(recorder.clone());
    assert!(load(&path, options(Some(log), None)));
    let entries = recorder.entries();
    assert!(
        entries.contains(&(1, "can't find $__state pointer".to_string())),
        "{entries:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

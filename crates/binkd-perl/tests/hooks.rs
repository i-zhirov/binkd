//! The callback hooks of the port, the port of the `perl_on_*` functions of
//! `perlhooks.c`.
//!
//! The cases run a script against the hooks and hold the mark of the script.
//! The crate is optional: the cases run with `cargo test -p binkd-perl`.

use std::fs;
use std::path::{Path, PathBuf};

use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::{DomainTable, FtnDomain};
use binkd_perl::{Call, CallError, File, Handshake, Hooks, Options, Received, Sent, SessionVars};

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

/// Returns the options of the test with the config.
fn options() -> Options {
    let domains = domains();
    let aka = aka(&domains);
    Options {
        log: None,
        domains,
        aka,
        sender: None,
        config: None,
    }
}

/// Writes a script to a directory of the test and returns the path.
fn script(name: &str, body: &str) -> (PathBuf, PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("binkd-perl-hooks-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hooks.pl");
    fs::write(&path, body).unwrap();
    (dir, path)
}

/// Returns a wrapper script that runs the shared script and marks the hooks.
fn wrapper(dir: &Path) -> String {
    let hooks = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/perl/hooks.pl");
    let mark = dir.join("mark.txt");
    format!(
        "$ENV{{BINKD_PERL_MARK}} = \"{}\";\ndo \"{}\";\nsub on_exit {{ mark(\"on_exit\"); }}\n1;\n",
        mark.display(),
        hooks.display()
    )
}

#[test]
fn the_callbacks_run() {
    let (dir, path) = script("run", "");
    fs::write(&path, wrapper(&dir)).unwrap();
    let hooks = Hooks::load_with(&path.display().to_string(), options()).unwrap();
    hooks.on_start();
    hooks.config_loaded();
    hooks.on_exit();
    assert_eq!(hooks.need_reload(&["binkd.conf".to_string()], true), 0);
    let mut call = Call {
        addr: FtnAddr::parse("2:5047/13@fidonet", &domains()).unwrap(),
        hosts: "host".to_string(),
        proxy: String::new(),
        socks: String::new(),
        traffic_mail: 0,
        traffic_file: 0,
    };
    assert!(hooks.on_call(&mut call));
    let error = CallError {
        addr: call.addr.clone(),
        error: "failed".to_string(),
        location: 1,
    };
    assert!(hooks.on_error(&error));
    let mut handshake = Handshake::default();
    assert_eq!(hooks.on_handshake(&mut handshake), None);
    assert_eq!(hooks.after_handshake(&SessionVars::default()), None);
    hooks.after_session(&SessionVars::default(), true);
    let file = File::default();
    assert_eq!(hooks.before_recv(&SessionVars::default(), &file, 0), 1);
    let mut received = Received::default();
    assert_eq!(hooks.after_recv(&SessionVars::default(), &mut received), 2);
    let mut sending = File::default();
    assert_eq!(hooks.before_send(&SessionVars::default(), &mut sending), 0);
    let mut sent = Sent::default();
    assert_eq!(hooks.after_sent(&SessionVars::default(), &mut sent), 1);
    hooks.on_send(&SessionVars::default(), 0, "NUL", "");
    hooks.on_recv(&SessionVars::default(), b"raw");
    let mut limit = 100;
    assert_eq!(
        hooks.setup_rlimit(&SessionVars::default(), "file", &mut limit),
        1
    );
    assert_eq!(limit, 100);
    let text = fs::read_to_string(dir.join("mark.txt")).unwrap();
    for name in [
        "start",
        "config_loaded",
        "on_exit",
        "need_reload",
        "call",
        "error",
        "on_handshake",
        "after_handshake",
        "after_session",
        "before_recv",
        "after_recv",
        "before_send",
        "after_sent",
        "on_send",
        "on_recv",
        "setup_rlimit",
    ] {
        assert!(text.lines().any(|line| line == name), "{name}: {text}");
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_callbacks_return_the_values() {
    let (dir, path) = script(
        "values",
        r#"
sub on_call { $hosts = "newhost"; return 1; }
sub need_reload { return 1; }
sub before_recv { return 2; }
sub before_send { $name = "newname"; return 0; }
sub after_sent { $action = "D"; return 1; }
sub setup_rlimit { $rlimit = 42; return 1; }
sub after_recv { $file = "newfile"; return 1; }
sub on_handshake { @me = ("2:5047/13@fidonet"); $passwd = "secret"; return ""; }
sub after_handshake { return "abort"; }
1;
"#,
    );
    let hooks = Hooks::load_with(&path.display().to_string(), options()).unwrap();
    let mut call = Call {
        addr: FtnAddr::parse("2:5047/13@fidonet", &domains()).unwrap(),
        hosts: "oldhost".to_string(),
        proxy: "oldproxy".to_string(),
        socks: "oldsocks".to_string(),
        traffic_mail: 0,
        traffic_file: 0,
    };
    assert!(hooks.on_call(&mut call));
    assert_eq!(call.hosts, "newhost");
    assert_eq!(call.proxy, "oldproxy");
    assert_eq!(call.socks, "oldsocks");
    assert_eq!(hooks.need_reload(&[], false), 1);
    let file = File::default();
    assert_eq!(hooks.before_recv(&SessionVars::default(), &file, 0), 2);
    let mut sending = File {
        name: "oldname".to_string(),
        ..File::default()
    };
    assert_eq!(hooks.before_send(&SessionVars::default(), &mut sending), 0);
    assert_eq!(sending.name, "newname");
    let mut sent = Sent {
        action: 'A',
        ..Sent::default()
    };
    assert_eq!(hooks.after_sent(&SessionVars::default(), &mut sent), 1);
    assert_eq!(sent.action, 'D');
    let mut limit = 100;
    assert_eq!(
        hooks.setup_rlimit(&SessionVars::default(), "file", &mut limit),
        1
    );
    assert_eq!(limit, 42);
    let mut received = Received {
        file: "oldfile".to_string(),
        ..Received::default()
    };
    assert_eq!(hooks.after_recv(&SessionVars::default(), &mut received), 0);
    assert_eq!(received.file, "newfile");
    let mut handshake = Handshake::default();
    assert_eq!(hooks.on_handshake(&mut handshake), None);
    let me = handshake.me.expect("no @me");
    assert_eq!(me.len(), 1);
    assert_eq!(me[0].format(false), "2:5047/13@fidonet");
    assert_eq!(handshake.password.as_deref(), Some("secret"));
    assert_eq!(
        hooks.after_handshake(&SessionVars::default()).as_deref(),
        Some("abort")
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_handshake_aborts() {
    let (dir, path) = script("abort", "sub on_handshake { return \"ERR\"; }\n1;\n");
    let hooks = Hooks::load_with(&path.display().to_string(), options()).unwrap();
    let mut handshake = Handshake::default();
    assert_eq!(hooks.on_handshake(&mut handshake).as_deref(), Some("ERR"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_after_recv_empty_name_answers_two() {
    let (dir, path) = script(
        "after-recv",
        "sub after_recv { $file = \"\"; return 1; }\n1;\n",
    );
    let hooks = Hooks::load_with(&path.display().to_string(), options()).unwrap();
    let mut received = Received {
        file: "old".to_string(),
        ..Received::default()
    };
    assert_eq!(hooks.after_recv(&SessionVars::default(), &mut received), 2);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_script_without_the_subs_keeps_the_defaults() {
    let (dir, path) = script("empty", "1;\n");
    let hooks = Hooks::load_with(&path.display().to_string(), options()).unwrap();
    let mut call = Call {
        addr: FtnAddr::parse("2:5047/13@fidonet", &domains()).unwrap(),
        hosts: "host".to_string(),
        proxy: String::new(),
        socks: String::new(),
        traffic_mail: 0,
        traffic_file: 0,
    };
    assert!(hooks.on_call(&mut call));
    assert_eq!(hooks.need_reload(&[], false), 0);
    assert_eq!(
        hooks.before_recv(&SessionVars::default(), &File::default(), 0),
        0
    );
    let mut limit = 100;
    assert_eq!(
        hooks.setup_rlimit(&SessionVars::default(), "file", &mut limit),
        1
    );
    assert_eq!(limit, 100);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_hook_calls_the_helper_of_the_log() {
    use std::sync::{Arc, Mutex};

    use binkd_perl::LogSink;

    struct Sink(Mutex<Vec<(i32, String)>>);
    impl LogSink for Sink {
        fn log(&self, level: i32, message: &str) {
            if let Ok(mut lines) = self.0.lock() {
                lines.push((level, message.to_string()));
            }
        }
    }

    let (dir, path) = script(
        "context",
        "sub before_recv { Log(\"from hook\"); return 0; }\n1;\n",
    );
    let sink = Arc::new(Sink(Mutex::new(Vec::new())));
    let domains = domains();
    let aka = aka(&domains);
    let hooks = Hooks::load_with(
        &path.display().to_string(),
        Options {
            log: Some(sink.clone()),
            domains,
            aka,
            sender: None,
            config: None,
        },
    )
    .unwrap();
    assert_eq!(
        hooks.before_recv(&SessionVars::default(), &File::default(), 0),
        0
    );
    let lines = sink.0.lock().unwrap();
    assert!(
        lines
            .iter()
            .any(|(level, line)| *level == 3 && line == "from hook"),
        "{lines:?}"
    );
    drop(lines);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn two_sessions_do_not_share_the_variables() {
    let dir = std::env::temp_dir().join(format!(
        "binkd-perl-clones-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let body = format!(
        "sub before_recv {{\n\
         \x20 $counter = ($counter // 0) + 1;\n\
         \x20 my $path = \"{}/$name.txt\";\n\
         \x20 open(my $fh, \">>\", $path) or die \"open $path: $!\";\n\
         \x20 print $fh \"$counter\\n\";\n\
         \x20 close($fh);\n\
         \x20 return 0;\n\
         }}\n\
         1;\n",
        dir.display()
    );
    let (script_dir, path) = script("clones", &body);
    let hooks = Hooks::load_with(&path.display().to_string(), options()).unwrap();
    let first = hooks.clone_for_session().expect("no first clone");
    let second = hooks.clone_for_session().expect("no second clone");
    let run = |hooks: Hooks, name: &'static str| {
        std::thread::spawn(move || {
            for _ in 0..100 {
                let file = File {
                    name: name.to_string(),
                    ..File::default()
                };
                assert_eq!(hooks.before_recv(&SessionVars::default(), &file, 0), 0);
            }
        })
    };
    let a = run(first, "a");
    let b = run(second, "b");
    a.join().expect("the first session panicked");
    b.join().expect("the second session panicked");
    let read = |name: &str| {
        fs::read_to_string(dir.join(format!("{name}.txt")))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.parse::<i32>().ok())
            .collect::<Vec<_>>()
    };
    let expected: Vec<i32> = (1..=100).collect();
    assert_eq!(read("a"), expected, "the first session shared a value");
    assert_eq!(read("b"), expected, "the second session shared a value");
    let _ = fs::remove_dir_all(script_dir);
    let _ = fs::remove_dir_all(dir);
}

/// The values of `after_recv()`: the hook sees the partial file and the real
/// name before the rename, the port of `$tmpfile` and `$file` of
/// `perl_after_recv()`.
#[test]
fn the_after_recv_values_reach_the_script() {
    let (dir, path) = script("after-recv-values", "");
    fs::write(&path, wrapper(&dir)).unwrap();
    let hooks = Hooks::load_with(&path.display().to_string(), options()).unwrap();
    let mut file = Received {
        name: "x.pkt".to_string(),
        size: 10,
        time: 1,
        tmp: "/tmp/x.dt".to_string(),
        file: "/tmp/x".to_string(),
    };
    assert_eq!(hooks.after_recv(&SessionVars::default(), &mut file), 0);
    let text = fs::read_to_string(dir.join("mark.txt")).unwrap();
    assert!(
        text.contains("after_recv tmpfile=/tmp/x.dt file=/tmp/x"),
        "{text}"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_session_variables_reach_the_script() {
    let (dir, path) = script("session-vars", "");
    let mark = dir.join("mark.txt");
    fs::write(
        &path,
        format!(
            "sub before_recv {{\n\
             \x20 open(my $fh, \">>\", \"{}\") or die \"open: $!\";\n\
             \x20 print $fh \"$start|$host|$ip|$our_ip|$our_port|$sysname|$sysop|$location|$opt{{ND}}|$opt{{NR}}|$opt{{MD}}|$opt{{crypt}}|$opt{{GZ}}\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            mark.display()
        ),
    )
    .unwrap();
    let hooks = Hooks::load(&path.display().to_string()).unwrap();
    let vars = SessionVars {
        start: 1_000_000_000,
        host: "peer.example".to_string(),
        ip: "192.0.2.1".to_string(),
        our_ip: "198.51.100.2".to_string(),
        our_port: 24554,
        sysname: "Peer system".to_string(),
        sysop: "Peer sysop".to_string(),
        location: "Peer location".to_string(),
        opt: binkd_perl::Opt {
            nd: 1,
            nr: 1,
            md: 0,
            crypt: 1,
            gz: 3,
        },
        ..SessionVars::default()
    };
    assert_eq!(hooks.before_recv(&vars, &File::default(), 0), 0);
    let text = fs::read_to_string(&mark).unwrap();
    assert_eq!(
        text.trim_end(),
        "1000000000|peer.example|192.0.2.1|198.51.100.2|24554|Peer system|Peer sysop|Peer location|1|1|0|1|3"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_traffic_and_compression_variables_reach_the_script() {
    let (dir, path) = script("traffic-vars", "");
    let mark = dir.join("mark.txt");
    fs::write(
        &path,
        format!(
            "sub before_recv {{\n\
             \x20 open(my $fh, \">>\", \"{}\") or die \"open: $!\";\n\
             \x20 print $fh \"$traf_mail|$traf_file|$z_send|$z_recv\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            mark.display()
        ),
    )
    .unwrap();
    let hooks = Hooks::load(&path.display().to_string()).unwrap();
    let vars = SessionVars {
        traffic_mail: 100,
        traffic_file: 200,
        z_send: 2,
        z_recv: 1,
        ..SessionVars::default()
    };
    assert_eq!(hooks.before_recv(&vars, &File::default(), 0), 0);
    let text = fs::read_to_string(&mark).unwrap();
    assert_eq!(text.trim_end(), "100|200|2|1");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_session_queue_reaches_the_script() {
    let (dir, path) = script("session-queue", "");
    let mark = dir.join("mark.txt");
    fs::write(
        &path,
        format!(
            "sub before_recv {{\n\
             \x20 my $n = scalar @queue;\n\
             \x20 my $q = $queue[0];\n\
             \x20 open(my $fh, \">>\", \"{}\") or die \"open: $!\";\n\
             \x20 print $fh \"$n|$q->{{file}}|$q->{{size}}|$q->{{time}}|$q->{{sent}}|$q->{{flvr}}|$q->{{action}}|$q->{{type}}|$q->{{addr}}\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            mark.display()
        ),
    )
    .unwrap();
    let hooks = Hooks::load(&path.display().to_string()).unwrap();
    let entry = binkd_perl::QueueFile {
        file: "/tmp/a.pkt".to_string(),
        size: 100,
        time: 123,
        sent: 1,
        flvr: 'f',
        action: 'd',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let expected = format!("1|/tmp/a.pkt|100|123|1|f|d|m|{}", entry.addr.format(false));
    let vars = SessionVars {
        queue: vec![entry],
        ..SessionVars::default()
    };
    assert_eq!(hooks.before_recv(&vars, &File::default(), 0), 0);
    let text = fs::read_to_string(&mark).unwrap();
    assert_eq!(text.trim_end(), expected);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_script_rebuilds_the_session_queue() {
    let (dir, path) = script("refresh-queue", "");
    fs::write(
        &path,
        "sub before_send {\n\
         \x20 $queue[0]{size} = 400;\n\
         \x20 push @queue, {\n\
         \x20   file => \"/tmp/second.cut\",\n\
         \x20   size => 200,\n\
         \x20   time => 456,\n\
         \x20   sent => 0,\n\
         \x20   flvr => \"m\",\n\
         \x20   action => \"d\",\n\
         \x20   type => \"m\",\n\
         \x20   addr => \"2:5047/13@fidonet\",\n\
         \x20 };\n\
         \x20 push @queue, { size => 1 };\n\
         \x20 return 0;\n\
         }\n\
         1;\n",
    )
    .unwrap();
    let hooks = Hooks::load(&path.display().to_string()).unwrap();
    let entry = binkd_perl::QueueFile {
        file: "/tmp/first.pkt".to_string(),
        size: 100,
        time: 123,
        sent: 1,
        flvr: 'f',
        action: '\0',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let vars = SessionVars {
        queue: vec![entry],
        ..SessionVars::default()
    };
    assert_eq!(hooks.before_send(&vars, &mut File::default()), 0);
    let fallback = FtnAddr::parse("2:5047/13@fidonet", &domains()).unwrap();
    let queue = hooks
        .refresh_queue(&fallback)
        .expect("the script holds no queue");
    assert_eq!(queue.len(), 2, "{queue:?}");
    // The script changed the first entry.
    assert_eq!(queue[0].file, "/tmp/first.pkt");
    assert_eq!(queue[0].size, 400);
    assert_eq!(queue[0].sent, 1);
    // The script added the second entry, and the port skipped the entry
    // without a file.
    assert_eq!(queue[1].file, "/tmp/second.cut");
    assert_eq!(queue[1].size, 200);
    assert_eq!(queue[1].time, 456);
    assert_eq!(queue[1].sent, 0);
    assert_eq!(queue[1].flvr, 'm');
    assert_eq!(queue[1].action, 'd');
    assert_eq!(queue[1].type_, 'm');
    assert_eq!(queue[1].addr.format(false), "2:5047/13");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn an_empty_queue_clears_the_session_queue() {
    let (dir, path) = script("empty-queue", "");
    fs::write(
        &path,
        "sub before_send {\n\x20 @queue = ();\n\x20 return 0;\n}\n1;\n",
    )
    .unwrap();
    let hooks = Hooks::load(&path.display().to_string()).unwrap();
    let vars = SessionVars::default();
    assert_eq!(hooks.before_send(&vars, &mut File::default()), 0);
    let fallback = FtnAddr::parse("2:5047/13@fidonet", &domains()).unwrap();
    // The C code answers an empty list for a defined but empty `@queue`, so
    // the session holds no file.
    let queue = hooks.refresh_queue(&fallback).expect("no queue");
    assert!(queue.is_empty(), "{queue:?}");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_script_removes_an_entry_from_the_session_queue() {
    let (dir, path) = script("remove-queue", "");
    fs::write(
        &path,
        "sub before_send {\n\
         \x20 splice(@queue, 1, 1);\n\
         \x20 return 0;\n\
         }\n\
         1;\n",
    )
    .unwrap();
    let hooks = Hooks::load(&path.display().to_string()).unwrap();
    let first = binkd_perl::QueueFile {
        file: "/tmp/first.pkt".to_string(),
        size: 100,
        time: 123,
        sent: 1,
        flvr: 'f',
        action: '\0',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let second = binkd_perl::QueueFile {
        file: "/tmp/second.pkt".to_string(),
        size: 200,
        time: 456,
        sent: 0,
        flvr: 'f',
        action: '\0',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let vars = SessionVars {
        queue: vec![first, second],
        ..SessionVars::default()
    };
    assert_eq!(hooks.before_send(&vars, &mut File::default()), 0);
    let fallback = FtnAddr::parse("2:5047/13@fidonet", &domains()).unwrap();
    let queue = hooks
        .refresh_queue(&fallback)
        .expect("the script holds no queue");
    assert_eq!(queue.len(), 1, "{queue:?}");
    assert_eq!(queue[0].file, "/tmp/first.pkt");
    assert_eq!(queue[0].size, 100);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_script_deletes_an_entry_of_before_recv() {
    let (dir, path) = script("before-recv-remove", "");
    fs::write(
        &path,
        "sub before_recv {\n\
         \x20 splice(@queue, 1, 1);\n\
         \x20 return 0;\n\
         }\n\
         1;\n",
    )
    .unwrap();
    let hooks = Hooks::load(&path.display().to_string()).unwrap();
    let first = binkd_perl::QueueFile {
        file: "/tmp/first.pkt".to_string(),
        size: 100,
        time: 123,
        sent: 0,
        flvr: 'f',
        action: '\0',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let second = binkd_perl::QueueFile {
        file: "/tmp/second.pkt".to_string(),
        size: 200,
        time: 456,
        sent: 0,
        flvr: 'f',
        action: '\0',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let vars = SessionVars {
        queue: vec![first, second],
        ..SessionVars::default()
    };
    // The C `perl_before_recv()` answers the return value of the hook and it
    // does not call `refresh_queue()`, so the session keeps its queue. The
    // array of the interpreter holds the deletion, which a later hook that
    // calls `setup_queue()` overwrites.
    assert_eq!(hooks.before_recv(&vars, &File::default(), 0), 0);
    let fallback = FtnAddr::parse("2:5047/13@fidonet", &domains()).unwrap();
    let queue = hooks
        .refresh_queue(&fallback)
        .expect("the script holds no queue");
    assert_eq!(queue.len(), 1, "{queue:?}");
    assert_eq!(queue[0].file, "/tmp/first.pkt");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_script_removes_an_entry_of_after_recv() {
    let (dir, path) = script("after-recv-remove", "");
    fs::write(
        &path,
        "sub after_recv {\n\
         \x20 splice(@queue, 1, 1);\n\
         \x20 return 0;\n\
         }\n\
         1;\n",
    )
    .unwrap();
    let hooks = Hooks::load(&path.display().to_string()).unwrap();
    let first = binkd_perl::QueueFile {
        file: "/tmp/first.pkt".to_string(),
        size: 100,
        time: 123,
        sent: 1,
        flvr: 'f',
        action: '\0',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let second = binkd_perl::QueueFile {
        file: "/tmp/second.pkt".to_string(),
        size: 200,
        time: 456,
        sent: 0,
        flvr: 'f',
        action: '\0',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let vars = SessionVars {
        queue: vec![first, second],
        ..SessionVars::default()
    };
    let mut received = Received::default();
    assert_eq!(hooks.after_recv(&vars, &mut received), 0);
    let fallback = FtnAddr::parse("2:5047/13@fidonet", &domains()).unwrap();
    let queue = hooks
        .refresh_queue(&fallback)
        .expect("the script holds no queue");
    assert_eq!(queue.len(), 1, "{queue:?}");
    assert_eq!(queue[0].file, "/tmp/first.pkt");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_script_deletes_an_entry_of_after_sent() {
    let (dir, path) = script("after-sent-remove", "");
    fs::write(
        &path,
        "sub after_sent {\n\
         \x20 splice(@queue, 1, 1);\n\
         \x20 return 0;\n\
         }\n\
         1;\n",
    )
    .unwrap();
    let hooks = Hooks::load(&path.display().to_string()).unwrap();
    let first = binkd_perl::QueueFile {
        file: "/tmp/first.pkt".to_string(),
        size: 100,
        time: 123,
        sent: 1,
        flvr: 'f',
        action: '\0',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let second = binkd_perl::QueueFile {
        file: "/tmp/second.pkt".to_string(),
        size: 200,
        time: 456,
        sent: 0,
        flvr: 'f',
        action: '\0',
        type_: 'm',
        addr: FtnAddr::parse("2:5047/100@fidonet", &domains()).unwrap(),
    };
    let vars = SessionVars {
        queue: vec![first, second],
        ..SessionVars::default()
    };
    // The C `perl_after_sent()` calls `setup_queue()` before the hook and it
    // does not call `refresh_queue()`, so the session keeps its queue. The
    // array of the interpreter holds the deletion, which a later hook that
    // calls `setup_queue()` overwrites.
    assert_eq!(hooks.after_sent(&vars, &mut Sent::default()), 0);
    let fallback = FtnAddr::parse("2:5047/13@fidonet", &domains()).unwrap();
    let queue = hooks
        .refresh_queue(&fallback)
        .expect("the script holds no queue");
    assert_eq!(queue.len(), 1, "{queue:?}");
    assert_eq!(queue[0].file, "/tmp/first.pkt");
    let _ = fs::remove_dir_all(dir);
}

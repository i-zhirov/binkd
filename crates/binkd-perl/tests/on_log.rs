//! The hook of the log of the port, the port of `perl_on_log()` of `perlhooks.c`.
//!
//! The case runs the port against the script of the oracle of the C code,
//! `tests/perl/hooks.pl`, so the two hold one behaviour. The crate is optional: the
//! case runs with `cargo test -p binkd-perl`, and a system that holds no
//! interpreter answers no hook and the case holds the port of the C code.

use std::path::Path;

use binkd_perl::Hooks;

/// Returns the path of the script of the oracle.
fn script() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/perl/hooks.pl")
        .display()
        .to_string()
}

#[test]
fn the_hook_of_the_log_changes_the_message_and_the_level() {
    let Some(hooks) = Hooks::load(&script()) else {
        return;
    };
    let mut message = b"Exit on option '-n'".to_vec();
    let mut level = 0;
    // The C code of `perl_on_log()` answers 1 when the hook keeps the message,
    // the port of the check of `vLog()`.
    assert!(hooks.on_log(&mut message, &mut level));
    assert_eq!(
        String::from_utf8_lossy(&message),
        "Exit on option (perl) '-n'"
    );
    assert_eq!(level, 1);
}

#[test]
fn the_hook_of_the_log_suppresses_the_message() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-suppress-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("hooks.pl");
    // The C code of `perl_on_log()` answers 0 for the value 0 and for an
    // undefined value.
    std::fs::write(&script, "sub on_log { return 0; }\n1;\n").unwrap();
    let Some(hooks) = Hooks::load(&script.display().to_string()) else {
        return;
    };
    let mut message = b"hello".to_vec();
    let mut level = 3;
    assert!(!hooks.on_log(&mut message, &mut level));
    assert_eq!(message, b"hello");
    assert_eq!(level, 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_script_without_the_hook_keeps_the_message() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-no-hook-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("hooks.pl");
    std::fs::write(&script, "1;\n").unwrap();
    let Some(hooks) = Hooks::load(&script.display().to_string()) else {
        return;
    };
    let mut message = b"hello".to_vec();
    let mut level = 3;
    assert!(hooks.on_log(&mut message, &mut level));
    assert_eq!(message, b"hello");
    assert_eq!(level, 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_broken_script_answers_no_hooks() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-broken-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("hooks.pl");
    std::fs::write(&script, "die \"boom\";\n").unwrap();
    assert!(Hooks::load(&script.display().to_string()).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn concurrent_calls_do_not_race_on_the_master() {
    use std::sync::Arc;

    use binkd_log::{DateTime, Log, Outcome, DEBUG, FATAL, INFO};
    use binkd_perl::LogHook;

    let dir = std::env::temp_dir().join(format!(
        "binkd-perl-concurrent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mark = dir.join("mark.txt");
    let log_path = dir.join("binkd.log");
    let script = dir.join("hooks.pl");
    std::fs::write(
        &script,
        format!(
            "sub on_log {{\n\
             \x20 $counter = ($counter // 0) + 1;\n\
             \x20 select(undef, undef, undef, 0.001);\n\
             \x20 s/^msg/marked/;\n\
             \x20 open(my $fh, \">>\", \"{}\") or die \"open: $!\";\n\
             \x20 print $fh \"$counter\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 1;\n\
             }}\n\
             1;\n",
            mark.display()
        ),
    )
    .unwrap();
    let Some(hooks) = Hooks::load(&script.display().to_string()) else {
        return;
    };
    let log = Arc::new(Log::new(
        DEBUG,
        FATAL,
        &log_path.display().to_string(),
        Vec::new(),
    ));
    log.set_hook(Some(Arc::new(LogHook::new(hooks))));
    let now = DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    };
    let mut threads = Vec::new();
    for _ in 0..2 {
        let log = Arc::clone(&log);
        threads.push(std::thread::spawn(move || {
            for _ in 0..50 {
                assert_eq!(log.message(INFO, "msg", &now), Outcome::Written);
            }
        }));
    }
    for thread in threads {
        thread.join().expect("the thread panicked");
    }
    let text = std::fs::read_to_string(&log_path).unwrap_or_default();
    let marked = text.lines().filter(|line| line.contains("marked")).count();
    assert_eq!(marked, 100, "the hook did not see every message");
    let counters: Vec<i32> = std::fs::read_to_string(&mark)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.parse().ok())
        .collect();
    assert_eq!(counters.len(), 100, "the hook did not run 100 times");
    let mut sorted = counters;
    sorted.sort();
    assert_eq!(sorted, (1..=100).collect::<Vec<_>>());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn two_clones_call_the_log_hook_on_two_threads() {
    let dir = std::env::temp_dir().join(format!(
        "binkd-perl-log-clones-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mark = dir.join("mark.txt");
    let script = dir.join("hooks.pl");
    std::fs::write(
        &script,
        format!(
            "sub on_log {{\n\
             \x20 $counter = ($counter // 0) + 1;\n\
             \x20 my $text = $_;\n\
             \x20 s/^msg/marked/;\n\
             \x20 open(my $fh, \">>\", \"{}\") or die \"open: $!\";\n\
             \x20 print $fh \"$counter $text\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 1;\n\
             }}\n\
             1;\n",
            mark.display()
        ),
    )
    .unwrap();
    let Some(hooks) = Hooks::load(&script.display().to_string()) else {
        return;
    };
    let first = hooks.clone_for_session().expect("no first clone");
    let second = hooks.clone_for_session().expect("no second clone");
    let run = |hooks: Hooks, name: &'static str| {
        std::thread::spawn(move || {
            for _ in 0..50 {
                let mut message = format!("msg-{name}").into_bytes();
                let mut level = 3;
                assert!(hooks.on_log(&mut message, &mut level));
                assert_eq!(String::from_utf8_lossy(&message), format!("marked-{name}"));
            }
        })
    };
    let first = run(first, "a");
    let second = run(second, "b");
    first.join().expect("the first thread panicked");
    second.join().expect("the second thread panicked");
    let lines: Vec<(i32, String)> = std::fs::read_to_string(&mark)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, ' ');
            Some((parts.next()?.parse().ok()?, parts.next()?.to_string()))
        })
        .collect();
    assert_eq!(lines.len(), 100, "the hook did not run 100 times");
    for name in ["a", "b"] {
        let mut counters: Vec<i32> = lines
            .iter()
            .filter(|(_, text)| text == &format!("msg-{name}"))
            .map(|(counter, _)| *counter)
            .collect();
        counters.sort();
        assert_eq!(
            counters,
            (1..=50).collect::<Vec<_>>(),
            "the clone {name} shared a value"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

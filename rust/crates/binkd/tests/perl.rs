//! The hooks of the Perl oracle, the port of `perlhooks.c`.
//!
//! The C code links the hooks only with `./configure --with-perl`, whose default
//! is no, and no test of the C tree names a hook. The test runs the oracle of
//! `BINKD_C_PERL` with the script of `tests/perl/hooks.pl` and holds the line
//! that the hook of the log changed, the port of `perl_on_log()`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Returns the path of the C binary of the Perl hooks, when the environment
/// names it.
fn oracle() -> Option<PathBuf> {
    std::env::var("BINKD_C_PERL").ok().map(PathBuf::from)
}

#[test]
fn the_oracle_of_the_perl_hook_changes_the_line_of_the_log() {
    let Some(oracle) = oracle() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("binkd-perl-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/perl/hooks.pl");
    let log = dir.join("binkd.log");
    let text = format!(
        "domain fidonet {} 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"Test system\"\n\
         sysop \"Test sysop\"\n\
         location \"Test location\"\n\
         nodeinfo \"Test node\"\n\
         inbound {}\n\
         perl-hooks {}\n\
         loglevel 8\n\
         conlog 8\n\
         log {}\n",
        dir.join("outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();

    let out = Command::new(&oracle)
        .arg("-n")
        .arg(&config)
        .output()
        .expect("cannot run the oracle");
    let text = fs::read_to_string(&log).unwrap_or_default();
    // The hook of the port changes the message of a line of the log, the port
    // of the `$_` of `perl_on_log()` of `perlhooks.c`. The C code writes
    // the `BEGIN` line before the hooks run, so the hook changes the line of
    // the `-n` switch, whose level the hook also raises from 0 to 1.
    assert!(
        text.contains("Exit on option (perl) '-n'"),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

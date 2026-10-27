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
        .join("../../../tests/perl/hooks.pl")
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
    // The C code of `perl_on_log()` answers 1 when the hook changed the two, the
    // port of the check of `vLog()`.
    assert!(hooks.on_log(&mut message, &mut level));
    assert_eq!(
        String::from_utf8_lossy(&message),
        "Exit on option (perl) '-n'"
    );
    assert_eq!(level, 1);
}

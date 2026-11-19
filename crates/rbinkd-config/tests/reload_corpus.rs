//! The reload of the config against the golden corpus.
//!
//! A case is a config file `<name>.conf` and a changed variant
//! `<name>.changed`. The test loads the case, writes the variant over the case file,
//! runs the reload, and compares the dump of the new config with the expectation of
//! the variant. The C binary made the expectation with `binkd -d <name>.conf` on the
//! changed file, and the test runs it again when `BINKD_C` is set.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

use rbinkd_config::{dump, Outcome, Parser, Reloader};

/// Returns the directory of the config corpus.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/config")
}

/// Normalizes the line endings of a dump.
fn normalize(text: String) -> String {
    text.replace("\r\n", "\n")
}

/// Copies the corpus into a temporary directory.
fn copy_corpus(dir: &Path) -> PathBuf {
    let target = std::env::temp_dir().join(format!(
        "binkd-reload-{}-{}",
        std::process::id(),
        dir.file_name().unwrap().to_string_lossy()
    ));
    let _ = fs::remove_dir_all(&target);
    copy_dir(dir, &target);
    target
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}

/// Writes the changed variant over the case file and moves the time of the file.
fn change(dir: &Path, name: &str) {
    let variant = fs::read_to_string(dir.join(format!("{name}.changed"))).unwrap();
    let case = dir.join(format!("{name}.conf"));
    fs::write(&case, variant).unwrap();
    let time = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
    let file = fs::OpenOptions::new().write(true).open(&case).unwrap();
    file.set_modified(time).unwrap();
}

#[test]
fn reload_corpus() {
    let dir = corpus_dir();
    let cases = ["reload_basic", "reload_node"];
    let mut failures = Vec::new();
    let mut checks = 0;

    for name in cases {
        let work = copy_corpus(&dir);

        // The config of the start.
        let mut parser = Parser::new();
        parser.set_base_dir(&work);
        let config = parser.parse(&format!("{name}.conf")).unwrap();
        let before = dump(&config);

        // The `-C` option alone starts the check.
        let mut reloader = Reloader::new(config.clone());
        reloader.set_base_dir(&work);
        reloader.set_check_on_change(true);
        match reloader.check() {
            Outcome::Unchanged => {}
            _ => failures.push(format!(
                "{name}: a check without a change reloaded the config"
            )),
        }

        // The flag off and no `SIGHUP`: no check at all.
        let mut reloader = Reloader::new(config.clone());
        reloader.set_base_dir(&work);
        match reloader.check() {
            Outcome::Unchanged => {}
            _ => failures.push(format!("{name}: a check without -C reloaded the config")),
        }

        // The change of the file.
        change(&work, name);
        let expected =
            normalize(fs::read_to_string(dir.join(format!("{name}.changed.expect"))).unwrap());

        // `SIGHUP` forces the reload.
        let mut reloader = Reloader::new(config.clone());
        reloader.set_base_dir(&work);
        reloader.set_forced(true);
        let reloaded = match reloader.check() {
            Outcome::Reloaded(config) => *config,
            Outcome::Unchanged => {
                failures.push(format!("{name}: SIGHUP did not reload the config"));
                continue;
            }
            Outcome::Failed(e) => {
                failures.push(format!("{name}: SIGHUP failed: {e}"));
                continue;
            }
        };
        checks += 1;
        let got = normalize(dump(&reloaded));
        if got != expected {
            failures.push(format!(
                "{name}: the dump of the reloaded config differs from the expectation"
            ));
        }

        // The second check sees no change: the C code stores the new time.
        match reloader.check() {
            Outcome::Unchanged => {}
            _ => failures.push(format!("{name}: the second check reloaded the config")),
        }

        // A broken file keeps the old config.
        let mut reloader = Reloader::new(config.clone());
        reloader.set_base_dir(&work);
        reloader.set_forced(true);
        fs::write(work.join(format!("{name}.conf")), "nosuchkeyword 1\n").unwrap();
        let file = fs::OpenOptions::new()
            .write(true)
            .open(work.join(format!("{name}.conf")))
            .unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(2_000_000_100))
            .unwrap();
        match reloader.check() {
            Outcome::Failed(_) => {}
            _ => failures.push(format!("{name}: a broken config did not fail")),
        }
        if dump(reloader.config()) != before {
            failures.push(format!("{name}: a failed reload changed the config"));
        }

        // The C binary is the oracle: it dumps the same changed file.
        if let Ok(binary) = std::env::var("BINKD_C") {
            // Put the changed variant back.
            change(&work, name);
            let out = Command::new(&binary)
                .arg("-d")
                .arg(format!("{name}.conf"))
                .current_dir(&work)
                .output()
                .expect("cannot run the C binary");
            let c_dump = normalize(String::from_utf8_lossy(&out.stdout).into_owned());
            if c_dump != expected {
                failures.push(format!(
                    "{name}: the dump of the C binary differs from the expectation"
                ));
            }
        }

        let _ = fs::remove_dir_all(&work);
    }

    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n{}",
        failures.len(),
        checks,
        failures.join("\n")
    );
    eprintln!("ok: rbinkd-config reload: {checks} cases passed");
}

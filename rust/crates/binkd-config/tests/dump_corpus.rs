//! The `-d` dump of the config against the golden corpus.
//!
//! The C binary made the expectations: `binkd -d <config>`. The test parses the
//! same config and compares the dump with the expected text, byte for byte. When the
//! `BINKD_C` environment variable names the C binary, the test also runs it and
//! checks that the expectation is current.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use binkd_config::{dump, Parser};

/// Returns the directory of the config corpus.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/golden/config")
}

/// Returns the difference of two dumps, line by line.
fn diff(file: &Path, got: &str, want: &str) -> String {
    let mut report = format!("{}: FAIL\n", file.display());
    let got: Vec<&str> = got.split('\n').collect();
    let want: Vec<&str> = want.split('\n').collect();
    for i in 0..got.len().max(want.len()) {
        let g = got.get(i).copied().unwrap_or("<missing>");
        let w = want.get(i).copied().unwrap_or("<missing>");
        if g != w {
            report.push_str(&format!("  line {}: got \"{g}\", want \"{w}\"\n", i + 1));
        }
    }
    report
}

#[test]
fn dump_corpus() {
    let dir = corpus_dir();
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("cannot read the config corpus")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("conf"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no config corpus cases");

    let mut checks = 0;
    let mut failures = Vec::new();
    for file in &files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let expected = fs::read_to_string(file.with_extension("expect")).unwrap_or_default();

        // The dump of the port.
        let mut parser = Parser::new();
        parser.set_base_dir(&dir);
        let config = match parser.parse(&name) {
            Ok(config) => config,
            Err(e) => {
                failures.push(format!("{}: parse error: {e}", file.display()));
                continue;
            }
        };
        let got = dump(&config);
        checks += 1;
        if got != expected {
            failures.push(diff(file, &got, &expected));
        }

        // The C binary is the oracle: check the expectation when it is here.
        if let Ok(binary) = std::env::var("BINKD_C") {
            let out = Command::new(&binary)
                .arg("-d")
                .arg(&name)
                .current_dir(&dir)
                .output()
                .expect("cannot run the C binary");
            let c_dump = String::from_utf8_lossy(&out.stdout).into_owned();
            if c_dump != expected {
                failures.push(diff(file, &c_dump, &expected));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n{}",
        failures.len(),
        checks,
        failures.join("\n")
    );
    eprintln!("ok: binkd-config dump: {checks} cases passed");
}

#[test]
fn cases_exist() {
    assert!(corpus_dir().join("minimal.conf").is_file());
    assert!(corpus_dir().join("minimal.expect").is_file());
}

//! The runnable example of `doc/perl.md`.
//!
//! The case reads the page, writes the example to a temporary script, and runs
//! the hooks of the script. A change to the page that breaks the example fails
//! the case, so the documentation stays verified.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rbinkd_config::Config;
use rbinkd_perl::{Hooks, LogSink, Options};

/// A log sink that holds the lines of the example.
#[derive(Clone, Default)]
struct RecordingLog(Arc<Mutex<Vec<(i32, String)>>>);

impl RecordingLog {
    /// Returns the lines of the sink.
    fn entries(&self) -> Vec<(i32, String)> {
        self.0.lock().unwrap().clone()
    }
}

impl LogSink for RecordingLog {
    fn log(&self, level: i32, message: &str) {
        self.0.lock().unwrap().push((level, message.to_string()));
    }
}

/// Returns the Perl block of the page.
fn example() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../doc/perl.md");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("cannot read {}: {error}", path.display())
    });
    let mut lines = text.lines();
    let mut blocks = Vec::new();
    while let Some(line) = lines.next() {
        if line.trim() != "```perl" {
            continue;
        }
        let mut block = String::new();
        let mut closed = false;
        for line in lines.by_ref() {
            if line.trim() == "```" {
                closed = true;
                break;
            }
            block.push_str(line);
            block.push('\n');
        }
        assert!(closed, "the Perl block of doc/perl.md is not closed");
        blocks.push(block);
    }
    assert_eq!(
        blocks.len(),
        1,
        "doc/perl.md must hold exactly one Perl example"
    );
    blocks.remove(0)
}

/// Writes the example to a temporary file and returns the directory and path.
fn write_example(block: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "rbinkd-perl-doc-example-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hooks.pl");
    fs::write(&path, block).unwrap();
    (dir, path)
}

#[test]
fn the_doc_example_runs() {
    let (dir, path) = write_example(&example());
    let mut config = Config::new();
    config.sysname = "Doc system".to_string();
    let log = RecordingLog::default();
    let captured = log.clone();
    let options = Options {
        log: Some(Arc::new(log)),
        domains: config.spool.domains.clone(),
        aka: config.spool.aka.clone(),
        sender: None,
        config: Some(config),
    };
    let hooks = Hooks::load_with(&path.display().to_string(), options)
        .expect("the example of doc/perl.md does not load");

    hooks.on_start();
    assert_eq!(
        captured.entries(),
        vec![(3, "example: started as Doc system".to_string())]
    );

    let mut message = b"ready".to_vec();
    let mut level = 3;
    assert!(hooks.on_log(&mut message, &mut level));
    assert_eq!(message, b"example: ready");
    assert_eq!(level, 3);

    let _ = fs::remove_dir_all(dir);
}

//! The differential test of the spool layer against the golden corpus.
//!
//! The upstream C implementation is checked by the C↔Rust interop jobs; the
//! corpus is the Rust value oracle.

use std::io::Cursor;

use binkd_corpus::check;
use binkd_spool::flo::{read_flo_line, trans_flo_line, FloAction, RfRule};

fn run_case(func: &str, args: &[Vec<u8>]) -> Option<Vec<u8>> {
    match func {
        "flo_read" => {
            let mut reader = Cursor::new(args[0].clone());
            let mut out = Vec::new();
            while let Some((action, name)) = read_flo_line(&mut reader) {
                if !out.is_empty() {
                    out.push(b'|');
                }
                out.push(match action {
                    FloAction::Delete => b'd',
                    FloAction::Truncate => b't',
                    FloAction::None => b'-',
                });
                out.push(b':');
                out.extend_from_slice(&name);
            }
            Some(out)
        }
        "flo_trans" => {
            let mut rules = Vec::new();
            let mut i = 1;
            while i + 1 < args.len() {
                rules.push(RfRule {
                    from: String::from_utf8_lossy(&args[i]).into_owned(),
                    to: String::from_utf8_lossy(&args[i + 1]).into_owned(),
                });
                i += 2;
            }
            match trans_flo_line(&args[0], &rules) {
                Some(out) => Some(out),
                None => Some(b"<null>".to_vec()),
            }
        }
        _ => panic!("unknown function {func}"),
    }
}

#[test]
fn corpus() {
    let checks = check("binkd-spool", run_case);
    eprintln!("ok: binkd-spool corpus: {checks} checks passed");
}

#[test]
fn cases_exist() {
    assert!(binkd_corpus::load()
        .iter()
        .any(|c| c.crate_name == "binkd-spool" && c.func == "flo_read"));
}

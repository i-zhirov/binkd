//! Flow file (`.?lo`) handling, the port of `readflo.c`.

use std::io::BufRead;

use rbinkd_ftn::text::ed;

/// A rewrite rule of the `ftrans` directive.
#[derive(Clone, Debug)]
pub struct RfRule {
    pub from: String,
    pub to: String,
}

/// The action on a flow file line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloAction {
    /// No action.
    None,
    /// Delete the file after the session.
    Delete,
    /// Truncate the file after the session.
    Truncate,
}

/// Reads a line from a flow file.
///
/// The function skips empty lines and lines that start with `~`. It
/// returns `None` at the end of the file.
///
/// The C code reads with `fgets()` and splits a longer line into several
/// lines. The port reads a whole line.
pub fn read_flo_line(reader: &mut impl BufRead) -> Option<(FloAction, Vec<u8>)> {
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line).ok()? == 0 {
            return None;
        }
        while line.last().is_some_and(|b| b.is_ascii_whitespace()) {
            line.pop();
        }
        match line.first().copied() {
            None | Some(b'~') => continue,
            Some(b'^') => return Some((FloAction::Delete, line[1..].to_vec())),
            Some(b'#') => return Some((FloAction::Truncate, line[1..].to_vec())),
            _ => return Some((FloAction::None, line)),
        }
    }
}

/// Rewrites a flow file line with the `ftrans` rules.
///
/// Returns `None` when the rules are empty. The C code copies the
/// result into a `MAXPATHLEN` buffer.
pub fn trans_flo_line(s: &[u8], rules: &[RfRule]) -> Option<Vec<u8>> {
    if rules.is_empty() {
        return None;
    }
    let mut buf = s.to_vec();
    for rule in rules {
        buf = ed(
            Some(&buf),
            Some(rule.from.as_bytes()),
            Some(rule.to.as_bytes()),
        );
    }
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_lines() {
        let mut reader = Cursor::new(b"file.pkt\n~ignore\n\n^del.pkt\n#trunc.pkt\n");
        assert_eq!(
            read_flo_line(&mut reader),
            Some((FloAction::None, b"file.pkt".to_vec()))
        );
        assert_eq!(
            read_flo_line(&mut reader),
            Some((FloAction::Delete, b"del.pkt".to_vec()))
        );
        assert_eq!(
            read_flo_line(&mut reader),
            Some((FloAction::Truncate, b"trunc.pkt".to_vec()))
        );
        assert_eq!(read_flo_line(&mut reader), None);
    }

    #[test]
    fn rewrite() {
        let rules = vec![RfRule {
            from: "/old".to_string(),
            to: "/new".to_string(),
        }];
        assert_eq!(
            trans_flo_line(b"/OLD/file.pkt", &rules),
            Some(b"/new/file.pkt".to_vec())
        );
        assert_eq!(trans_flo_line(b"/old/file.pkt", &[]), None);
    }
}

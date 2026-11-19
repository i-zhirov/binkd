//! FTN address handling, the port of `ftnaddr.c`.

use crate::domain::DomainTable;
use crate::pattern::pmatch_ncase;

/// The maximum length of a stringized FTN address.
pub const FTN_ADDR_SZ: usize = 80 + 32;

/// The maximum length of a domain name.
const MAX_DOMAIN: usize = 32;

/// The maximum number of tokens of an address.
const TOKENS: usize = 10;

/// The path separator of the platform.
pub const PATH_SEPARATOR: char = '/';

/// An FTN address.
///
/// A value of -1 in a field means unknown or a wildcard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FtnAddr {
    pub domain: String,
    pub z: i32,
    pub net: i32,
    pub node: i32,
    pub p: i32,
}

impl Default for FtnAddr {
    fn default() -> Self {
        FtnAddr::new()
    }
}

impl FtnAddr {
    /// Creates an address with all fields unknown.
    pub fn new() -> FtnAddr {
        FtnAddr {
            domain: String::new(),
            z: -1,
            net: -1,
            node: -1,
            p: -1,
        }
    }

    /// Parses an address and returns the address, or `None` on a syntax
    /// error.
    pub fn parse(s: &str, domains: &DomainTable) -> Option<FtnAddr> {
        let mut rest = s.as_bytes();
        let mut tokens = [Token::Null; TOKENS];

        for token in tokens.iter_mut() {
            *token = gettoken(&mut rest).ok()?;
        }

        let mut fa = FtnAddr::new();
        let mut i = 0usize;

        if tokens[i].num() && tokens[i + 1].sep() == Some(b':') {
            fa.z = tokens[i].value();
            i += 2;
        }

        if tokens[i].num() && tokens[i + 1].sep() == Some(b'/') {
            fa.net = tokens[i].value();
            i += 1;
        }

        if tokens[i].sep() == Some(b'/') {
            i += 1;
        }

        if tokens[i].num() {
            fa.node = tokens[i].value();
            i += 1;
        } else if tokens[i].sep() != Some(b'.') {
            return None;
        }

        if tokens[i].sep() == Some(b'.') {
            if !tokens[i + 1].num() {
                return None;
            }
            fa.p = tokens[i + 1].value();
            i += 2;
        }

        if tokens[i].sep() == Some(b'@') {
            let Token::Word(word) = tokens[i + 1] else {
                return None;
            };
            fa.domain = match domains.get(&String::from_utf8_lossy(word)) {
                Some(d) => domains.resolved(d).name.clone(),
                None => String::from_utf8_lossy(word).into_owned(),
            };
            i += 2;
        }

        if tokens[i] != Token::Null
            || (fa.node == -1 && fa.p == -1)
            || (!fa.domain.is_empty() && fa.z == -1)
            || (fa.z != -1 && fa.net == -1)
            || (fa.net != -1 && fa.node == -1)
        {
            return None;
        }

        if fa.p == -1 {
            fa.p = 0;
        }

        Some(fa)
    }

    /// Returns the string form of the address.
    ///
    /// `force_point` always prints the point field.
    pub fn format(&self, force_point: bool) -> String {
        let mut out = String::new();
        if self.z != -1 {
            out.push_str(&format!("{}:", self.z));
        }
        if self.net != -1 {
            out.push_str(&format!("{}", self.net));
        }
        if self.node != -1 {
            out.push_str(&format!("/{}", self.node));
        }
        if self.p != 0 || self.node == -1 || force_point {
            out.push_str(&format!(".{}", self.p));
        }
        if !self.domain.is_empty() {
            out.push_str(&format!("@{}", self.domain));
        }
        out
    }

    /// Fills the unknown fields from the main AKA.
    pub fn expand(&mut self, aka: &[FtnAddr], domains: &DomainTable) {
        if self.z == -1 {
            self.z = match domains.get(&self.domain) {
                Some(d) => domains.resolved(d).z[0],
                None => aka.first().map(|a| a.z).unwrap_or(-1),
            };
        }
        if let Some(first) = aka.first() {
            if self.net == -1 {
                self.net = first.net;
            }
            if self.node == -1 {
                self.node = first.node;
            }
        }
        if self.domain.is_empty() {
            self.domain = domains.matched(self.z, aka);
        }
    }

    /// Returns 0 if the addresses are equal.
    ///
    /// The domain compare ignores case.
    pub fn compare(&self, other: &FtnAddr) -> i32 {
        let mut r = compare_case_insensitive(&self.domain, &other.domain);
        if r == 0 {
            r = self.z.wrapping_sub(other.z);
        }
        if r == 0 {
            r = self.net.wrapping_sub(other.net);
        }
        if r == 0 {
            r = self.node.wrapping_sub(other.node);
        }
        if r == 0 {
            r = self.p.wrapping_sub(other.p);
        }
        r
    }

    /// Returns 0 if any address of the array matches the mask.
    pub fn mask_match(mask: &[u8], addresses: &[FtnAddr]) -> i32 {
        for fa in addresses {
            if pmatch_ncase(mask, fa.format(true).as_bytes()) {
                return 0;
            }
        }
        -1
    }

    /// Returns the internet domain of the address.
    ///
    /// The result has the form `p1.f13.n5047.z2.binkp.net.`.
    pub fn to_domain(&self, domains: &DomainTable, root_domain: &str) -> String {
        let mut out = if self.p == 0 {
            format!("f{}.n{}.z{}.", self.node, self.net, self.z)
        } else {
            format!("p{}.f{}.n{}.z{}.", self.p, self.node, self.net, self.z)
        };
        match domains.get(&self.domain).and_then(|d| d.idomain.as_deref()) {
            Some(idomain) => out.push_str(idomain),
            None => out.push_str(root_domain),
        }
        out
    }

    /// Returns the BSO file name of the address, or an empty string when
    /// the domain is unknown.
    pub fn to_filename(&self, domains: &DomainTable) -> String {
        let Some(d) = domains.get(&self.domain) else {
            return String::new();
        };
        let ext = if self.z != d.z[0] {
            format!(".{:03x}", self.z)
        } else {
            String::new()
        };
        let pnt = if self.p != 0 {
            format!(".pnt{}{:08x}", PATH_SEPARATOR, self.p)
        } else {
            String::new()
        };
        format!(
            "{}{}{}{}{}{:04x}{:04x}{}",
            d.path, PATH_SEPARATOR, d.dir, ext, PATH_SEPARATOR, self.net, self.node, pnt
        )
    }
}

fn compare_case_insensitive(a: &str, b: &str) -> i32 {
    let mut ai = a.bytes().map(|c| c.to_ascii_lowercase());
    let mut bi = b.bytes().map(|c| c.to_ascii_lowercase());
    loop {
        match (ai.next(), bi.next()) {
            (Some(x), Some(y)) if x == y => continue,
            (Some(x), Some(y)) => return x as i32 - y as i32,
            (Some(_), None) => return 1,
            (None, Some(_)) => return -1,
            (None, None) => return 0,
        }
    }
}

/// A token of an FTN address.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    /// The end of the string.
    Null,
    /// One of `: / . @`.
    Sep(u8),
    /// A number.
    Num(i32),
    /// A word.
    Word(&'a [u8]),
}

impl Token<'_> {
    fn num(&self) -> bool {
        matches!(self, Token::Num(_))
    }

    fn sep(&self) -> Option<u8> {
        match self {
            Token::Sep(c) => Some(*c),
            _ => None,
        }
    }

    fn value(&self) -> i32 {
        match self {
            Token::Num(n) => *n,
            _ => 0,
        }
    }
}

/// Reads the next token.
///
/// `Err` means an invalid character. `Ok(Token::Null)` means the end of
/// the string.
fn gettoken<'a>(s: &mut &'a [u8]) -> Result<Token<'a>, ()> {
    let Some(&b) = s.first() else {
        return Ok(Token::Null);
    };
    if b == b':' || b == b'/' || b == b'.' || b == b'@' {
        *s = &s[1..];
        Ok(Token::Sep(b))
    } else if b.is_ascii_digit() {
        let mut n: i32 = 0;
        let mut i = 0;
        while i < s.len() && s[i].is_ascii_digit() {
            n = n.wrapping_mul(10).wrapping_add((s[i] - b'0') as i32);
            i += 1;
        }
        *s = &s[i..];
        Ok(Token::Num(n))
    } else if b.is_ascii_alphabetic() {
        let mut i = 0;
        while i < s.len()
            && i < MAX_DOMAIN
            && (s[i] == b'.' || s[i] == b'-' || s[i] == b'_' || s[i].is_ascii_alphanumeric())
        {
            i += 1;
        }
        let word = &s[..i];
        *s = &s[i..];
        Ok(Token::Word(word))
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FtnDomain;

    fn table() -> DomainTable {
        DomainTable::new(vec![
            FtnDomain {
                name: "fidonet".to_string(),
                path: "/tmp/fido".to_string(),
                dir: "outb".to_string(),
                z: [2, -1],
                idomain: None,
                alias4: None,
            },
            FtnDomain {
                name: "aliasnet".to_string(),
                path: "/tmp/fido".to_string(),
                dir: "outb".to_string(),
                z: [-1, -1],
                idomain: None,
                alias4: Some(0),
            },
        ])
    }

    #[test]
    fn parse_and_format() {
        let d = table();
        let fa = FtnAddr::parse("2:5047/13.1@fidonet", &d).unwrap();
        assert_eq!(fa.z, 2);
        assert_eq!(fa.net, 5047);
        assert_eq!(fa.node, 13);
        assert_eq!(fa.p, 1);
        assert_eq!(fa.format(false), "2:5047/13.1@fidonet");
    }

    #[test]
    fn parse_rejects() {
        let d = table();
        assert!(FtnAddr::parse("2:5047", &d).is_none());
        assert!(FtnAddr::parse("@fidonet", &d).is_none());
    }

    #[test]
    fn parse_node_only() {
        let d = table();
        let fa = FtnAddr::parse("5047", &d).unwrap();
        assert_eq!((fa.z, fa.net, fa.node, fa.p), (-1, -1, 5047, 0));
    }

    #[test]
    fn filename() {
        let d = table();
        let mut fa = FtnAddr::parse("2:5047/13", &d).unwrap();
        fa.domain = "fidonet".to_string();
        assert_eq!(fa.to_filename(&d), "/tmp/fido/outb/13b7000d");
    }
}

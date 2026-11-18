//! The differential test against the golden corpus.
//!
//! The upstream C implementation is checked by the C↔Rust interop jobs; the
//! corpus is the Rust value oracle.

use binkd_corpus::{check, encode_field, hex_decode, hex_decode_prefix, hex_encode};
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::cram;
use binkd_ftn::crypt::{decrypt_buf, encrypt_buf, init_keys};
use binkd_ftn::domain::{DomainTable, FtnDomain};
use binkd_ftn::pattern::xpmatch;
use binkd_ftn::pkt::{pkt_getaddr, pkt_setaddr};
use binkd_ftn::text;
use binkd_ftn::text::InbCase;
use binkd_ftn::token::getwordx2;

/// The fixed domain table of the corpus.
fn domains() -> DomainTable {
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
            name: "othernet".to_string(),
            path: "/tmp/other".to_string(),
            dir: "outb".to_string(),
            z: [3, -1],
            idomain: Some("other.net".to_string()),
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

/// The fixed AKA of the corpus.
fn aka() -> Vec<FtnAddr> {
    vec![FtnAddr {
        domain: "fidonet".to_string(),
        z: 2,
        net: 5047,
        node: 13,
        p: 0,
    }]
}

fn int(b: &[u8]) -> i64 {
    std::str::from_utf8(b).unwrap().parse().unwrap()
}

fn text_arg(b: &[u8]) -> &str {
    std::str::from_utf8(b).unwrap()
}

fn is_null(b: &[u8]) -> bool {
    b == b"<null>"
}

fn yes(b: bool) -> Option<Vec<u8>> {
    Some(if b { b"1".to_vec() } else { b"0".to_vec() })
}

fn addr_from_args(args: &[Vec<u8>]) -> FtnAddr {
    let mut fa = FtnAddr::new();
    fa.z = int(&args[0]) as i32;
    fa.net = int(&args[1]) as i32;
    fa.node = int(&args[2]) as i32;
    fa.p = int(&args[3]) as i32;
    if !args[4].is_empty() {
        fa.domain = text_arg(&args[4]).to_string();
    }
    fa
}

fn addr_string(fa: &FtnAddr) -> Vec<u8> {
    format!("{}:{}:{}:{}:{}", fa.z, fa.net, fa.node, fa.p, fa.domain).into_bytes()
}

fn run_case(func: &str, args: &[Vec<u8>]) -> Option<Vec<u8>> {
    match func {
        "pmatch" => yes(xpmatch(&args[0], &args[1], int(&args[2]) != 0)),
        "getword" => {
            let src = if is_null(&args[0]) {
                None
            } else {
                Some(&args[0][..])
            };
            // getwordx2 takes Option<&[u8]>, so the call is below.
            match getwordx2(
                src,
                int(&args[1]) as i32,
                int(&args[2]) as u32,
                &args[3],
                &args[4],
            ) {
                Some(w) => Some(w),
                None => Some(b"<null>".to_vec()),
            }
        }
        "crypt_keys" => {
            let keys = init_keys(&args[0]);
            Some(format!("{:x} {:x} {:x}", keys[0], keys[1], keys[2]).into_bytes())
        }
        "crypt_encrypt" | "crypt_decrypt" => {
            let mut data = hex_decode(&args[1])?;
            let mut keys = init_keys(&args[0]);
            if func == "crypt_encrypt" {
                encrypt_buf(&mut data, &mut keys);
            } else {
                decrypt_buf(&mut data, &mut keys);
            }
            Some(hex_encode(&data).into_bytes())
        }
        "cram_digest" | "cram_digest_hex" => {
            let (key, data) = if func == "cram_digest_hex" {
                (hex_decode(&args[0])?, hex_decode(&args[1])?)
            } else {
                (args[0].clone(), args[1].clone())
            };
            let mut challenge = Vec::with_capacity(data.len() + 1);
            challenge.push(data.len() as u8);
            challenge.extend_from_slice(&data);
            Some(cram::build_digest(&key, &challenge)?.into_bytes())
        }
        "cram_tostring" => {
            let digest = hex_decode_prefix(&args[0], int(&args[1]) as usize)?;
            Some(cram::to_string(&digest).into_bytes())
        }
        "cram_parse" => match cram::parse(text_arg(&args[0])) {
            Some(v) => {
                let mut out = format!("{}:", v[0]).into_bytes();
                out.extend_from_slice(hex_encode(&v[1..]).as_bytes());
                Some(out)
            }
            None => Some(b"<null>".to_vec()),
        },
        "addr_parse" => {
            if is_null(&args[0]) {
                return Some(b"<error>".to_vec());
            }
            match FtnAddr::parse(text_arg(&args[0]), &domains()) {
                Some(fa) => Some(addr_string(&fa)),
                None => Some(b"<error>".to_vec()),
            }
        }
        "addr_format" => {
            let fa = addr_from_args(args);
            Some(fa.format(int(&args[5]) != 0).into_bytes())
        }
        "addr_cmp" => {
            if is_null(&args[0]) && is_null(&args[1]) {
                return Some(b"=".to_vec());
            }
            if is_null(&args[0]) {
                return Some(b"<".to_vec());
            }
            if is_null(&args[1]) {
                return Some(b">".to_vec());
            }
            let a = FtnAddr::parse(text_arg(&args[0]), &domains())?;
            let b = FtnAddr::parse(text_arg(&args[1]), &domains())?;
            let r = a.compare(&b);
            Some(match r.cmp(&0) {
                std::cmp::Ordering::Less => b"<".to_vec(),
                std::cmp::Ordering::Greater => b">".to_vec(),
                std::cmp::Ordering::Equal => b"=".to_vec(),
            })
        }
        "addr_filename" => {
            let fa = addr_from_args(args);
            Some(fa.to_filename(&domains()).into_bytes())
        }
        "addr_domain" => {
            let fa = addr_from_args(args);
            Some(fa.to_domain(&domains(), text_arg(&args[5])).into_bytes())
        }
        "addr_exp" => {
            let mut fa = addr_from_args(args);
            fa.expand(&aka(), &domains());
            Some(addr_string(&fa))
        }
        "addr_get_domain" => {
            let table = domains();
            let d = table.get(text_arg(&args[0]));
            Some(
                d.map(|d| d.name.clone())
                    .unwrap_or_else(|| "<null>".to_string())
                    .into_bytes(),
            )
        }
        "addr_matched_domain" => Some(domains().matched(int(&args[0]) as i32, &aka()).into_bytes()),
        "quote" => Some(text::strquote(&args[1], int(&args[0]) as i32)),
        "dequote" => Some(text::strdequote(&args[0])),
        "wipe" => {
            let mut s = args[0].clone();
            text::strwipe(&mut s);
            Some(s)
        }
        "ed" => {
            let a = if args[1].is_empty() {
                None
            } else {
                Some(&args[1][..])
            };
            Some(text::ed(Some(&args[0]), a, Some(&args[2])))
        }
        "inboundcase" => {
            let mut s = args[1].clone();
            text::makeinboundcase(&mut s, inb_case(int(&args[0])));
            Some(s)
        }
        "safe_atol" => {
            if is_null(&args[0]) {
                return Some(b"<error>".to_vec());
            }
            Some(match text::safe_atol(Some(text_arg(&args[0]))) {
                Ok(v) => v.to_string().into_bytes(),
                Err(_) => b"<error>".to_vec(),
            })
        }
        "last_slash" => Some(
            text::last_slash(&args[0])
                .map(|s| s.to_vec())
                .unwrap_or_else(|| b"<null>".to_vec()),
        ),
        "extract_filename" => Some(text::extract_filename(&args[0]).to_vec()),
        "ispkt" => yes(text::ispkt(&args[0])),
        "isreq" => yes(text::isreq(&args[0])),
        "istic" => yes(text::istic(&args[0])),
        "isarcmail" => yes(text::isarcmail(&args[0])),
        "netname" => Some(text::netname(&args[1], &mut || 0)),
        "tz_off" => Some(text::tz_off(int(&args[0]) as i32).to_string().into_bytes()),
        "pkt_getaddr" => {
            let raw = hex_decode(&args[0])?;
            match pkt_getaddr(&raw) {
                Some(v) => Some(
                    v.iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                        .into_bytes(),
                ),
                None => Some(b"<error>".to_vec()),
            }
        }
        "pkt_setaddr" => {
            let mut raw = hex_decode(&args[0])?;
            let addr = [
                int(&args[1]) as i16,
                int(&args[2]) as i16,
                int(&args[3]) as i16,
                int(&args[4]) as i16,
                int(&args[5]) as i16,
                int(&args[6]) as i16,
                int(&args[7]) as i16,
                int(&args[8]) as i16,
            ];
            if !pkt_setaddr(&mut raw, &addr) {
                return Some(b"<error>".to_vec());
            }
            Some(hex_encode(&raw).into_bytes())
        }
        _ => panic!("unknown function {func}"),
    }
}

fn inb_case(v: i64) -> InbCase {
    match v {
        0 => InbCase::Save,
        1 => InbCase::Upper,
        2 => InbCase::Lower,
        3 => InbCase::Mixed,
        _ => panic!("bad inboundcase style"),
    }
}

#[test]
fn corpus() {
    // The upstream C behavior sets the same values for the %VAR% cases.
    std::env::set_var("BINKD_TEST_VAR", "value");
    std::env::set_var("BINKD_TEST_LOWER", "lower");
    let checks = check("binkd-ftn", run_case);
    eprintln!("ok: binkd-ftn corpus: {checks} checks passed");
}

#[test]
fn the_encoder_matches_the_runner() {
    // The message encoder of the runner escapes a space and a backslash.
    assert_eq!(encode_field(b"a b"), "a\\x20b");
    assert_eq!(encode_field(b"a\\b"), "a\\x5cb");
}

// The shared golden corpus covers the value cases. These tests pin the
// shapes that a table cannot express.
#[test]
fn cases_exist() {
    assert!(binkd_corpus::load()
        .iter()
        .any(|c| c.crate_name == "binkd-ftn" && c.func == "pmatch"));
}

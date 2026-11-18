//! Pure FTN logic of binkd.
//!
//! The crate holds no I/O and no global state. The functions follow the C
//! implementation in the upstream C tree. The tests in `tests/corpus.rs`
//! check the Rust code against the golden corpus.

#![forbid(unsafe_code)]

pub mod addr;
pub mod cram;
pub mod crypt;
pub mod domain;
pub mod flavour;
pub mod md5;
pub mod node;
pub mod pattern;
pub mod pkt;
pub mod text;
pub mod token;

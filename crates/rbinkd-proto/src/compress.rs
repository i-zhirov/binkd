//! The compression of the files of a session, the port of `compress.c`.
//!
//! The C code streams a file through zlib or bzip2 in blocks of `ZBLKSIZE`
//! (1024) bytes. The port uses the `flate2` crate with the pure Rust backend
//! for zlib and the `bzip2` crate for bzip2. Both crates keep the same
//! streaming contract as the C libraries, so a stream of the C side and a stream
//! of the Rust side are interchangeable.

use std::io;

/// The size of the read and the write buffer of a stream, the port of
/// `ZBLKSIZE` of `compress.h`.
pub const BLOCK_SIZE: usize = 1024;

/// The compression method, the port of the `type` argument of `compress.c`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// zlib, the type 1 of `compress.c`.
    Gz,
    /// bzip2, the type 2 of `compress.c`.
    Bz2,
}

impl Method {
    /// The code of the method, the port of the `type` value of `compress.c`.
    pub fn code(self) -> u32 {
        match self {
            Method::Gz => 1,
            Method::Bz2 => 2,
        }
    }

    /// The name of the method in the `M_FILE` extra argument.
    pub fn name(self) -> &'static str {
        match self {
            Method::Gz => "GZ",
            Method::Bz2 => "BZ2",
        }
    }

    /// The method of a code, or `None` for an unknown code.
    pub fn from_code(code: u32) -> Option<Method> {
        match code {
            1 => Some(Method::Gz),
            2 => Some(Method::Bz2),
            _ => None,
        }
    }

    /// The method of a name, or `None` for an unknown name.
    pub fn from_name(name: &str) -> Option<Method> {
        match name {
            "GZ" => Some(Method::Gz),
            "BZ2" => Some(Method::Bz2),
            _ => None,
        }
    }
}

/// A set of the compression methods of a side, the port of the `z_canrecv`
/// and `z_cansend` bit masks of `protocol.c`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Methods(u32);

impl Methods {
    /// The empty set.
    pub const fn none() -> Methods {
        Methods(0)
    }

    /// The set of every method of the build.
    pub fn all() -> Methods {
        Methods(Method::Gz.code() | Method::Bz2.code())
    }

    /// The set with one method.
    pub fn of(method: Method) -> Methods {
        Methods(method.code())
    }

    /// Adds a method to the set.
    pub fn insert(&mut self, method: Method) {
        self.0 |= method.code();
    }

    /// Returns the bit mask of the methods, the port of `z_cansend`.
    pub fn bits(self) -> u32 {
        self.0
    }

    /// Reports whether the set holds the method.
    pub fn contains(self, method: Method) -> bool {
        self.0 & method.code() != 0
    }

    /// Reports whether the set is empty.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The methods of the set in the order of the `M_FILE` extra argument.
    pub fn names(self) -> Vec<&'static str> {
        let mut names = Vec::new();
        for method in [Method::Gz, Method::Bz2] {
            if self.contains(method) {
                names.push(method.name());
            }
        }
        names
    }

    /// The preferred method of the set, the port of the choice of
    /// `z_send_init()` of `protocol.c`: bzip2 before zlib.
    pub fn preferred(self) -> Option<Method> {
        if self.contains(Method::Bz2) {
            Some(Method::Bz2)
        } else if self.contains(Method::Gz) {
            Some(Method::Gz)
        } else {
            None
        }
    }

    /// The preferred method that both sets hold. We do not send a method that
    /// we do not hold.
    pub fn preferred_between(self, other: Methods) -> Option<Method> {
        Methods(self.0 & other.0).preferred()
    }
}

/// One rule of the `zallow` and `zdeny` directives, the port of `struct
/// zrule` of `readcfg.h`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZRule {
    /// The mask of the file names.
    pub mask: String,
    /// `true` for `zallow`, `false` for `zdeny`.
    pub allow: bool,
}

/// Tests the rules, the port of `zrule_test()` of `readcfg.c`.
///
/// The first matching rule decides. No match denies, as the C code does.
pub fn allows(rules: &[ZRule], netname: &str) -> bool {
    for rule in rules {
        if rbinkd_ftn::pattern::pmatch_ncase(rule.mask.as_bytes(), netname.as_bytes()) {
            return rule.allow;
        }
    }
    false
}

/// The result of one compression step, the port of the results of
/// `do_compress()` of `compress.c`.
#[derive(Clone, Debug)]
pub struct Compressed {
    /// The compressed bytes.
    pub data: Vec<u8>,
    /// The number of the consumed input bytes.
    pub consumed: usize,
    /// The compressed stream ended.
    pub ended: bool,
}

/// The result of one decompression step, the port of the results of
/// `do_decompress()` of `compress.c`.
#[derive(Clone, Debug)]
pub struct Decompressed {
    /// The decompressed bytes.
    pub data: Vec<u8>,
    /// The number of the consumed input bytes.
    pub consumed: usize,
    /// The compressed stream ended.
    pub ended: bool,
}

/// A compression stream, the port of the state of `compress_init()` and
/// `compress_deinit()` of `compress.c`.
pub struct Compressor {
    inner: Inner,
}

enum Inner {
    Gz(flate2::Compress),
    Bz2(bzip2::Compress),
}

impl Compressor {
    /// Creates a stream, the port of `compress_init()` of `compress.c`.
    ///
    /// The level 0 selects the default level of zlib. bzip2 always uses the
    /// level 1, as the C code does. The level is limited to the range of the
    /// `zlevel` directive of the C code.
    pub fn new(method: Method, level: i32) -> Compressor {
        let inner = match method {
            Method::Gz => {
                let level = level.clamp(0, 9);
                let level = if level == 0 {
                    flate2::Compression::default()
                } else {
                    flate2::Compression::new(level as u32)
                };
                // The C code calls `deflateInit()`, that is the zlib format
                // with a header.
                Inner::Gz(flate2::Compress::new(level, true))
            }
            Method::Bz2 => {
                // The C code calls `BZ2_bzCompressInit(..., 1, 0, 0)`: the
                // level 1 and the default work factor.
                Inner::Bz2(bzip2::Compress::new(bzip2::Compression::new(1), 0))
            }
        };
        Compressor { inner }
    }

    /// Compresses a block, the port of `do_compress()` of `compress.c`.
    ///
    /// The call writes at most `output_limit` bytes, consumes as much input as
    /// the output holds, and reports whether the stream ended. The `finish` flag
    /// ends the stream, as the last block of the C code does.
    pub fn compress(
        &mut self,
        input: &[u8],
        output_limit: usize,
        finish: bool,
    ) -> Result<Compressed, io::Error> {
        let mut output = vec![0u8; output_limit];
        let (produced, consumed, ended) = match &mut self.inner {
            Inner::Gz(stream) => {
                let before_in = stream.total_in();
                let before_out = stream.total_out();
                let flush = if finish {
                    flate2::FlushCompress::Finish
                } else {
                    flate2::FlushCompress::None
                };
                let status = stream
                    .compress(input, &mut output, flush)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                (
                    (stream.total_out() - before_out) as usize,
                    (stream.total_in() - before_in) as usize,
                    status == flate2::Status::StreamEnd,
                )
            }
            Inner::Bz2(stream) => {
                let before_in = stream.total_in();
                let before_out = stream.total_out();
                let action = if finish {
                    bzip2::Action::Finish
                } else {
                    bzip2::Action::Run
                };
                let status = stream
                    .compress(input, &mut output, action)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                (
                    (stream.total_out() - before_out) as usize,
                    (stream.total_in() - before_in) as usize,
                    status == bzip2::Status::StreamEnd,
                )
            }
        };
        output.truncate(produced);
        Ok(Compressed {
            data: output,
            consumed,
            ended,
        })
    }
}

/// A decompression stream, the port of the state of `decompress_init()` and
/// `decompress_deinit()` of `compress.c`.
pub struct Decompressor {
    inner: InnerDecompress,
}

enum InnerDecompress {
    Gz(flate2::Decompress),
    Bz2(bzip2::Decompress),
}

impl Decompressor {
    /// Creates a stream, the port of `decompress_init()` of `compress.c`.
    pub fn new(method: Method) -> Decompressor {
        let inner = match method {
            // The C code calls `inflateInit()` and `BZ2_bzDecompressInit()`.
            Method::Gz => InnerDecompress::Gz(flate2::Decompress::new(true)),
            Method::Bz2 => InnerDecompress::Bz2(bzip2::Decompress::new(false)),
        };
        Decompressor { inner }
    }

    /// Decompresses a block, the port of `do_decompress()` of `compress.c`.
    ///
    /// The call writes at most `output_limit` bytes and reports whether the
    /// compressed stream ended.
    pub fn decompress(
        &mut self,
        input: &[u8],
        output_limit: usize,
    ) -> Result<Decompressed, io::Error> {
        let mut output = vec![0u8; output_limit];
        let (produced, consumed, ended) = match &mut self.inner {
            InnerDecompress::Gz(stream) => {
                let before_in = stream.total_in();
                let before_out = stream.total_out();
                let status = stream
                    .decompress(input, &mut output, flate2::FlushDecompress::None)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                (
                    (stream.total_out() - before_out) as usize,
                    (stream.total_in() - before_in) as usize,
                    status == flate2::Status::StreamEnd,
                )
            }
            InnerDecompress::Bz2(stream) => {
                let before_in = stream.total_in();
                let before_out = stream.total_out();
                let status = stream
                    .decompress(input, &mut output)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                (
                    (stream.total_out() - before_out) as usize,
                    (stream.total_in() - before_in) as usize,
                    status == bzip2::Status::StreamEnd,
                )
            }
        };
        output.truncate(produced);
        Ok(Decompressed {
            data: output,
            consumed,
            ended,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        (0..5000u32).map(|i| (i * 7919 % 251) as u8).collect()
    }

    /// Compresses the input as `send_block()` of `protocol.c` does: the whole
    /// stream, in frames of at most `limit` bytes.
    fn compress_all(method: Method, input: &[u8], limit: usize) -> Vec<u8> {
        let mut compressor = Compressor::new(method, 0);
        let mut pending = input.to_vec();
        let mut frames = Vec::new();
        let mut frame = Vec::new();
        let mut ended = false;
        while !ended {
            let step = compressor
                .compress(&pending, limit - frame.len(), true)
                .unwrap();
            frame.extend_from_slice(&step.data);
            pending.drain(..step.consumed);
            ended = step.ended;
            if frame.len() == limit || ended {
                frames.extend_from_slice(&frame);
                frame.clear();
            }
        }
        frames
    }

    /// Decompresses the input as `recv_block()` of `protocol.c` does.
    fn decompress_all(method: Method, input: &[u8]) -> Vec<u8> {
        let mut decompressor = Decompressor::new(method);
        let mut output = Vec::new();
        let mut input = input;
        loop {
            let step = decompressor.decompress(input, BLOCK_SIZE).unwrap();
            output.extend_from_slice(&step.data);
            input = &input[step.consumed..];
            if step.ended {
                break;
            }
            if step.consumed == 0 && step.data.is_empty() {
                break;
            }
        }
        output
    }

    #[test]
    fn gz_round_trip() {
        let input = data();
        let packed = compress_all(Method::Gz, &input, 4096);
        // A zlib stream starts with the header of the default window.
        assert_eq!(packed[0] & 0x0f, 8);
        assert_eq!(decompress_all(Method::Gz, &packed), input);
    }

    #[test]
    fn bz2_round_trip() {
        let input = data();
        let packed = compress_all(Method::Bz2, &input, 4096);
        // A bzip2 stream starts with the signature `BZh`.
        assert_eq!(&packed[..3], b"BZh");
        assert_eq!(decompress_all(Method::Bz2, &packed), input);
    }

    #[test]
    fn small_frames() {
        let input = data();
        // The frame limit does not change the stream.
        let small = compress_all(Method::Gz, &input, 64);
        assert!(small.len() > 64);
        assert_eq!(decompress_all(Method::Gz, &small), input);
    }

    #[test]
    fn empty_file() {
        // An empty file is one empty stream, as in the C code.
        let packed = compress_all(Method::Gz, &[], 4096);
        assert_eq!(decompress_all(Method::Gz, &packed), Vec::<u8>::new());
    }

    #[test]
    fn method_codes() {
        assert_eq!(Method::Gz.code(), 1);
        assert_eq!(Method::Bz2.code(), 2);
        assert_eq!(Method::from_code(1), Some(Method::Gz));
        assert_eq!(Method::from_code(2), Some(Method::Bz2));
        assert_eq!(Method::from_code(3), None);
        assert_eq!(Method::from_name("BZ2"), Some(Method::Bz2));
        assert_eq!(Method::from_name("NZ"), None);
    }

    #[test]
    fn method_sets() {
        let mut methods = Methods::none();
        assert!(methods.is_empty());
        assert_eq!(methods.preferred(), None);
        methods.insert(Method::Gz);
        assert!(methods.contains(Method::Gz));
        assert!(!methods.contains(Method::Bz2));
        // bzip2 is preferred over zlib, as in `z_send_init()`.
        assert_eq!(methods.preferred(), Some(Method::Gz));
        methods.insert(Method::Bz2);
        assert_eq!(methods.preferred(), Some(Method::Bz2));
        assert_eq!(methods.names(), vec!["GZ", "BZ2"]);
        assert_eq!(Methods::all().names(), vec!["GZ", "BZ2"]);
    }
}

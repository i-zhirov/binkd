//! The keyword table of `readcfg.c`.
//!
//! The order of the table is the order of the `-d` dump of the C code, so a
//! keyword stays in its place.
//!
//! The C code has one entry per keyword with the callback and the target of the
//! value. The port dispatches on the name and keeps the table for the order and
//! the lookup.

/// The build features that add a keyword to the C table.
///
/// The C code adds a keyword with `#ifdef`. The port keeps the same switch, so the
/// dump of a config matches the dump of the C binary of the same build.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features {
    /// `WITH_ZLIB` or `WITH_BZLIB2`: `zlevel`, `zminsize`, `zallow`, `zdeny`.
    pub zlib: bool,
    /// `HTTPS`: `proxy` and `socks`.
    pub https: bool,
    /// `BW_LIM`: `limit-rate`.
    pub bwlim: bool,
    /// `WITH_PERL`: the `perl-*` keywords.
    pub perl: bool,
    /// `AMIGADOS_4D_OUTBOUND`: `aso`.
    pub amiga: bool,
    /// `ZLIBDL`: `zlib-dll` and `bzlib2-dll`.
    pub zlibdl: bool,
    /// `HAVE_VSYSLOG` and `HAVE_FACILITYNAMES`: `syslog`.
    pub syslog: bool,
}

impl Features {
    /// The features of a UNIX build with zlib, bzip2, and syslog.
    pub fn unix_default() -> Features {
        Features {
            zlib: true,
            syslog: true,
            ..Features::default()
        }
    }

    /// The features of the Windows build: the compression, proxy, and rate
    /// limit keywords, without syslog, `aso`, or the dynamic loaders.
    pub fn windows_default() -> Features {
        Features {
            zlib: true,
            https: true,
            bwlim: true,
            perl: false,
            amiga: false,
            zlibdl: false,
            syslog: false,
        }
    }

    /// The features of the target of the build.
    pub fn platform_default() -> Features {
        if cfg!(windows) {
            Features::windows_default()
        } else {
            Features::unix_default()
        }
    }
}

/// A keyword of the config file.
#[derive(Clone, Copy, Debug)]
pub struct Keyword {
    /// The name.
    pub name: &'static str,
    /// The feature that adds the keyword to the C table.
    pub feature: Option<fn(&Features) -> bool>,
}

const fn always(name: &'static str) -> Keyword {
    Keyword {
        name,
        feature: None,
    }
}

const fn with(name: &'static str, feature: fn(&Features) -> bool) -> Keyword {
    Keyword {
        name,
        feature: Some(feature),
    }
}

fn zlib(f: &Features) -> bool {
    f.zlib
}
fn https(f: &Features) -> bool {
    f.https
}
fn bwlim(f: &Features) -> bool {
    f.bwlim
}
fn perl(f: &Features) -> bool {
    f.perl
}
fn amiga(f: &Features) -> bool {
    f.amiga
}
fn zlibdl(f: &Features) -> bool {
    f.zlibdl
}
fn syslog(f: &Features) -> bool {
    f.syslog
}

/// The keywords of the config file, in the order of the C table.
pub const KEYWORDS: &[Keyword] = &[
    always("include"),
    always("passwords"),
    always("log"),
    always("loglevel"),
    always("conlog"),
    always("binlog"),
    always("fdinhist"),
    always("fdouthist"),
    always("tzoff"),
    always("domain"),
    always("address"),
    always("sysname"),
    always("bindaddr"),
    always("listen"),
    always("sysop"),
    always("location"),
    always("nodeinfo"),
    always("iport"),
    always("oport"),
    always("rescan-delay"),
    always("call-delay"),
    always("timeout"),
    always("oblksize"),
    always("maxservers"),
    always("maxclients"),
    always("inbound"),
    always("inbound-nonsecure"),
    always("temp-inbound"),
    always("node"),
    always("defnode"),
    always("kill-dup-partial-files"),
    always("kill-old-partial-files"),
    always("kill-old-bsy"),
    always("percents"),
    always("minfree"),
    always("minfree-nonsecure"),
    always("flag"),
    always("exec"),
    always("printq"),
    always("try"),
    always("hold"),
    always("hold-skipped"),
    always("backresolv"),
    always("pid-file"),
    always("remove-try-files"),
    with("proxy", https),
    with("socks", https),
    with("syslog", syslog),
    always("ftrans"),
    always("send-if-pwd"),
    always("root-domain"),
    always("prescan"),
    always("connect-timeout"),
    always("filebox"),
    always("brakebox"),
    always("deletebox"),
    always("skip"),
    always("skipmask"),
    always("inboundcase"),
    always("deletedirs"),
    always("overwrite"),
    always("dont-send-empty"),
    always("rename-style"),
    always("share"),
    always("check-pkthdr"),
    with("aso", amiga),
    with("perl-hooks", perl),
    with("perl-dll", perl),
    with("perl-strict", perl),
    with("perl-var", perl),
    always("nolog"),
    always("hide-aka"),
    always("present-aka"),
    with("zlevel", zlib),
    with("zminsize", zlib),
    with("zallow", zlib),
    with("zdeny", zlib),
    with("zlib-dll", zlibdl),
    with("bzlib2-dll", zlibdl),
    with("limit-rate", bwlim),
];

/// Returns true when the build has the keyword. The lookup ignores case.
pub fn is_keyword(name: &str, features: &Features) -> bool {
    position(name, features).is_some()
}

/// Returns the position of a keyword in the table of the build.
pub fn position(name: &str, features: &Features) -> Option<usize> {
    active(features).position(|k| k.name.eq_ignore_ascii_case(name))
}

/// Returns the keywords of the build, in the order of the C table.
pub fn active(features: &Features) -> impl Iterator<Item = &'static Keyword> {
    let features = *features;
    KEYWORDS
        .iter()
        .filter(move |k| k.feature.is_none_or(|f| f(&features)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup() {
        let features = Features::unix_default();
        assert!(is_keyword("INCLUDE", &features));
        assert!(is_keyword("defnode", &features));
        assert!(!is_keyword("nosuchkeyword", &features));
        assert_eq!(position("include", &features), Some(0));

        // The optional keywords follow the build.
        assert!(is_keyword("zlevel", &features));
        assert!(!is_keyword("limit-rate", &features));
        assert!(!is_keyword("perl-hooks", &features));
        let mut all = features;
        all.bwlim = true;
        all.perl = true;
        assert!(is_keyword("limit-rate", &all));
        assert!(is_keyword("perl-hooks", &all));
    }

    #[test]
    fn the_windows_default_has_the_rust_keywords() {
        let features = Features::windows_default();
        for name in [
            "proxy",
            "socks",
            "limit-rate",
            "zlevel",
            "zminsize",
            "zallow",
            "zdeny",
        ] {
            assert!(is_keyword(name, &features), "{name}");
        }
        for name in ["aso", "syslog", "zlib-dll", "bzlib2-dll", "perl-hooks"] {
            assert!(!is_keyword(name, &features), "{name}");
        }
    }

    #[test]
    fn the_platform_default_follows_the_target() {
        let features = Features::platform_default();
        if cfg!(windows) {
            assert_eq!(features, Features::windows_default());
        } else {
            assert_eq!(features, Features::unix_default());
        }
    }
}

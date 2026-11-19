# Compatibility and limitations

rbinkd is a Rust reimplementation of the Binkd Fidonet mailer. On a
supported platform it is a drop-in replacement for the upstream daemon
within the supported feature set.

## Lineage

The upstream project is [pgul/binkd](https://github.com/pgul/binkd). It owns
the C implementation, the legacy platform builds, and the original Binkd
documentation. rbinkd is independent Rust code that follows the upstream
configuration, spool, command-line, and wire behavior.

The upstream C daemon is the compatibility reference. The test suite also
uses it as the interop oracle. The C source is not part of this repository.

## Drop-in replacement

On a supported platform, install `rbinkd` in place of the Binkd executable
and keep the existing configuration and spool:

- `rbinkd` reads `rbinkd.conf` or `binkd.conf` on UNIX and `rbinkd.cfg`
  or `binkd.cfg` on Windows. If the config argument is omitted, it checks
  the new name first.
- It uses the same BSO spool layout, flow files, inbound names, and log
  files.
- It accepts the supported Binkd command-line options.
- It speaks binkp 1.0 and 1.1 and keeps the `binkd` protocol identity.
- The Windows service defaults to `rbinkd-service`. The legacy
  `binkd-service` name remains available with `-S binkd-service`.
- It keeps the supported config keywords and the `-d` dump.

The executable name and the default service name are the visible changes.
The legacy config and service names stay available so that an existing
installation can switch the binary without renaming its files or service.

## Supported platforms

| Platform | Rust build | CI |
| --- | --- | --- |
| Linux | supported | tested |
| macOS | supported | tested |
| Windows NT and later | supported | tested |
| BSD | build-supported | not tested |
| OS/2, DOS, Amiga | upstream C only | not tested |
| Windows 9x/Me | upstream C only | not tested |

The upstream C build remains the choice for the legacy platforms. It is
also the reference build for the interop tests.

## Behavior differences

These differences apply to all supported Rust builds:

- ASO (Amiga-style outbound) is not implemented. rbinkd uses BSO.
- The dynamic zlib and bzip2 loader keywords `zlib-dll` and `bzlib2-dll`
  are not implemented. The Rust build uses built-in compression.
- `RBINKD_LOG` and the legacy `BINKD_LOG` set the log path when the config
  holds no `log` directive. The `log` directive takes precedence.
- The legacy `BINKD_ICON_FILE` environment variable and the tray icon are
  not implemented.
- The message text of a log line can differ from the C daemon. The log line
  format stays compatible.
- The Perl hooks are optional. Build the binary with the `perl` feature to
  include them. A default build rejects the `perl-*` directives. The UNIX
  sample has active `perl-hooks` and `perl-var` lines. See
  [Perl hooks](perl.md) for the details.

The UNIX build:

- Accepts `syslog`.
- Does not enable the `proxy`, `socks`, and `limit-rate` configuration
  keywords. A config that uses them fails to parse on UNIX.

The Windows build:

- Runs as a service and accepts `-t` and `-S`.
- Has no tray icon and no `-T` switch.
- Rejects `syslog`, `aso`, `zlib-dll`, and `bzlib2-dll`.
- Accepts `proxy`, `socks`, and `limit-rate`.
- Uses the long file name for `*N` and skips the same-volume checks by
  default. Build with `--features windows-original` for the original C
  behavior: the short name and the volume checks use raw Windows calls.

### Windows short name and volume checks

The default Windows build answers no short-name lookup for `*N`. The macro
expands to the long file name. The default build also answers no partition
identity, so `check_config()` skips the checks that keep `temp-inbound`,
`inbound`, `inbound-nonsecure`, and node inboxes on the same volume.

Build with `--features windows-original` to use `GetShortPathNameW()` and
`GetFileInformationByHandle()` and restore the upstream C behavior. The
optional `rbinkd-win-ffi` crate is built only with that feature.

The upstream C Windows build also accepts `aso`. The Rust Windows build
does not. The upstream C build has no syslog backend.

## Upstream documentation

Use the upstream manuals for the original C behavior and for the legacy
platforms. The [documentation index](README.md) links to the original
manuals, protocol notes, and release records. This repository does not copy
them.

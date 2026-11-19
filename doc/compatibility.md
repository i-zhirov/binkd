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

- `rbinkd` reads `binkd.conf` on UNIX and `binkd.cfg` on Windows.
- It uses the same BSO spool layout, flow files, inbound names, and log
  files.
- It accepts the supported Binkd command-line options.
- It speaks binkp 1.0 and 1.1 and keeps the `binkd` protocol identity.
- It keeps the `binkd-service` Windows service name.
- It keeps the supported config keywords and the `-d` dump.

The executable name is the main visible difference. Runtime names stay
compatible so that an existing installation can switch the binary without
renaming its files or service.

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
- The legacy `BINKD_LOG` environment variable is not read. Set the log
  path in the configuration file.
- The legacy `BINKD_ICON_FILE` environment variable and the tray icon are
  not implemented.
- The message text of a log line can differ from the C daemon. The log line
  format stays compatible.
- The Perl hooks are optional. Build the binary with the `perl` feature to
  include them.

The UNIX build:

- Accepts `syslog`.
- Does not enable the `proxy`, `socks`, and `limit-rate` configuration
  keywords. A config that uses them fails to parse on UNIX.

The Windows build:

- Runs as a service and accepts `-t` and `-S`.
- Has no tray icon and no `-T` switch.
- Rejects `syslog`, `aso`, `zlib-dll`, and `bzlib2-dll`.
- Accepts `proxy`, `socks`, and `limit-rate`.

The upstream C Windows build also accepts `aso`. The Rust Windows build
does not. The upstream C build has no syslog backend.

## Upstream documentation

Use the upstream manuals for the original C behavior and for the legacy
platforms. The [documentation index](README.md) links to the original
manuals, protocol notes, and release records. This repository does not copy
them.

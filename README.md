# rbinkd

rbinkd is a Rust reimplementation of the Binkd Fidonet mailer. On supported
platforms it is a drop-in replacement for the upstream daemon within the
supported feature set.

[![CI](https://github.com/i-zhirov/rbinkd/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/i-zhirov/rbinkd/actions/workflows/ci.yml)

## Description

rbinkd transfers FTN mail and files over TCP/IP with the binkp protocol.
It is aimed at existing Binkd operators who want to keep their BSO spool,
configuration, and network setup while running a Rust implementation.

On a supported platform, replace the Binkd executable with `rbinkd`. The
daemon reads the same configuration files, accepts the same command-line
options, writes the same spool files, and keeps the `binkd` protocol
identity. Configure and operate it as you would the upstream mailer.

## Features

- binkp server and client modes, with binkp 1.0 and 1.1
- BSO spool handling, flow files, and inbound processing
- CRAM-MD5, CRYPT, GZ, BZ2, NR, and ND modes
- Config parsing, `-d` dumps, and config reload
- SRIF, Perl hooks in an optional build, and binary session logs
- Windows service mode
- Headless operation on UNIX and Windows

## Quick start

Build the release binary:

```sh
cargo build --release -p rbinkd
```

Create a configuration from the sample, or use an existing Binkd config:

```sh
cp binkd.conf binkd.local.conf
```

Start the daemon:

```sh
target/release/rbinkd binkd.local.conf
```

For the client-only or poll modes, see the [operations guide](doc/operations.md)
or `rbinkd -h`.

## Supported platforms

- Linux
- macOS
- Windows NT and later
- BSD is build-supported, but it is not tested in CI.

The upstream C implementation remains the build for OS/2, DOS, Amiga, and
Windows 9x/Me.

## Compatibility and limitations

Keep these differences in mind before you replace a Binkd binary:

- The executable is `rbinkd`, but configuration names, spool files, the
  service name, and the protocol identity remain `binkd`-compatible.
- ASO (Amiga-style outbound) is not implemented. Use BSO.
- The proxy, SOCKS, and rate-limit configuration keywords are available in
  the Windows build only.
- The Windows build has no tray icon and no `-T` switch.
- The Windows build rejects `syslog`, `aso`, `zlib-dll`, and `bzlib2-dll`.
- The dynamic zlib and bzip2 loader keywords are not implemented.
- The Rust build does not read the legacy `BINKD_LOG` environment variable.
  Set the log path in the configuration file.

See the [compatibility guide](doc/compatibility.md) for the full matrix and
the behavior differences.

## Documentation

- [Documentation index](doc/README.md)
- [Compatibility and limitations](doc/compatibility.md)
- [Configuration](doc/configuration.md)
- [Operations](doc/operations.md)
- [Development](doc/development.md)
- [Manual page](rbinkd.8)

## Lineage

rbinkd follows the Binkd mailer and its configuration and wire behavior.
The upstream project is [pgul/binkd](https://github.com/pgul/binkd). It owns
the C implementation, the legacy platform builds, and the original Binkd
documentation.

The Rust implementation is independent code. It uses the upstream C daemon
as the compatibility reference and as the oracle in the interop test suite.

## License

rbinkd is licensed under GPL-2.0-or-later. See [COPYING](COPYING).

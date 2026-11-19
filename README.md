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
daemon reads the same configuration files, accepts the supported
command-line options, writes the same spool files, and keeps the `binkd`
protocol identity. Configure and operate it as you would the upstream mailer.

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

For the original Windows `*N` and volume-check behavior, add
`--features windows-original`.

Create a configuration from the sample, or use an existing Binkd config:

```sh
cp binkd.conf rbinkd.conf
```

Start the daemon:

```sh
target/release/rbinkd rbinkd.conf
```

For the client-only or poll modes, see the [operations guide](doc/operations.md)
or `rbinkd -h`.

## Releases

Tagged releases publish Linux glibc and static musl archives for x86_64,
aarch64, armv5te, armv7, i686, mips, and mipsel; macOS archives for Intel
and Apple Silicon; and a Windows x86_64 archive. Every release includes
`SHA256SUMS`.

Download the archive for your platform from
[GitHub Releases](https://github.com/i-zhirov/rbinkd/releases), unpack it,
and put `rbinkd` on `PATH`. The archive also contains the man page, the
configuration samples, and the license.

Versioning and the tag process are described in
[Releases](doc/release.md).

## Supported platforms

- Linux
- macOS
- Windows NT and later
- BSD is build-supported, but it is not tested in CI.

The upstream C implementation remains the build for OS/2, DOS, Amiga, and
Windows 9x/Me.

## Compatibility and limitations

Keep these differences in mind before you replace a Binkd binary:

- The executable is `rbinkd`. The config reader accepts both `rbinkd.conf`
  and `binkd.conf` (and the `.cfg` forms). If the config argument is omitted,
  it checks the new name first. The Windows service defaults to
  `rbinkd-service`; use `-S binkd-service` for a legacy service. The spool
  files and protocol identity remain `binkd`-compatible.
- ASO (Amiga-style outbound) is not implemented. Use BSO.
- The proxy, SOCKS, and rate-limit configuration keywords are available in
  the Windows build only.
- The Windows build has no tray icon and no `-T` switch.
- The Windows build rejects `syslog`, `aso`, `zlib-dll`, and `bzlib2-dll`.
- The dynamic zlib and bzip2 loader keywords are not implemented.
- The Rust build reads `RBINKD_LOG` and the legacy `BINKD_LOG`. The `log`
  directive in the configuration takes precedence.
- The Perl hooks are optional. A default build rejects the `perl-*`
  directives. See [Perl hooks](doc/perl.md) for the optional build and the
  unsafe boundary.
- The default Windows build uses the long file name for `*N` and skips the
  same-volume checks. Build with `--features windows-original` for the
  original C behavior.

See the [compatibility guide](doc/compatibility.md) for the full matrix and
the behavior differences.

## Documentation

- [Documentation index](doc/README.md)
- [Compatibility and limitations](doc/compatibility.md)
- [Configuration](doc/configuration.md)
- [Operations](doc/operations.md)
- [Development](doc/development.md)
- [Perl hooks](doc/perl.md)
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

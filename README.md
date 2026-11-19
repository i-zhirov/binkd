# rbinkd

rbinkd is a Rust reimplementation of the Binkd Fidonet mailer.

[![CI](https://github.com/i-zhirov/rbinkd/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/i-zhirov/rbinkd/actions/workflows/ci.yml)

It transfers FTN mail and files over TCP/IP with the binkp protocol. It
is an FTN-compatible internet daemon.

## Build

Build the release binary:

```sh
cargo build --release -p rbinkd
```

The binary is `target/release/rbinkd`.

The Windows build supports Windows NT and later, including the service
mode. The tray icon is not supported. Windows 9x/Me is out of scope.

## Run

```sh
target/release/rbinkd binkd.conf
```

Use `binkd.conf` on UNIX or `binkd.cfg` on Windows. The samples are in
the repository.

On Windows, install and control the service with the same binary:

```sh
rbinkd -t install binkd.conf
rbinkd -t start|stop|restart|status|uninstall
```

`-S name` sets the service name.

rbinkd keeps the `binkd.conf` name, the `BINKD_*` variables, the
`binkd` protocol banner, and the `binkd-service` service name. These
names stay compatible with the upstream mailer.

## Documentation

The documentation index is [doc/README.md](doc/README.md). It links to
the Rust documents and to the original upstream documentation.

- [Rust rewrite plan](doc/rust-rewrite-plan.md)
- [Architecture decisions](doc/decisions/)
- [Rust man page](rbinkd.8)
- [Test suite](tests/README.md)

## Port status

- The Rust workspace is the only implementation in this tree.
- The Windows build accepts the proxy, socks, and `limit-rate`
  keywords. It rejects `aso`, `syslog`, `zlib-dll`, and `bzlib2-dll`.
- OS/2, DOS, Amiga, and Windows 9x/Me use the upstream C build.
- Linux, macOS, and Windows are CI-tested. BSD is build-supported but
  not CI-tested. See [the platform decision](doc/decisions/platform-ci.md).

The C implementation is upstream. It is the reference and the oracle of
the interop tests, the record of
[the C cutover](doc/decisions/c-out-of-tree.md).

## Upstream Binkd

The upstream project owns the C implementation, the legacy platform
builds, and the original documentation:

- [Upstream repository](https://github.com/pgul/binkd)
- [Upstream documentation](https://github.com/pgul/binkd/tree/master/doc)

The [documentation index](doc/README.md) links to the individual
upstream manuals, protocol notes, and release records.

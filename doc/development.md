# Development

This guide describes the source tree, the build, and the test process. Use
[AGENTS.md](../AGENTS.md) for the coding rules and the
[test suite guide](../tests/README.md) for the golden corpus and the full
interop procedure.

## Requirements

- Rust 1.85 or later
- A C toolchain only for the optional Perl hooks or for the C interop tests
- Perl development files only for a build with the `perl` feature

## Build

```sh
cargo build --release -p rbinkd
```

The binary is `target/release/rbinkd`.

## Test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --check
```

The test suite uses the golden corpus in `tests/golden/`. The upstream C
daemon is the interop oracle only when the test environment provides it.
See [tests/README.md](../tests/README.md) for the C reference build and the
optional test environments.

The minimum supported Rust version is 1.85:

```sh
cargo +1.85.0 check --workspace
```

## Workspace layout

| Crate | Responsibility |
| --- | --- |
| `rbinkd` | Binary and library entry point: CLI, daemon, signals, service |
| `rbinkd-config` | Configuration model, parser, dump, and reload |
| `rbinkd-ftn` | Pure FTN logic: addresses, patterns, packets, CRAM, ZipCrypto |
| `rbinkd-log` | Text and binary session logs |
| `rbinkd-net` | Listener, caller, proxy, SRV, and network session support |
| `rbinkd-proto` | binkp frames, session state machine, compression |
| `rbinkd-spool` | BSO queue, busy flags, flow files, inbound files |
| `rbinkd-srif` | SRIF request engine |
| `rbinkd-perl` | Optional Perl hooks behind a small C shim |
| `rbinkd-win` | Windows service and file-system FFI |
| `rbinkd-corpus` | Test-only corpus reader |

The binary and the library crate are `rbinkd`. The public API keeps the
command-line, config, and network behavior of the upstream project.

## Runtime design

The daemon owns one runtime and starts one task per session. A session
value holds its own state, transport, queue, and log context. The managers
share no mutable session state.

The layers point downward:

```text
rbinkd
  -> rbinkd-net, rbinkd-proto, rbinkd-config, rbinkd-log, rbinkd-srif
  -> rbinkd-spool
  -> rbinkd-ftn
```

`rbinkd-ftn` is pure and has no I/O. The spool layer owns file-system
effects. The network and protocol layers own the transport and the session.
The binary maps typed errors to exit codes and log output.

The workspace forbids unsafe code. The only exceptions are the Windows FFI
in `rbinkd-win` and the Perl C shim in `rbinkd-perl`.

## Documentation

- [Documentation index](README.md)
- [Compatibility and limitations](compatibility.md)
- [Configuration](configuration.md)
- [Operations](operations.md)
- [Test suite](../tests/README.md)
- [Coding rules](../AGENTS.md)

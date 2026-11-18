# Decision: the platform CI matrix

Status: accepted and implemented. The base C and Rust interop matrix runs on
Linux, macOS, and Windows. BSD is build-supported but not CI-tested. The C
implementation comes from the upstream tree, the record
`doc/decisions/c-out-of-tree.md`.

## Context

The upstream C implementation is the reference and the oracle of the interop
tests. The Rust workspace is the default build. The CI must run the Rust suite
and the C and Rust interop suite on the systems that the port supports, and it
must state the systems that it does not run.

The existing jobs `interop`, `interop-proxy`, `interop-bwlim`, and
`interop-v10` run on Ubuntu. The `macos` and `windows` jobs run the
Rust workspace suite, and the `windows` job runs the parity tests
and the service smoke test. The `interop-macos` and `interop-windows` jobs
build the upstream C reference; the workspace jobs do not.

The repository holds no accepted GitHub runner action for FreeBSD.
A third-party action would add a dependency of the CI on a project
that the team does not control.

## Options

1. Add a FreeBSD job with a third-party runner action. This option
   gives CI coverage of BSD, and it adds an external action and a
   runner that the team does not control.
2. Build the base C and Rust matrix on macOS and Windows, and keep
   BSD build-supported and not CI-tested. This option covers the two
   systems that the team runs, and it states the BSD gap.
3. Add no platform job and keep the C interop on Ubuntu. This
   option leaves the macOS and Windows C builds untested.

## Decision

The team takes option 2. The CI runs the base C and Rust matrix on
Linux, macOS, and Windows. BSD stays build-supported and not
CI-tested until an accepted FreeBSD runner action is available.

The platform matrix:

| Platform | Jobs | C build | Rust suite |
| --- | --- | --- | --- |
| Linux | `rust`, `msrv`, `perl`, `interop`, `interop-proxy`, `interop-bwlim`, `interop-v10` | upstream plain and variants | yes |
| macOS | `macos`, `interop-macos` | upstream UNIX plain | yes |
| Windows | `windows`, `interop-windows` | upstream MinGW plain | yes |
| BSD | none | build-supported | build-supported |
| OS/2, DOS, Amiga, Windows 9x/Me | none | upstream C only | no |

The interop coverage:

| Platform | Job | Base | Binary | Config, CLI, daemon | Proxy | Rate limit | Protocol 1.0 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Linux | `interop` | yes | yes | yes | no | no | no |
| Linux | `interop-proxy` | no | no | no | yes | no | no |
| Linux | `interop-bwlim` | no | no | no | no | yes | no |
| Linux | `interop-v10` | no | no | no | no | no | yes |
| macOS | `interop-macos` | yes | yes | yes | no | no | no |
| Windows | `interop-windows` | yes | yes | no | no | no | no |
| BSD | none | no | no | no | no | no | no |

## The Windows C build

The `interop-windows` job checks out the upstream C reference with
`.github/actions/upstream-c`. It installs `make`,
`mingw-w64-x86_64-gcc`, `mingw-w64-x86_64-binutils`,
`mingw-w64-x86_64-zlib`, and `mingw-w64-x86_64-bzip2` in the MINGW64
environment. In the `upstream-c` directory it builds:

```sh
make -f mkfls/nt95-mingw/Makefile \
  CC=gcc ZLIB=1 BZLIB2=1 \
  OUTDIR=bin/mingw32-zlib-bzlib2 \
  CFLAGS='-std=gnu89 -mthreads -pipe -funsigned-char -Wall -Wno-char-subscripts -Wno-comment -O2 -s' \
  LFLAGS='-static'
```

The command-line `OUTDIR` pins the output path
`bin/mingw32-zlib-bzlib2/binkd-mingw-zlib-bzlib2.exe`; the makefile
would otherwise append `-binkd-zlib-bzlib2`. The command-line
`CFLAGS` replaces the makefile `CFLAGS +=` line, so the obsolete
`-mno-cygwin` flag is not passed. `LFLAGS='-static'` links `libz.a`
and `libbz2.a` into the executable.

`BINKD_C` is the path
`bin\mingw32-zlib-bzlib2\binkd-mingw-zlib-bzlib2.exe` in the `upstream-c`
directory.
The PowerShell test step adds `C:\msys64\mingw64\bin` to `PATH` as
a safety measure for a toolchain DLL.

The C Windows build differs in these points:

- It defines `AMIGADOS_4D_OUTBOUND` and accepts `aso`; the Rust
  Windows build rejects `aso`.
- It has no syslog backend; the Rust Windows build rejects `syslog`.
- It accepts `limit-rate` only with `BW_LIM`; the Rust Windows build
  accepts it.
- It accepts the compression keywords only with `WITH_ZLIB` or
  `WITH_BZLIB2`; the Rust Windows build accepts them.
- It can load zlib and bzip2 dynamically with `ZLIBDL`; the Rust
  port cannot.

The C Windows binary is not the oracle of the config dump, the CLI,
or the daemon. The `interop-windows` job runs only the base
protocol, network, record, and binary tests.

The C Windows binary and the Rust Windows binary log
`BEGIN standalone, binkd/`, the `WIN32` branch of `binkd.c` and the
`#[cfg(windows)]` branch of `daemon.rs`; the UNIX builds of both log
`BEGIN, binkd/`. The binary interop test accepts both forms.

## The special builds

- `interop-proxy` builds the C binary with `--with-proxy --with-ntlm`
  and runs `binkd-net/tests/proxy_interop.rs` with
  `BINKD_C_PROXY`.
- `interop-bwlim` builds the C binary with `--with-bwlim` and runs
  `rust_client_to_c_server_rate_limited` with `BINKD_C_BWLIM`.
- `interop-v10` builds the C binary with `-DPRTCLVER=\"1.0\"` and
  runs `c_client_claims_freq_to_rust_server` and
  `c_client_skips_the_request_of_rust_server` with
  `BINKD_C_V10`.

The macOS and Windows jobs leave those variables unset, so the tests
skip.

## Consequences

- The upstream C reference builds on Linux, macOS, and Windows in CI.
- The base C↔Rust matrix runs on the three systems.
- The proxy, rate-limit, and protocol 1.0 variants stay Ubuntu-only.
- BSD is build-supported and not CI-tested. The README, the tests
  README, `AGENTS.md`, and the rewrite plan state the gap.
- A FreeBSD job needs a new decision and an accepted runner action.

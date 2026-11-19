# AGENTS.md

## Table of Contents

- [Project Overview](#project-overview)
- [Technical Context](#technical-context)
- [Project Structure](#project-structure)
- [Build And Test Commands](#build-and-test-commands)
- [Contribution Instructions](#contribution-instructions)
- [Code Guidelines](#code-guidelines)
    - [System Design](#system-design)
    - [Architecture](#architecture)
    - [Code Quality](#code-quality)
    - [Testing](#testing)
    - [Dependency Management](#dependency-management)
    - [Configuration & Documentation](#configuration--documentation)
    - [Markdown Formatting](#markdown-formatting)
    - [Other](#other)

## Project Overview

rbinkd is a Rust reimplementation of the Binkd Fidonet mailer. It transfers FTN
mail and files between FTN systems over TCP/IP with the binkp protocol. FTN node operators run
it as a daemon next to their BSO spool. The Rust workspace in this repository is the
only implementation in this repository and the default on UNIX and Windows.
The C implementation is upstream in `https://github.com/pgul/binkd`, and this
repository holds no C implementation. The optional Perl FFI shim remains C.
See `doc/compatibility.md` for lineage and platform support.
The upstream C implementation is the reference and the only build of OS/2,
DOS, Amiga, and Windows 9x/Me. The Rust crates are also a library for embedding
in other programs.

## Technical Context

- **Language/Version**: Rust edition 2021 with a minimum supported Rust
  version of 1.85 (`rust-version` in `Cargo.toml`). The upstream C
  implementation is C89 with K&R-style function definitions, and this
  repository does not build it.
- **Primary Dependencies**: Rust: `std` for the pure crates, `tokio`
  for the session and the daemon, `flate2` and `bzip2` for the compression,
  `fs2` for the free space of the binary, `time` for the safe local offset
  of the logs, the `TIME` frame, and `-v`, and `syslog` for the local syslog
  backend of the Unix binary. The optional `binkd-perl` crate holds the Perl C
  shim. The `binkd-win` crate holds the Windows service and file-system FFI
  and `windows-sys`. The upstream C implementation has its own dependencies:
  zlib, bzip2, and Perl (all optional), pthreads, libresolv, and libutil.
- **Storage**: The local file system. The BSO spool directories, the
  `.flo`, `.try`, `.hld`, and `.bsy` files, the inbound directory, the
  binary session logs, and the config file. No database.
- **Testing**: The Rust workspace uses the built-in `#[test]` harness and the
  golden corpus in `tests/golden/`. The corpus is the oracle of the Rust
  implementation. The upstream C implementation is the oracle only in the
  interop jobs.
- **Target Platform**: UNIX (Linux, macOS, BSD) and Windows NT and
  later. The Rust port supports the Windows service mode; the tray
  icon is not supported. The upstream C implementation also targets OS/2,
  DOS, Amiga, and Windows 9x/Me. Linux, macOS, and Windows are CI-tested; BSD
  is build-supported but not CI-tested.
- **Project Type**: Other. A long-running network daemon with a library
  workspace. The Rust workspace is a library workspace with a thin binary.
  The upstream C implementation is a flat program.
- **Performance Goals**: N/A, not documented. The code limits the transfer
  rate with the `limit-rate` directive.
- **Constraints**: The Rust code MUST keep full functional equivalence with
  the upstream C implementation and MUST read the same config files. The
  upstream C implementation is the reference and the oracle of the interop
  tests. The Rust port MUST NOT reproduce undefined behavior (see
  [Architecture](#architecture)). `binkd-ftn` MUST stay pure.
- **Scale/Scope**: One rbinkd instance per FTN node. Up to `maxservers` and
  `maxclients` concurrent sessions, 100 each by default. The config has
  about 100 keywords.

## Project Structure

```text
rbinkd/
├── .github/workflows/ci.yml    # CI: Rust, interop, MSRV, Perl, macOS, Windows
├── .github/actions/upstream-c/ # clone and patch the upstream C reference
├── AGENTS.md                   # this file
├── README.md                   # what rbinkd is, build and install
├── Cargo.toml                  # the workspace, forbids unsafe code
├── Cargo.lock                  # the resolved dependencies
├── crates/                     # the Rust crates of the implementation
│   ├── binkd-ftn/              # pure FTN logic, no I/O, no globals
│   ├── binkd-spool/            # BSO spool handling
│   ├── binkd-config/           # config model and parser
│   ├── binkd-proto/            # binkp frames and the session state machine
│   ├── binkd-log/              # text and binary session logs
│   ├── binkd-net/              # listen sockets and outgoing calls
│   ├── binkd-srif/             # the SRIF interface and its event engine
│   ├── binkd-corpus/           # test-only corpus reader
│   ├── binkd-perl/             # optional Perl hooks (excluded)
│   ├── binkd-win/              # Windows service and file-system FFI
│   └── rbinkd/                 # the binary: CLI, daemon, signals
├── ci/upstream-c.patch         # the interop patch of the upstream C tree
├── doc/                        # user and developer documentation
│   ├── README.md               # documentation index
│   ├── compatibility.md        # drop-in behavior and limitations
│   ├── configuration.md        # configuration guide
│   ├── operations.md           # running and service guide
│   └── development.md          # build, tests, and runtime design
├── tests/                      # Rust tests, corpus, Windows smoke test
│   ├── README.md               # how to run the tests, corpus format
│   ├── golden/                 # corpus cases (Rust reads them)
│   └── windows-smoke.ps1       # Windows service smoke test
```

The upstream C modules are not in this tree. `doc/development.md` describes
the Rust workspace and the runtime design.

## Build And Test Commands

Prerequisites and install steps are in `README.md`. The Rust build is the
default. The C reference is upstream, and the interop tests clone it.

- **Build (Rust, default)**: `cargo build --workspace`
- **Test (Rust)**: `cargo test --workspace`
- **Test the Perl hooks** (needs the Perl C shim):
  `cd crates/binkd-perl && cargo test -- --test-threads=1`
- **Test the Perl hooks of the binary**:
  `cargo test -p rbinkd --features perl -- --test-threads=1`
- **Check the Windows service** (needs the target):
  `cargo check -p binkd-win -p rbinkd --target x86_64-pc-windows-gnu`
- **Test the Windows parity** (needs Windows):
  `cargo test -p binkd-win && cargo test -p binkd-config`
  and the full workspace suite.
- **Test the Windows service** (needs Windows, an administrator prompt, and
  the release binary): `cargo build --release -p rbinkd`, then
  `pwsh -NoProfile -ExecutionPolicy Bypass -File tests/windows-smoke.ps1`.
- **Test the C and Rust interop** (needs the upstream C binary): clone the
  upstream C reference at the pinned commit, apply `ci/upstream-c.patch`,
  build it, set `BINKD_C`, and run the tests that `tests/README.md` names.
  The CI jobs `interop`, `interop-proxy`, `interop-bwlim`, and
  `interop-v10` run the plain, proxy, rate limit, and protocol 1.0
  matrices on Ubuntu. The `interop-macos` job runs the base matrix on
  macOS, and the `interop-windows` job runs the base protocol,
  network, record, and binary matrix on Windows with the MinGW C
  reference. The `.github/actions/upstream-c` action holds the clone and the
  patch steps.
- **Check the minimum supported Rust version** (needs the 1.85 toolchain):
  `cargo +1.85 check --workspace`
- **Lint (Rust)**: `cargo clippy --workspace --all-targets`
- **Format check (Rust)**: `cargo fmt --check`
- **Format fix (Rust)**: `cargo fmt`
- **Build the upstream C reference**: clone
  `https://github.com/pgul/binkd`, check out
  `5a84e8596a96ca8b1f4ec69d9a608cd264aa06ba`, apply `ci/upstream-c.patch`,
  then run `cp mkfls/unix/* .`, `chmod +x mkinstalldirs`, `./configure`,
  and `make` in the clone.
- **Build the upstream C oracle with Perl** (the `BINKD_C_PERL` cases): use the
  upstream C steps with `./configure --with-perl`, then set `BINKD_C_PERL` to
  the path of the built `binkd`.
- **Run (Rust)**: `./target/debug/rbinkd binkd.conf`
- **Version (Rust)**: `./target/debug/rbinkd -v`
- **Check the config (Rust)**: `./target/debug/rbinkd -n binkd.conf`
- **Dump the parsed config (Rust)**: `./target/debug/rbinkd -d binkd.conf`
- **Dump the version and the compile flags (Rust)**:
  `./target/debug/rbinkd -vv`

## Contribution Instructions

- You MUST verify your work with the Rust linter, formatter, and compiler.

  Use the following commands:
    - `cargo build --workspace` to check for type errors
    - `cargo clippy --workspace --all-targets` to run the linter
    - `cargo fmt --check` to check formatting
    - `cargo fmt` to fix formatting issues

- You MUST update the unit tests and the golden corpus for changed code.
  A new or changed behavior of a ported module needs a corpus case in
  `tests/golden/`, because the Rust tests read the corpus.

- You MUST run the tests with `cargo test --workspace` and make
  them pass.

- You MUST keep the Rust implementation equivalent to the upstream C
  implementation. A behavior change needs the same behavior in the upstream C
  implementation and a corpus or interop case that covers it.

- The upstream C release process updates the `HISTORY`, `PATCHLEVEL`, and
  spec files of the upstream C tree. This repository holds no copy of them.

- You MUST NOT add an external Rust dependency without a reason. The
  dependencies are the ones of [Dependency Management](#dependency-management);
  add one only with a reason and an exact version.

- You MUST NOT use a Rust language feature that is newer than the minimum
  supported Rust version. `rust-version` in `Cargo.toml` states it; the
  current value is 1.85. Check a change with
  `cargo +1.85 check --workspace` when the 1.85 toolchain is
  installed.

- When making changes to the project structure, ensure the Project Structure
  section in `AGENTS.md` is updated and remains valid.

- If a prompt asks you to refactor or improve existing code, check if you
  can phrase it as a code guideline. If it is possible, add it to the
  relevant Code Guidelines section in `AGENTS.md`.

- After completing the task, you MUST verify that the code you have written
  follows the Code Guidelines in this file.

- Even when your task is to write or update a document (a plan, a README,
  or any Markdown file) rather than code, you MUST still run
  `cargo fmt --check` and the tests that the document
  describes.

## Code Guidelines

### System Design

Design for a long-running network daemon with an embeddable library
core. The upstream C implementation runs as a daemon; the Rust workspace is a
library with a thin binary.

- The daemon runs as a long-lived process. Clean up sockets, spool
  temporary files, `.bsy` flags, and child processes on shutdown. Do not
  rely on process exit to free them.
- Handle several sessions concurrently. Keep the state of one session in one
  value, not in globals. Do not share mutable state between sessions.
- Persist the queue and the session state in the file system in the BSO
  format. Do not add a database.
- Perform long work (a file transfer, a compression) without blocking the
  other sessions.
- Support a graceful shutdown. Finish or abort a session, remove the `.bsy`
  files, and exit within a few seconds of a signal.
- The library crates are consumed by other code. `binkd-ftn` MUST NOT
  access the file system, the network, or the environment. Keep side
  effects out of the default code path.
- Export a stable public API. Mark internal items as private. Document
  every public item.
- Keep the dependency footprint minimal. Every transitive dependency is a
  burden on the consumers of the library.
- Do not mutate global state (environment variables, signal handlers,
  process-wide flags) in the library. The consumer may embed the library in
  a long-running process next to other code.
- Return typed errors instead of `exit()` or a fatal log message. The binary
  maps an error to the exit code and the log output.

### Architecture

The codebase follows these principles:

- **Separation of Concerns** — each module handles one aspect of the
  mailer: the protocol, the spool, the config, or the platform.
- **Single Responsibility Principle** — every file, function, and type has
  one reason to change. One upstream C file maps to one Rust module.
- **Dependency Direction** — dependencies point downward only, from the
  binary to the pure core. A lower layer never calls a higher one.
- **Explicit Boundaries** — a module interface is intentional. Do not reach
  into the internals of another module.
- **Data Flow Clarity** — data moves from the config and the spool, through
  the session, to the wire and back, in a traceable path.
- **Minimize Coupling, Maximize Cohesion** — a module is self-contained
  and interacts through a narrow interface.
- **Make Invalid States Impossible** — use types and validation. The Rust
  port returns a typed value instead of writing through output pointers.
- **Observability Built-in** — logging and error reporting are
  first-class. Less critical in the pure core, which has no log; the daemon
  layers own the log and the session log.
- **Keep It Boring** — prefer well-understood patterns over clever ones.
  The Rust port follows the upstream C code function by function.

The layers of the Rust target, from top to bottom:

```text
rbinkd (binary: CLI, daemon, signals, config reload)
     ↓
binkd-net, binkd-proto, binkd-config, binkd-log, binkd-srif
     ↓
binkd-spool (queue, busy flags, inbound)
     ↓
binkd-ftn (pure FTN logic: addresses, patterns, CRAM, ZipCrypto)
```

The tests add one layer on the side:

```text
tests (corpus tests in crates/*/tests/)
     ↓
binkd-corpus (test-only corpus reader)
```

Each layer calls the layer below it. No layer calls a layer above it.
`binkd-ftn` is a leaf: it depends on `std` only. Today the workspace has
`binkd-ftn`, `binkd-spool`, `binkd-config`, `binkd-proto`, `binkd-log`,
`binkd-net`, `binkd-srif`, `binkd-corpus`, `binkd-win`, and `rbinkd`; the
optional `binkd-perl` is excluded from the workspace.

The layers of the upstream C implementation, for reference:

| Layer | Responsibility | Examples |
| --- | --- | --- |
| Entry point | CLI, startup order, signals | `binkd.c` |
| Managers | Accept loop, outgoing calls | `server.c`, `client.c` |
| Session | binkp state machine, compression | `protocol.c`, `compress.c` |
| Spool | Queue, busy flags, inbound | `ftnq.c`, `bsy.c`, `inbound.c` |
| Config | Keyword parser and model | `readcfg.c` |
| FTN core | Addresses, domains, packet headers | `ftnaddr.c`, `ftndom.c` |
| Utilities | Strings, match, tokens | `tools.c`, `pmatch.c` |
| Platform | OS abstraction and ports | `sys.h`, `unix/`, `nt/` |

**Known exclusions** (the upstream C code violates the rules above; the Rust
port fixes them):

- `binkd.c` does engine work in `main`: the inetd session, the pid file,
  and the Perl and zlib initialization.
- The C headers have cycles: `protocol.c` ↔ `inbound.c`, `protocol.c` ↔
  `srif.c`, and `ftnq.c` ↔ `ftnnode.c`.
- Process-wide mutable state: `current_config` in `readcfg.c`, `sockfd[]`
  and `ext_rand` in `server.c`, and the flags in `common.h`.
- Platform `#ifdef` blocks inside core modules: `tools.c`, `protocol.c`,
  `ftnq.c`, and `bsy.c`.
- Modules reach into the structs of other modules: `exitproc.c` closes the
  `sockfd[]` array of `server.c`, and `binlog.c` reads `STATE` internals.

### Code Quality

- **Rust style**: the rustfmt defaults. Run `cargo fmt`.
- **Static analysis**: `cargo clippy --workspace --all-targets`
  with the workspace lints in `Cargo.toml`: `unsafe_code = "forbid"`
  and clippy `all = "warn"`. Do not weaken a workspace lint. The exceptions:
  `binkd-perl`, the crate of the FFI of the Perl hooks, holds
  `unsafe_code = "allow"` for the Perl C shim; the
  crate is optional and the default build holds it not, and a build that wants
  the hooks names the feature `perl` of the `rbinkd` crate. The `binkd-win`
  crate holds the second exception for the FFI of the Windows
  service, the FFI exception of `binkd-win`; the crate is a
  target-specific dependency of `rbinkd` and compiles on Windows only.
- **Doc comments**: every public item of a Rust crate has a `///` comment
  and every module a `//!` comment. State the upstream C function that the
  port follows, for example "the port of `getw.c`".
- **Error handling**: a library function returns `Option` or `Result` and
  never calls `panic!`, `exit()`, or a fatal log. `panic!` in a test or for
  an unreachable state is acceptable. The binary maps an error to the exit
  code and the log output.
- **Naming**: Rust uses snake_case functions, PascalCase types, and
  `binkd-*` crate names. Keep the upstream C names in the port-mapping
  comments, so a reader can find the source function.
- **Imports**: Rust uses `use crate::...` and module paths. A test-only
  crate is a `[dev-dependencies]` entry, for example `binkd-corpus`.

### Testing

- The golden corpus lives in `tests/golden/*.tsv`. It is the single source
  of truth for the value cases. The Rust tests read the files. The upstream C
  implementation is the oracle only in the interop jobs, where the
  `.github/actions/upstream-c` action clones and patches it.
- Every new or changed behavior of a ported module needs a corpus case. Do
  not delete a case to make a test pass: a failing case is a behavior change.
- Rust unit tests live next to the code in a `#[cfg(test)] mod tests`
  block. The corpus tests live in `crates/*/tests/corpus.rs`.
- The Rust crates have no mocks. A function under test is pure, and the
  corpus supplies the values.
- A recorded session holds a fixed banner time in the `# time` metadata line,
  so a replay and a recording that sends `TIME` are deterministic.
- The `tz_off` cases of `tests/golden/tools.tsv` cover the truncation toward
  zero of positive and negative seconds values.
- The log sink cases use a fake sink: `a_sink_receives_the_level_and_text`,
  `a_hook_rewrites_the_message_of_the_sink`,
  `a_suppressing_hook_leaves_the_sink_empty`,
  `a_nolog_mask_suppresses_the_sink`, and
  `a_level_above_the_file_level_skips_the_sink` of `binkd-log`. The
  `binkd::syslog` cases cover the facility and the priority mapping. No live
  syslog daemon is needed. The Rust parser and dump cases cover the `syslog`
  keyword. No golden C case parses a `syslog <name>` line; the dump holds only
  the fallback, because the C parser `read_syslog_facility()` has an inverted
  argument check.
- All tests MUST pass before a change is complete:
  `cargo test --workspace`.
- The C and Rust interop suite runs the upstream C binary against the Rust
  session in both roles, and the Rust session against itself. The binary-level
  test `crates/rbinkd/tests/interop_binary.rs` starts the real upstream C and
  Rust binaries in both roles and runs the C server with the C client. The CI
  jobs `interop`, `interop-proxy`, `interop-bwlim`, and `interop-v10` build the
  upstream C reference and set the `BINKD_C*` variables. The tests and the
  build commands are in `tests/README.md`.
- The `interop-macos` job builds the upstream UNIX C reference and runs the
  base protocol, network, record, config, CLI, daemon, and binary
  tests. The `interop-windows` job builds the upstream MinGW C reference and
  runs the base protocol, network, record, and binary tests. The
  config corpus, CLI, and daemon tests are not the Windows C oracle.
  The proxy, rate-limit, and protocol 1.0 variants stay Ubuntu-only.
  BSD is build-supported but not CI-tested; see `doc/compatibility.md`.
- The Windows target-gated tests cover the short-name and volume wrappers,
  the partition provider, the Windows feature set, and the Windows dump. They
  run in the `windows` CI job. The config corpus holds only successful dumps,
  so the partition and missing-inbox rejection paths are unit tests.
- The Windows service smoke test is the script `tests/windows-smoke.ps1`.
  The Windows job of CI runs it after the release build. The script installs a
  temporary service, checks the service states, the process, the busy and pid
  files, and the log, and removes the service and the temporary directory.
- The CI runs the workspace suite on Linux, macOS, and Windows. The
  `interop-macos` and `interop-windows` jobs run the C↔Rust matrix with the
  upstream C reference; BSD is not in CI. The Unix-only signal and shell cases
  carry `#[cfg(unix)]`.
- The corpus cases are text files in the repository, not generated data.
  The format is documented in `tests/README.md`.

### Dependency Management

- **Pin all dependency versions explicitly.** `binkd-proto`, `binkd-net`, and
  `rbinkd` depend on `tokio` with an exact version (`=1.54.0`) and the features
  that each crate needs; `rbinkd` adds the `signal` feature for the handlers of
  the daemon, which `std` cannot install. `binkd-proto` depends on `flate2` with
  the pure Rust backend (`=1.1.10`, `rust_backend`) and on `bzip2` (`=0.6.1`).
  `rbinkd` depends on `fs2` (`=0.4.3`) for the free space of the inbound
  directories, and on `time` (`=0.3.44`) with the `local-offset` feature for
  the safe local offset of the logs, the `TIME` frame, and `-v`; the pure
  crates use `std` only and do not use `time`. `rbinkd` also depends on
  `syslog` (`=7.0.0`) as a Unix-only target dependency for the safe local
  syslog backend, and the Windows build compiles it not. The `binkd-win` crate
  depends on `windows-sys` (`=0.61.2`) with the features of the service
  control manager and `Win32_Storage_FileSystem` for the short-name and volume
  wrappers; it is a target-specific dependency of `rbinkd` and the UNIX build
  does not compile it. Add a dependency only with an exact version.
- **Prefer vanilla solutions.** Use `std` when it solves the problem. The
  Rust port of a C module uses `std` only, apart from the async runtime of
  the session and of the daemon.
- **Reputable sources only.** A dependency MUST be well-established and
  actively maintained.
- **Avoid unpopular libraries.** Do not add a niche or obscure crate.
- **Minimize dependency count.** Justify every addition in the change
  description. Each one adds attack surface and maintenance burden.
- **Use the latest stable version.** Check the registry when you add a
  dependency. Do not copy a version number from memory.

**Rationale**: fewer, well-vetted dependencies reduce the security, the
supply-chain, and the maintenance cost.

**Known exclusions** (the upstream C code; the Rust port replaces them):

- The upstream C code links zlib and bzip2 (`compress.c`) and loads them at run
  time on Windows (`zlibdl.c`).
- The upstream C code links the Perl interpreter (`perlhooks.c`).
- The upstream C code carries vendored OpenSSL code for NTLM (`ntlm/`).

### Configuration & Documentation

- **Runtime configuration**: the config file is the last argument
  (`./rbinkd binkd.conf`). The samples are `binkd.conf` (UNIX) and
  `binkd.cfg` (DOS and Windows). The command line flags are in
  `rbinkd.8`. The Rust build does not read the legacy `BINKD_LOG` or
  `BINKD_ICON_FILE` variables. Set the log path in the config. The tray icon
  is not supported.
- **The documentation index is `doc/README.md`.** It links to the upstream
  manuals and protocol notes. Do not copy upstream documentation into this
  tree. Keep only documentation for the Rust implementation here.
- **The config format is a contract.** A change to a directive needs a
  corpus case in `tests/golden/config/` and a comparison of the `-d` dump with
  the upstream C dump of `binkd -d`.
- **Update the documents that the change touches**: `README.md` for the
  build and the links, `tests/README.md` for the test suite and the corpus
  format, `doc/README.md` for the documentation links, and
  `doc/development.md` for the workspace layout and runtime design.
- **The release process** updates the `HISTORY`, the `PATCHLEVEL` value in
  `Config.h`, and the spec file of the upstream C tree. This repository holds
  no copy of them.
- **No secrets in the repository.** The node passwords live in the
  `passwords` file and the config of the node. Test configs use example
  hosts and example passwords.

### Markdown Formatting

All Markdown files MUST follow these formatting rules:

- **Line length**: Keep lines at most 80 characters, but don't overwrap the
  lines artificially short just to hit the limit, keep them close to 80
  characters where possible. This is not a hard lint gate, but SHOULD be
  followed for readability. Lines inside fenced code blocks are exempt from
  this limit.
- **Unordered lists**: Use dashes (`-`) for bullet points. Indent nested
  list items by 4 spaces.
- **Continuation lines**: When a list item wraps to the next line, align
  the continuation with the first character of the item text, not the list
  marker. This applies to all list types.
- **Emphasis**: Use asterisks (`*`) for emphasis (`*italic*`, `**bold**`).
  Do NOT use underscores.
- **Headings**: Duplicate heading names are allowed only among sibling
  headings (same parent level). Avoid duplicates across different levels.
- **Inline HTML**: Avoid raw HTML in Markdown. The only allowed elements
  are `<a>`, `<p>`, `<details>`, `<summary>`, and `<img>`.
- **Trailing spaces**: Do NOT leave trailing whitespace on any line. Do NOT
  use two-space line breaks; use a blank line instead.
- **Bare URLs**: Bare URLs are permitted and do not need to be wrapped in
  angle brackets.
- **Table formatting**: Align table columns with padding when the table fits
  within 80 characters. If the table exceeds 80 characters, switch to a
  compact format using single spaces only. This applies to the separator row
  as well; write it as `| --- |`, not `|--|`.

**Rationale**: uniform Markdown formatting improves readability for both
humans and AI agents that consume project documentation.

### Other

- **Commit messages**: a short imperative subject line and an optional body
  with the reason. The upstream C release bot writes the `Auto increase
  patchlevel` and `Auto update HISTORY` commits in the upstream tree; do not
  write such a commit here.
- **Port mapping comments**: every ported function states the upstream C source
  function that it follows, for example "the port of `ftnaddr.c`".
- **Log format**: keep the log line format of `vLog()` in the upstream
  `tools.c` stable. Scripts parse the log. The message text is not a contract;
  the `-d` config dump is.

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

binkd is a Fidonet mailer. It transfers FTN mail and files between
FTN systems over TCP/IP with the binkp protocol. FTN node operators run
it as a daemon next to their BSO spool. The Rust workspace in `rust/` is the
default implementation on UNIX and Windows. The C code stays in the tree as the
reference and the build of OS/2, DOS, Amiga, and Windows 9x/Me. The Rust
crates are also a library for embedding in other programs.

## Technical Context

- **Language/Version**: C (C89, K&R-style function definitions) for the
  reference implementation. Rust edition 2021 with a minimum supported Rust
  version of 1.85 (`rust-version` in `rust/Cargo.toml`).
- **Primary Dependencies**: C: zlib, bzip2, and Perl (all optional),
  pthreads, libresolv, and libutil. Rust: `std` for the pure crates, `tokio`
  for the session and the daemon, and `flate2` and `bzip2` for the
  compression. The optional `binkd-perl` crate holds the Perl C shim. The
  `binkd-win` crate holds the Windows service FFI and `windows-sys`.
- **Storage**: The local file system. The BSO spool directories, the
  `.flo`, `.try`, `.hld`, and `.bsy` files, the inbound directory, the
  binary session logs, and the config file. No database.
- **Testing**: A custom C harness (`tests/harness.h`) with the golden
  corpus in `tests/golden/`. The corpus is the shared oracle of the C and
  Rust implementations. Rust uses the built-in `#[test]` harness.
- **Target Platform**: UNIX (Linux, macOS, BSD) and Windows NT and later. The
  Rust port supports the Windows service mode; the tray icon is not supported.
  The C code also targets OS/2, DOS, Amiga, and Windows 9x/Me.
- **Project Type**: Other. A long-running network daemon with a library
  workspace. The C code is a flat program; the Rust workspace is a
  library workspace with a thin binary.
- **Performance Goals**: N/A, not documented. The code limits the transfer
  rate with the `limit-rate` directive.
- **Constraints**: The Rust code MUST keep full functional equivalence with
  the C code and MUST read the same config files. The C code is the reference
  and the oracle of the tests. The Rust port MUST NOT reproduce undefined
  behavior (see [Architecture](#architecture)). `binkd-ftn` MUST stay pure.
- **Scale/Scope**: One binkd instance per FTN node. Up to `maxservers` and
  `maxclients` concurrent sessions, 100 each by default. The config has
  about 100 keywords.

## Project Structure

```text
binkd/
├── .github/workflows/ci.yml    # CI: Rust, C, interop, MSRV, Perl, OSes
├── AGENTS.md                   # this file
├── README.md                   # what binkd is, build and install
├── task.md                     # the Rust rewrite task statement
├── binkd.c                     # C entry point: CLI, startup, signals
├── protocol.c                  # C binkp 1.0/1.1 session state machine
├── server.c, client.c           # C listen sockets and outgoing calls
├── ftnq.c, bsy.c, inbound.c    # C queue, busy flags, inbound files
├── readcfg.c                   # C config parser and reload
├── ftnaddr.c, ftndom.c         # C FTN addresses and domains
├── ftnnode.c                   # C node table
├── tools.c, xalloc.c           # C helpers and allocation wrappers
├── pmatch.c, getw.c            # C wildcard match and tokenizer
├── crypt.c, md5b.c             # C ZipCrypto and MD5
├── compress.c                  # C zlib and bzip2 wrappers
├── perlhooks.c                 # C Perl hook bridge
├── unix/, nt/, os2/, dos/, amiga/  # C platform layers
├── mkfls/                      # per-platform makefiles and configure
├── doc/                        # protocol, config, rewrite, decisions
├── tests/                      # C tests, corpus, Windows smoke test
│   ├── README.md               # how to run the tests, corpus format
│   ├── golden/                 # corpus cases (C and Rust read them)
│   ├── windows-smoke.ps1       # Windows service smoke test
│   ├── test_corpus.c           # the C runner of the corpus
│   └── test_crypt.c, test_md5.c # C invariant tests
└── rust/                       # the Cargo workspace of the rewrite
    ├── Cargo.toml              # workspace, forbids unsafe code
    └── crates/
        ├── binkd-ftn/          # pure FTN logic, no I/O, no globals
        ├── binkd-spool/        # BSO spool handling
        ├── binkd-config/       # config model and parser
        ├── binkd-proto/        # binkp frames and the session state machine
        ├── binkd-log/          # text and binary session logs
        ├── binkd-net/          # listen sockets and outgoing calls
        ├── binkd-srif/         # the SRIF interface and its event engine
        ├── binkd-corpus/       # test-only corpus reader
        ├── binkd-perl/         # optional Perl hooks (excluded)
        ├── binkd-win/          # Windows service FFI (Windows only)
        └── binkd/              # the binary: CLI, daemon, signals
```

The other C files at the root are the remaining modules of the mailer.
`doc/rust-rewrite-plan.md` maps every C module to its Rust crate.

## Build And Test Commands

Prerequisites and install steps are in `README.md`. The Rust build is the
default. The C build is the reference and the oracle of the tests.

- **Build (Rust, default)**: `cd rust && cargo build --workspace`
- **Test (Rust)**: `cd rust && cargo test --workspace`
- **Test the Perl hooks** (needs the Perl C shim):
  `cd rust/crates/binkd-perl && cargo test -- --test-threads=1`
- **Test the Perl hooks of the binary**:
  `cd rust && cargo test -p binkd --features perl -- --test-threads=1`
- **Check the Windows service** (needs the target):
  `cd rust && cargo check -p binkd-win -p binkd --target x86_64-pc-windows-gnu`
- **Test the Windows service** (needs Windows, an administrator prompt, and
  the release binary): `cd rust && cargo build --release -p binkd`, then
  `cd ..` and
  `pwsh -NoProfile -ExecutionPolicy Bypass -File tests/windows-smoke.ps1`.
- **Test the C and Rust interop** (needs the C binary): build the C
  reference, set `BINKD_C`, and run the tests that `tests/README.md` names.
  The CI jobs `interop`, `interop-proxy`, `interop-bwlim`, and `interop-v10`
  run the plain, proxy, rate limit, and protocol 1.0 matrices.
- **Check the minimum supported Rust version** (needs the 1.85 toolchain):
  `cd rust && cargo +1.85 check --workspace`
- **Lint (Rust)**: `cd rust && cargo clippy --workspace --all-targets`
- **Format check (Rust)**: `cd rust && cargo fmt --check`
- **Format fix (Rust)**: `cd rust && cargo fmt`
- **Build (C reference)**:
    - `cp mkfls/unix/* .`
    - `chmod +x mkinstalldirs`
    - `./configure`
    - `make`
- **Build the C oracle with Perl** (the `BINKD_C_PERL` cases): use the C
  reference steps with `./configure --with-perl`, then set `BINKD_C_PERL` to
  the path of the built `binkd`.
- **Test (C)**: `make -C tests`
- **Test (C) with AddressSanitizer**:
  `make -C tests CFLAGS="-O0 -g -fsanitize=address -fno-omit-frame-pointer"`
- **Run (Rust)**: `./rust/target/debug/binkd binkd.conf`
- **Version (Rust)**: `./rust/target/debug/binkd -v`
- **Check the config (Rust)**: `./rust/target/debug/binkd -n binkd.conf`
- **Dump the parsed config (Rust)**: `./rust/target/debug/binkd -d binkd.conf`
- **Dump the version and the compile flags (Rust)**:
  `./rust/target/debug/binkd -vv`
- **Run (C reference)**: `./binkd binkd.conf`

## Contribution Instructions

- You MUST verify your work with the Rust linter, formatter, and compiler.

  Use the following commands:
    - `cd rust && cargo build --workspace` to check for type errors
    - `cd rust && cargo clippy --workspace --all-targets` to run the linter
    - `cd rust && cargo fmt --check` to check formatting
    - `cd rust && cargo fmt` to fix formatting issues

- You MUST update the unit tests and the golden corpus for changed code.
  A new or changed behavior of a ported module needs a corpus case in
  `tests/golden/`, because both the C runner and the Rust tests read the
  same files.

- You MUST run the tests with `make -C tests` and
  `cd rust && cargo test --workspace` and make them pass.

- You MUST keep the C and Rust implementations equivalent. A behavior
  change in one implementation needs the same change in the other and a
  corpus case that covers it.

- You MUST NOT edit `HISTORY`, the `PATCHLEVEL` value in `Config.h`, or
  `mkfls/unix/binkd.spec`. The release process updates them.

- You MUST NOT add an external Rust dependency without a reason. The
  dependencies are the ones of [Dependency Management](#dependency-management);
  add one only with a reason and an exact version.

- You MUST NOT use a Rust language feature that is newer than the minimum
  supported Rust version. `rust-version` in `rust/Cargo.toml` states it; the
  current value is 1.85. Check a change with
  `cd rust && cargo +1.85 check --workspace` when the 1.85 toolchain is
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
  `cd rust && cargo fmt --check` and the tests that the document
  describes.

## Code Guidelines

### System Design

Design for a long-running network daemon with an embeddable library
core. The C code runs as a daemon; the Rust workspace is a library with a
thin binary.

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
  one reason to change. One C file maps to one Rust module.
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
  The Rust port follows the C code function by function.

The layers of the Rust target, from top to bottom:

```text
binkd (binary: CLI, daemon, signals, config reload)
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
`binkd-net`, `binkd-srif`, `binkd-corpus`, `binkd-win`, and `binkd`; the
optional `binkd-perl` is excluded from the workspace.

The layers of the C code, for reference:

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

**Known exclusions** (the C code violates the rules above; the Rust port
fixes them):

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

- **C style**: 2-space indentation, the opening brace on its own line for
  functions and blocks, and a space before the parenthesis in a call
  (`Log (0, ...)`). No C linter or formatter is configured. Do not
  reformat C code that you do not change.
- **Rust style**: the rustfmt defaults. Run `cd rust && cargo fmt`.
- **Static analysis**: `cd rust && cargo clippy --workspace --all-targets`
  with the workspace lints in `rust/Cargo.toml`: `unsafe_code = "forbid"`
  and clippy `all = "warn"`. Do not weaken a workspace lint. The exceptions:
  `binkd-perl`, the crate of the FFI of the Perl hooks, holds
  `unsafe_code = "allow"`, the decision of `doc/decisions/perl-hooks.md`; the
  crate is optional and the default build holds it not, and a build that wants
  the hooks names the feature `perl` of the `binkd` crate. The `binkd-win`
  crate holds the second exception for the FFI of the Windows
  service, the decision of `doc/decisions/windows-support.md`; the crate is a
  target-specific dependency of `binkd` and compiles on Windows only.
- **Doc comments**: every public item of a Rust crate has a `///` comment
  and every module a `//!` comment. State the C function that the port
  follows, for example "the port of `getw.c`".
- **Error handling**: a library function returns `Option` or `Result` and
  never calls `panic!`, `exit()`, or a fatal log. `panic!` in a test or for
  an unreachable state is acceptable. The binary maps an error to the exit
  code and the log output.
- **Naming**: Rust uses snake_case functions, PascalCase types, and
  `binkd-*` crate names. Keep the C names in the port-mapping comments,
  so a reader can find the source function.
- **Imports**: Rust uses `use crate::...` and module paths. A test-only
  crate is a `[dev-dependencies]` entry, for example `binkd-corpus`.

### Testing

- The C tests live in `tests/` as `test_*.c` and link the real modules
  against `tests/stubs.c` and `tests/harness.c`.
- The golden corpus lives in `tests/golden/*.tsv`. It is the single source
  of truth for the value cases. The C runner and the Rust tests read the
  same files, so a difference in one case fails one side.
- Every new or changed behavior of a ported module needs a corpus case. A
  case must pass in both runners. Do not delete a case to make a test pass:
  a failing case is a behavior change.
- Rust unit tests live next to the code in a `#[cfg(test)] mod tests`
  block. The corpus tests live in `crates/*/tests/corpus.rs`.
- The Rust crates have no mocks. A function under test is pure, and the
  corpus supplies the values.
- All tests MUST pass before a change is complete: `make -C tests` and
  `cd rust && cargo test --workspace`.
- The C and Rust interop suite runs the C binary against the Rust session in
  both roles, and the Rust session against itself. The binary-level test
  `crates/binkd/tests/interop_binary.rs` starts the real C and Rust binaries
  in both roles and runs the C server with the C client. The CI jobs
  `interop`, `interop-proxy`, `interop-bwlim`, and `interop-v10` build the C
  reference and set the `BINKD_C*` variables. The tests and the build commands
  are in `tests/README.md`.
- The Windows service smoke test is the script `tests/windows-smoke.ps1`.
  The Windows job of CI runs it after the release build. The script installs a
  temporary service, checks the service states, the process, the busy and pid
  files, and the log, and removes the service and the temporary directory.
- The CI runs the workspace suite on Linux, macOS, and Windows. The Unix-only
  signal and shell cases carry `#[cfg(unix)]`.
- Run the C tests with AddressSanitizer when the change touches memory
  handling: `make -C tests CFLAGS="-O0 -g -fsanitize=address"`.
- The corpus cases are text files in the repository, not generated data.
  The format is documented in `tests/README.md`.

### Dependency Management

- **Pin all dependency versions explicitly.** `binkd-proto`, `binkd-net`, and
  `binkd` depend on `tokio` with an exact version (`=1.54.0`) and the features
  that each crate needs; `binkd` adds the `signal` feature for the handlers of
  the daemon, which `std` cannot install. `binkd-proto` depends on `flate2` with
  the pure Rust backend (`=1.1.10`, `rust_backend`) and on `bzip2` (`=0.6.1`).
  Every other crate uses `std` only. The `binkd-win` crate depends on
  `windows-sys` (`=0.61.2`) with the features of the service control manager;
  it is a target-specific dependency of `binkd` and the UNIX build does not
  compile it. Add a dependency only with an exact version.
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

**Known exclusions** (the C code; the Rust port replaces them in the named
phase of `doc/rust-rewrite-plan.md`):

- The C code links zlib and bzip2 (`compress.c`) and loads them at run
  time on Windows (`zlibdl.c`).
- The C code links the Perl interpreter (`perlhooks.c`).
- The C code carries vendored OpenSSL code for NTLM (`ntlm/`).

### Configuration & Documentation

- **Runtime configuration**: the config file is the last argument
  (`./binkd binkd.conf`). The samples are `binkd.conf` (UNIX) and
  `binkd.cfg` (DOS and Windows). The command line flags are in
  `binkd.txt`. The environment variables are `BINKD_LOG` (the log path)
  and `BINKD_ICON_FILE` (the Windows tray icon).
- **The config format is a contract.** A change to a directive needs a
  corpus case in `tests/golden/config/` and a comparison of the `-d` dump with
  the C dump of `binkd -d`.
- **Update the documents that the change touches**: `README.md` for the
  build and the links, `tests/README.md` for the test suite and the corpus
  format, and `doc/rust-rewrite-plan.md` for the port status and the
  module map.
- **Do not edit `HISTORY`, the `PATCHLEVEL` value in `Config.h`, or
  `mkfls/unix/binkd.spec`.** The release process updates them.
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
  with the reason. The release bot writes the `Auto increase patchlevel` and
  `Auto update HISTORY` commits; do not write such a commit.
- **Port mapping comments**: every ported function states the C source
  function that it follows, for example "the port of `ftnaddr.c`".
- **Log format**: keep the log line format of `vLog()` in `tools.c` stable.
  Scripts parse the log. The message text is not a contract; the `-d` config
  dump is.

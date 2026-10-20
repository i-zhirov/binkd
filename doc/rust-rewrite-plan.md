# Plan: gradual rewrite of binkd in Rust

This document is the action plan for the Rust rewrite of binkd.
It records the current state of the C code, the test strategy, the target
architecture, and the port order.

## 1. Goal and constraints

The task in `task.md` gives four requirements:

1. Cover the C code base with tests as much as possible.
2. Rewrite the code in idiomatic Rust.
3. Keep full functional equivalence.
4. Keep config interoperability.

The task adds two more requirements:

- Replace modules one by one when necessary.
- Make the final implementation usable as a library.

The plan uses a differential test harness to hold the two implementations
together. The C code stays the reference oracle until the Rust code passes
every test. The Rust code replaces the C code module by module.

## 2. Current state

binkd is a Fidonet mailer. It moves FTN mail and files over TCP/IP
with the binkp protocol. The code is C89 with K&R-style function
definitions. The tree has about 27000 lines of C.

### 2.1 Build

The project has no top-level Makefile. The build copies a platform
makefile from `mkfls/` into the source root.

Build on a UNIX system:

```
cp mkfls/unix/{Makefile*,configure*,install-sh,mkinstalldirs} .
chmod +x mkinstalldirs
./configure
make
```

The plan was verified on macOS 14 with clang 21. The build produced a
working `binkd` binary. The `-d` option dumps the parsed config and
works with a small test config.

### 2.2 Module map

The table below lists the modules, their size, and the Rust crate that
replaces each module. The sizes come from `wc -l`.

| C module | Lines | Responsibility | Rust crate |
| --- | --- | --- | --- |
| `pmatch.c` | 143 | Shell-style wildcard match | `binkd-ftn` |
| `getw.c` | 141 | Token parser with quotes and escapes | `binkd-ftn` |
| `xalloc.c` | 61 | Allocation wrappers | `std` |
| `tools.c` | 1007 | Strings, args, paths, packets | `binkd-ftn`, `binkd-log` |
| `ftnaddr.c` | 270 | FTN address parse, format, compare | `binkd-ftn` |
| `ftndom.c` | 56 | Domain table lookup | `binkd-ftn` |
| `crypt.c` | 124 | ZipCrypto stream cipher | `binkd-ftn` |
| `md5b.c` | 494 | MD5, HMAC-MD5, CRAM-MD5 | `binkd-ftn` |
| `readflo.c` | 89 | `.flo` line parse and rewrite | `binkd-spool` |
| `ftnnode.c` | 437 | Node table | `binkd-ftn` |
| `bsy.c` | 237 | `.bsy` and `.csy` flag files | `binkd-spool` |
| `inbound.c` | 615 | Inbound partial files and rename | `binkd-spool` |
| `prothlp.c` | 157 | Netname and kill list helpers | `binkd-proto` |
| `ftnq.c` | 1153 | BSO queue scan, sort, `.try` files | `binkd-spool` |
| `readcfg.c` | 2327 | Config parser and reload | `binkd-config` |
| `protocol.c` | 3473 | binkp 1.0/1.1 session state machine | `binkd-proto` |
| `compress.c` | 238 | zlib and bzip2 wrappers | `binkd-proto` |
| `binlog.c` | 216 | Binary session logs | `binkd-log` |
| `srif.c` | 357 | SRIF event engine | `binkd-srif` |
| `server.c` | 334 | Listen sockets and accept loop | `binkd-net` |
| `client.c` | 719 | Outgoing calls and connect | `binkd-net` |
| `https.c` | 433 | HTTP and SOCKS proxy connect | `binkd-net` |
| `srv_gai.c` | 173 | DNS SRV lookup | `binkd-net` |
| `iptools.c` | 133 | Socket options and port lookup | `binkd-net` |
| `binkd.c` | 750 | CLI, startup order, signals | `binkd` |
| `branch.c` | 158 | Fork or thread per connection | `binkd` |
| `breaksig.c` | 62 | Break handlers | `binkd` |
| `exitproc.c` | 172 | Exit cleanup | `binkd` |
| `setpttl.c` | 26 | Process title | `binkd` |
| `run.c` | 338 | External program start | `binkd` |
| `perlhooks.c` | 2479 | Perl hook interpreter | decision, section 7 |
| `rfc2553.c` | 313 | getaddrinfo fallback | not ported |
| `snprintf.c` | 823 | snprintf fallback | not ported |
| `getopt.c` | 112 | getopt fallback | not ported |
| Platform directories | about 4700 | Platform layers | `binkd` |
| `ntlm/` | 1688 | NTLM proxy auth (OpenSSL derived) | `binkd-net` |

### 2.3 Platform matrix

The C code supports these targets:

- UNIX with autoconf: Linux, macOS, BSD.
- Windows: MinGW and MSVC, 32 and 64 bit.
- OS/2: EMX, Watcom, and IBM C.
- DOS: DJGPP and MS C 6.
- Amiga with ixemul.

The Rust port targets UNIX and Windows first. The plan keeps the C code
for OS/2, DOS, and Amiga until a decision (section 7).

### 2.4 Test state

The repository had no test suite. Only `snprintf.c` has a self-test with
the `TEST_SNPRINTF` define.

The first test step is complete:

- `tests/` holds a unit test harness for the pure C modules.
- 7 test programs cover 488 checks.
- The tests run with `make -C tests`.
- The tests also run with AddressSanitizer.

### 2.5 Observability surfaces

These surfaces are the oracles for the equivalence tests:

- The binkp wire protocol. The documents `doc/binkp10-en.txt` and
  `doc/binkp11-en.txt` define the frames and the session flow.
- The `-d` option. It dumps every parsed config value. The output is a
  byte-comparable contract for the config parser.
- The spool directory. Queue files, `.flo` files, `.try` files, `.bsy`
  files, and inbound names are observable file system effects.
- The log file. The format is stable. The message text is not a contract.
- The `-p`, `-n`, and `-P` modes. They run a poll and exit.

## 3. Success criteria

The rewrite is complete when these conditions hold:

- The Rust `binkd` binary passes every test of the C test suite.
- The Rust code parses every config file of the corpus and the `-d` dump
  is byte-identical to the C dump.
- The Rust code and the C code interoperate in both roles: Rust client
  to C server, and C client to Rust server.
- The Rust code reproduces the spool file effects for the fixtures.
- The `binkd` crate exposes a library API for other programs.
- The C test suite and the differential suite pass in CI.

## 4. Test strategy

The plan uses four test levels. Each level pins one part of the behaviour.

### Level 1: unit tests

Unit tests check single functions.

- The C suite in `tests/` pins the current behaviour of the pure
  modules.
- The Rust crates carry the same tests. The Rust tests must reproduce the
  C results.
- The Rust tests use the golden corpus (level 2) for the shared cases.

### Level 2: golden corpus

The corpus holds the cases in text files. Both implementations read the
same files. A difference in one case fails the test.

Proposed layout:

```
tests/golden/pmatch.tsv
tests/golden/ftnaddr_parse.tsv
tests/golden/ftnaddr_format.tsv
tests/golden/ftnaddr_filename.tsv
tests/golden/getword.tsv
tests/golden/crypt.tsv
tests/golden/cram.tsv
tests/golden/pkt_hdr.tsv
tests/golden/flo.tsv
```

Format: one case per line. Fields use a tab separator. A field with a
tab, a newline, or a backslash uses the same escape form as
`strquote()` and `strdequote()`.

Example for `pmatch.tsv`:

```
pattern	string	ncase	want
*.pkt	12345.pkt	1	1
*.pkt	12345.pkt.bad	1	0
```

The C runner and the Rust runner read the same file. The plan moves the
current inline tables of the C tests into this format in phase 1.

### Level 3: session replay

A recorded session is a byte log of a real binkp session.

- The C `binkd` records the log in a test mode.
- The replay test drives a session over an in-memory duplex stream and
  compares every frame with the log.
- The same log drives the Rust session code.
- Cases cover binkp 1.0, binkp 1.1 multi-batch, CRAM-MD5, CRYPT,
  GZ, BZ2, NR mode, ND mode, EXTCMD, and error paths.

### Level 4: end-to-end interop

The interop matrix runs two real programs on the loopback address.

| Client | Server | Options |
| --- | --- | --- |
| C | C | baseline |
| Rust | C | plain, CRAM, CRYPT, GZ, BZ2 |
| C | Rust | plain, CRAM, CRYPT, GZ, BZ2 |
| Rust | Rust | plain, CRAM, CRYPT, GZ, BZ2 |

Each case uses a fixture spool. The test sends one packet in each
direction and compares the spool directories after the session.

### 4.1 Equivalence policy

The contract covers these items:

- The wire bytes.
- The file system effects.
- The config parse result, including the `-d` dump.
- The command line options and their exit codes.
- The log line format, not the message text.

The rewrite does not reproduce these items:

- Memory-unsafe behaviour, for example a buffer overflow.
- Undefined behaviour.
- The exact log message text, unless a test pins it.

Each intentional difference goes to a decision record in this document
(section 7).

### 4.2 Found behaviour to pin

The tests found these behaviours. The Rust port must reproduce them.

- `parse_ftnaddress("5047")` accepts a node-only address. The result
  has zone `-1`, net `-1`, and node `5047`.
- The `[...]` pattern of `xpmatch()` ignores the case-insensitive flag.
  Only the literal parts use the flag.
- `MD_buildDigest()` reads the challenge length from the first byte.
  The length can be up to 255, not only 16.
- `ftnaddress_to_filename()` uses the hex form `%04x%04x` for the net
  and the node, and `.pnt/%08x` for the point.
- The ZipCrypto key schedule uses `unsigned long` arithmetic. On a 64-bit
  system the `keys[1]` value exceeds 32 bits. The key stream stays the
  same, because the code uses only the low byte of the key shift. An
  independent 32-bit reference gave the same key stream. The Rust port must
  use wrapping `u32` arithmetic.
- `getwordx2()` can under-allocate the buffer for a `%VAR%` expansion.
  The Rust port returns a correct string. This item is an intentional
  difference.

## 5. Target architecture

The Rust code lives in a Cargo workspace in `rust/`. The C code stays in
the root until the cutover. The workspace has one crate per module group
and one binary crate.

```
rust/
  Cargo.toml               # workspace
  crates/
    binkd-ftn/             # pure FTN logic, no I/O, no unsafe
    binkd-config/          # config model and parser
    binkd-spool/           # queue, busy flags, inbound
    binkd-proto/           # binkp session, compression, crypt
    binkd-net/             # listener, caller, proxy, SRV
    binkd-log/             # log and binary log
    binkd-srif/            # SRIF events
    binkd/                 # binary: CLI, daemon, signals
    binkd-ffi/             # test only: C reference bindings
  examples/embed.rs        # library use example
```

### 5.1 Crate rules

- `binkd-ftn` has no I/O and no global state. Every function is pure.
  The crate forbids `unsafe`.
- `binkd-config` returns a `Config` value. It has no global state.
  The parser reports errors with a file name and a line number.
- `binkd-proto` drives a session over `AsyncRead` and `AsyncWrite`.
  The caller supplies the transport. The crate does not open a socket.
- `binkd-spool` uses a small `FileSystem` trait. The tests supply a
  temporary directory or an in-memory file system.
- `binkd-ffi` exists for tests only. It links the C objects and
  exposes them to the differential tests.

### 5.2 Library API

The library form is the main product. The binary is a thin wrapper.

```rust
use binkd::{Config, Engine, FtnAddr, Role};

// Parse a config file.
let cfg = Config::parse_file("binkd.conf")?;

// Run a session over a transport that the caller supplies.
let report = binkd::session::run(stream, Role::Client, &cfg).await?;

// Run the full engine: listener, queue scanner, and caller.
let engine = Engine::new(cfg)?;
engine.serve(listener).await?;
engine.poll(&FtnAddr::parse("2:5047/13")?).await?;
```

Requirements for the library form:

- No global mutable state. The caller creates one `Engine` per instance.
- No process exit on error. Functions return `Result`.
- No signal handlers in the library. The binary installs them.
- Feature flags: `zlib`, `bzip2`, `crypt`, `cram`, `proxy`, `ntlm`,
  `bwlimit`, `perl`, `srif`.

### 5.3 Concurrency model

The C code forks one process per connection, or uses one thread per
connection. The Rust code uses asynchronous tasks on a Tokio runtime.

The difference has observable effects:

- The C fork build isolates a crash in one session. The Rust engine
  catches panics per session, so a panic must not stop the engine.
- The C code counts `n_servers` and `n_clients` with semaphores. The Rust
  code uses an atomic counter and a semaphore.
- The C code removes every `.bsy` file at exit. The Rust code keeps the
  same cleanup at shutdown.
- The C `bsy.c` warns that the behaviour differs between the fork and the
  thread builds. The plan picks the thread-build behaviour as the target.

## 6. Port order

Each phase ends with a working build and a green test suite. The C code
stays the reference until the cutover.

### Phase 0: C test baseline (done)

Deliverables:

- `tests/` harness with 7 test programs and 488 checks.
- `tests/README.md` with the run instructions.

Exit criteria:

- `make -C tests` passes.
- The C build works on UNIX.

### Phase 1: workspace, differential harness, pure core

Deliverables:

- The Cargo workspace in `rust/` with `binkd-ftn` and `binkd-ffi`.
- The golden corpus in `tests/golden/`. The C runner reads the corpus.
- Rust ports of `pmatch.c`, `getw.c`, `crypt.c`, `md5b.c`,
  `ftnaddr.c`, `ftndom.c`, and the pure parts of `tools.c`.
- A differential test that runs both implementations on the corpus.

Exit criteria:

- `cargo test --workspace` passes.
- The differential test covers every corpus case.
- `binkd-ftn` has no `unsafe` code.

### Phase 2: spool layer

Deliverables:

- `binkd-spool` with `flo`, `queue`, `busy`, `inbound`, and the node
  table of `ftnnode.c`.
- Fixture spools under `tests/fixtures/spool/`.
- Differential tests that compare the file system state after each
  operation.

Exit criteria:

- The queue scan, the sort order, the `.try` format, and the `.hld`
  format match the C code.
- The inbound rename, the duplicate check, and the packet header check
  match the C code.

### Phase 3: config

Deliverables:

- `binkd-config` with the full keyword table of `readcfg.c`.
- A config corpus with the sample files and one case per keyword.
- A dump test that compares the `-d` output with the C output.

Exit criteria:

- The `-d` dump is byte-identical for every corpus file.
- Includes, the passwords file, masks, rate rules, and the skip rules
  match the C code.
- The reload on file change works.

### Phase 4: protocol

Deliverables:

- `binkd-proto` with the session state machine, the frame code, the
  compression, and the crypt code.
- Recorded sessions under `tests/sessions/`.
- A replay test and the interop matrix of level 4.

Exit criteria:

- The frames match the recorded sessions byte for byte.
- The interop matrix passes for plain, CRAM, CRYPT, GZ, and BZ2.
- The NR mode, the ND mode, and the multi-batch mode pass.

### Phase 5: network, managers, SRIF, binary log

Deliverables:

- `binkd-net` with the listener, the caller, the proxy code, the SRV
  lookup, and the rate limit.
- `binkd-srif` and `binkd-log`.
- A loopback test that exchanges a packet in each direction.

Exit criteria:

- Two Rust instances exchange a packet.
- A Rust instance and a C instance exchange a packet in both roles.
- The proxy, the SRV lookup, and the rate limit match the C code.

### Phase 6: binary and library

Deliverables:

- The `binkd` crate with the CLI, the startup order, the signals, the
  pid file, the config reload, and the external program start.
- The public library API and an example.
- An integration test with the sample config.

Exit criteria:

- The Rust binary passes every level 4 scenario.
- The example embeds a session in another program.
- The command line options match `binkd.8`.

### Phase 7: Perl hooks and legacy platforms

The `perlhooks.c` module is the largest risk. The plan weighs three
options in a decision record:

1. Link the C `perlhooks.c` into the Rust binary with the C ABI. This
   option keeps the hooks and the hook API. It keeps one C module in the
   tree.
2. Port the hooks to a Rust Perl binding. This option gives one language
   but adds a dependency and porting work.
3. Drop the hooks behind a build flag. This option breaks the config
   compatibility for the hook users.

The plan recommends option 1 for the first release, then option 2 when a
maintainer asks for it.

Legacy platforms: OS/2, DOS, and Amiga. The plan keeps the C code for
these platforms and marks them out of scope for the Rust port. A decision
record must state this.

### Phase 8: cutover

Deliverables:

- The Rust binary is the default build.
- The C code moves to a separate directory or is removed.
- The README, the man page, and the packaging files are updated.
- CI runs both suites.

Exit criteria:

- A release candidate passes the full suite.
- The release notes state the port status and the known differences.

## 7. Decisions and open questions

Each decision gets a record in `doc/decisions/` when the team makes it.
The list below holds the decisions that the plan needs before phase 4.

1. Perl hooks: keep the C module, port the hooks, or drop the hooks.
2. Legacy platforms: keep the C code, or stop support.
3. Log text: is the message text part of the contract?
   The plan assumes no. The log line format is a contract.
4. Error exit codes: the C code calls `exit(1)` on a level 0 log message.
   The library form returns an error. The binary maps the error to the
   exit code.
5. The `-d` dump: the plan treats the dump as a stable contract.
   A future change needs a version note.
6. Windows support: the C code has a service mode and a tray icon. The
   plan keeps these in phase 6.
7. Minimum supported Rust version and the dependency policy.

## 8. Risks

| Risk | Effect | Mitigation |
| --- | --- | --- |
| `protocol.c` has hidden state in globals | A port can change the wire behaviour | Model the session state as one value. Use the replay tests |
| The config parser has 100 keywords and side effects | A port can change the parse result | Use the corpus and the `-d` dump |
| The C code has platform `#ifdef` blocks in the core files | A port can mix platform behaviour | Keep the platform code at the edges |
| The concurrency model changes | `.bsy` files and the exit cleanup can differ | Keep the level 4 tests and the thread-build behaviour |
| Perl hooks need the Perl interpreter | The hooks can break | Decide in phase 7. Keep the C module first |
| The log text changes | Scripts that parse the log can break | Keep the line format. Pin the format with a test |
| The `-d` dump changes | Config tools can break | Compare the dump in phase 3 |
| The port has no user feedback for a long time | The port can drift | Each phase ends with a green suite |

## 9. Verification commands

```
make -C tests                          # C unit tests
make -C tests clean                     # remove the test build
cargo test -p binkd-ftn                 # Rust unit tests
cargo test --workspace                  # Rust and differential tests
```

The interop tests arrive in phase 4. They need the C binary and the Rust
binary. Build the C binary with the procedure in section 2.1. Build the
Rust binary with:

```
cargo build -p binkd
```

## 10. Milestones

The milestones follow the phases. Each milestone is a mergeable state.

1. Milestone 1: phase 0 and phase 1. The Rust pure core matches the C
   code on the corpus.
2. Milestone 2: phase 2 and phase 3. The Rust code reads and writes
   the spool and parses every config.
3. Milestone 3: phase 4. The Rust code runs a binkp session with the C
   code.
4. Milestone 4: phase 5 and phase 6. The Rust binary runs as a daemon
   and as a library.
5. Milestone 5: phase 7 and phase 8. The Rust code is the default. The
   C code is a reference or is gone.

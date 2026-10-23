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

Phase 0 is complete:

- `tests/` holds a unit test harness for the pure C modules.
- The tests run with `make -C tests` and with AddressSanitizer.

Phase 1 is complete:

- `tests/golden/` holds the golden corpus. The C runner in
  `tests/test_corpus.c` checks 223 cases against the C code.
- `rust/` holds the Cargo workspace. The tests check the same corpus
  against the Rust code.
- `rust/crates/binkd-ftn` holds the pure FTN logic: patterns, tokens,
  addresses, domains, CRAM-MD5, ZipCrypto, packet headers, and the pure
  helpers of `tools.c`.
- `rust/crates/binkd-spool` holds the flow file code.
- `rust/crates/binkd-corpus` holds the corpus reader of the tests.

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

Layout:

```
tests/golden/pmatch.tsv     xpmatch
tests/golden/getword.tsv     getwordx2
tests/golden/crypt.tsv       ZipCrypto keys and buffers
tests/golden/cram.tsv        HMAC-MD5 and CRAM-MD5
tests/golden/ftn.tsv         FTN addresses and domains
tests/golden/tools.tsv       the pure helpers of tools.c
tests/golden/pkt.tsv         packet headers
tests/golden/flo.tsv         flow file lines
```

Format: one case per line. A line has the fields `<func> <arg>... =>
<want>`, separated by a tab. A field escapes a tab, a newline, and a
backslash with the `\`, `\t`, `\n`, and `\xHH` forms. The text
`<null>` means a NULL value. A `# crate:` header names the crate that
owns the file.

Example for `pmatch.tsv`:

```
pmatch	*.pkt	12345.pkt	1	=>	1
pmatch	*.pkt	12345.pkt.bad	1	=>	0
```

The C runner in `tests/test_corpus.c` and the Rust tests read the same
files. The runner of the C code rejects a file without a known crate name,
so no case stays unchecked.

The corpus is the differential oracle. An in-process binding to the C code
(`binkd-ffi`) is not necessary for the corpus tests. The plan adds
`binkd-ffi` in phase 4 for randomized differential tests.

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
| Rust | C | plain, CRAM, CRYPT, GZ, BZ2, NR, ND |
| C | Rust | plain, CRAM, CRYPT, GZ, BZ2, NR, ND |
| Rust | Rust | plain, CRAM, CRYPT, GZ, BZ2, NR, ND |

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
- `MD_getChallenge()` writes one byte past the buffer for an odd number of
  hex digits. The Rust port takes the full pairs. The corpus has no such
  case.
- `getwordx2()` does not stop for `n <= 0`, and it reads past the end of
  the source for a `%VAR` without the closing `%`. The Rust port returns
  `None` in both cases.
- `trans_flo_line()` calls `strnzcpy()` with the same buffer when a rule
  has an empty `from` field. The C code overlaps the copy. The Rust port
  returns a new value.
- `ed()` can read the result of a rule with an empty `from` field as the
  source and the destination of the copy. The Rust port returns a new
  value.
- `tz_off()` computes the local time zone when the value is -1. The Rust
  port of phase 1 returns the explicit value only. The platform layer of
  phase 6 computes the local value.
- `read_flo_line()` splits a line longer than `MAXPATHLEN` into several
  lines with `fgets()`. The Rust port reads a whole line.

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
    binkd-spool/           # queue, flow files, try files, busy flags
    binkd-proto/           # binkp session, compression, crypt
    binkd-net/             # listener, caller, proxy, SRV
    binkd-log/             # log and binary log
    binkd-srif/            # SRIF events
    binkd/                 # binary: CLI, daemon, signals
    binkd-corpus/           # test only: the golden corpus reader
    binkd-ffi/              # test only: C reference bindings
  examples/embed.rs        # library use example
```

`binkd-ftn`, `binkd-spool`, and `binkd-corpus` exist. The other crates
arrive in the later phases.

### 5.1 Crate rules

- `binkd-ftn` has no I/O and no global state. Every function is pure.
  The crate forbids `unsafe`.
- `binkd-config` returns a `Config` value. It has no global state.
  The parser reports errors with a file name and a line number.
- `binkd-proto` drives a session over `AsyncRead` and `AsyncWrite`.
  The caller supplies the transport. The crate does not open a socket.
- `binkd-spool` uses `std::fs`. The tests run on a temporary directory
  and compare the file system state. An in-memory file system would hide the
  behavior of the real code.
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

### Phase 1: workspace, differential harness, pure core (done)

Deliverables:

- The Cargo workspace in `rust/` with `binkd-ftn`, `binkd-spool`, and
  `binkd-corpus`.
- The golden corpus in `tests/golden/`. The C runner reads the corpus.
- Rust ports of `pmatch.c`, `getw.c`, `crypt.c`, `md5b.c`,
  `ftnaddr.c`, `ftndom.c`, `readflo.c`, and the pure parts of `tools.c`.
- A differential test that runs the Rust code on the corpus.

The tests cover 215 cases of `binkd-ftn` and 8 cases of `binkd-spool`.
The C runner covers the same 223 cases.

Exit criteria:

- `cargo test --workspace` passes.
- The differential test covers every corpus case.
- `binkd-ftn` has no `unsafe` code.

The value cases moved from the inline tables of the C tests to the corpus,
so the corpus is the only source of the cases.

### Phase 2: spool layer (done)

Deliverables:

- `binkd-spool` with `flo`, `queue`, `busy`, `inbound`, and the node
  table of `ftnnode.c`.
- The spool corpus in `tests/golden/spool/`.
- Differential tests that compare the file system state after each
  operation.

The port holds:

- `binkd-ftn::node` holds the node table of `ftnnode.c`. The `defnode`
  DNS resolution stays out; it belongs to the network layer.
- `binkd-spool::queue` holds the queue scan of `ftnq.c`: the domain and
  zone directories, the point directories, the fileboxes, the sort, the
  sizes, and the node selection.
- `binkd-spool::tryfile` holds the `.try` files, the `.hld` files, the
  old `.bsy` files, and the poll files.
- `binkd-spool::busy` holds the `.bsy` and `.csy` flags of the busy
  list, and `binkd-spool::inbound` holds the partial files, the rename,
  and the duplicate check of `inbound.c`.
- `tests/golden/spool/` holds 12 cases. The C runner in
  `tests/test_spool.c` and the Rust test in
  `crates/binkd-spool/tests/spool_corpus.rs` read the same files.

The C runner passed a relative temporary name to `check_pkthdr()`, so the C
code could not open the packet file. The runner now passes the absolute path,
and the corpus covers the good and the bad packet addresses.

The session counters and the SRIF events of `inb_done()` stay with the
protocol layer. The `defnode` DNS resolution of `get_node_info()` stays with
the network layer.

Exit criteria:

- The queue scan, the sort order, the `.try` format, and the `.hld`
  format match the C code. (done)
- The inbound rename, the duplicate check, and the packet header check
  match the C code. (done)

The queue order of two entries with the same sort key follows the
`readdir()` order in both implementations. The corpus avoids such entries,
because the order is not the same on every file system.

### Phase 3: config (done)

Deliverables:

- `binkd-config` with the full keyword table of `readcfg.c`. (done)
- A config corpus with the sample files and one case per keyword. (done)
- A dump test that compares the `-d` output with the C output. (done)

The port holds:

- `binkd-config::model` holds the `Config` value: the lists and the scalars
  of `BINKD_CONFIG`, with the spool part of `binkd-spool` for the fields that
  the spool layer uses.
- `binkd-config::keywords` holds the keyword table in the order of the C table.
- `binkd-config::parser` holds the line parser of `readcfg0()`, the `include`
  nesting, the `read_*` callbacks of the keyword table, the `passwords` file,
  and the checks of `readcfg()`.
- `binkd-config::port` holds the port names; the platform layer supplies the
  full service table.
- `binkd-config::dump` holds the `-d` dump of `debug_readcfg()`.
- `tests/golden/config/` holds 5 cases. The dump of the port matches the output
  of the C binary byte for byte.

- `binkd-config::reload` holds `checkcfg()`: the `-C` option, the `SIGHUP`
  flag, the time of every config file, the parse of the first file, and the old
  config when the new one is broken.

The C `read_syslog_facility()` inverts its argument check, so a `syslog <name>`
line fails the C parser. The port accepts the line; the corpus has no such line.

Exit criteria:

- The `-d` dump is byte-identical for every corpus file. (done)
- Includes, the passwords file, masks, rate rules, and the skip rules
  match the C code. (done)
- The reload on file change works. (done)

### Phase 4: protocol

Deliverables:

- `binkd-proto` with the session state machine, the frame code, the
  compression, and the crypt code.
- The NR mode and the ND mode: the `M_GET` and `M_SKIP` frames, the
  `sent_fls` and `GET_FILE_balance` bookkeeping, the `.stc` status files,
  and the deferred rename of the receiver.
- Recorded sessions under `tests/sessions/`.
- A replay test and the interop matrix of level 4.

Progress:

- `binkd-proto::frame` holds the frames of `protocol.c`: the `M_*` commands,
  the data and the command frames, and the reader and the writer. (done)
- `binkd-proto::session` holds the session state machine of `protocol.c` over
  `AsyncRead` and `AsyncWrite`: the banner, `ADR`, `PWD`, the login, the file
  transfer in both directions, `GOT`, and `EOB`. (done)
- The in-memory test `crates/binkd-proto/tests/session.rs` runs the two ends of
  a session against each other: plain, password, bad password, and a file in both
  directions. (done)
- The interop test `crates/binkd-proto/tests/interop.rs` runs the two directions
  of the matrix over the loopback address: the Rust client against the C server,
  where the C server authenticates the session and writes the file into its inbound
  directory, and the C client against the Rust server, where the C binary creates
  the poll with `-P <node>` and runs it with `-p` in one call. (done)
- `binkd-proto::compress` holds the compression of `compress.c`: zlib and bzip2
  as streams, the block size of the C code, and the `zallow` and `zdeny` rules.
  The session advertises the methods in its `OPT` message and applies them per
  file. (done)
- The interop test covers the compression in both directions: the Rust client
  sends a zlib stream and a bzip2 stream to the C server, and the C client sends
  a zlib stream and a bzip2 stream to the Rust server. (done)
- The session waits for the acknowledgement of every file that it sent, the port of
  the `sent_fls` condition of the end of `protocol()`. Without the wait a caller
  closes the transport before the answerer sends the `M_GOT` frame, so the recording
  of the answerer lost the frame. (done)
- The session holds the `CRYPT` mode of `protocol.c`: the challenge of CRAM-MD5
  that the answerer sends, the digest that the caller answers, the `OPT CRYPT`
  of both sides, and the `YES_CRYPT` state of `complete_login()`. The port crypts
  every frame, the header included, with the ZipCrypto keys of `crypt.c`. (done)
- The interop test covers the mode in both directions: the Rust client crypts a
  session for the C server, and the C client crypts a session for the Rust server.
  The log of the C binary reports `session in CRYPT mode` in both cases. (done)
- `binkd-proto::record` holds the recording and the replay of a session: a
  recording keeps the bytes of the two directions of one session in a text file,
  `Recorder` wraps a transport and keeps them, and `Replay` plays the bytes of the
  peer and keeps the bytes of our side. The recordings of the Rust session are in
  `tests/sessions/` and the replay test compares every byte. (done)
- The replay of a live session of the C binary passes. The C client abandons a
  session when the answerer does not hold the AKA that the caller calls
  (`remote has no such AKA`), so the recording of the answerer must hold the
  address of the caller. (done)
- The NR mode and the ND mode are open. Step 4 is done: the spool holds the
  link status files of the ND mode, `binkd-spool::status`, the port of
  `ND_set_status()`: `status_path()` gives `<outbound>/<address>.stc`,
  `set_status()` writes the name, the size, and the time of the file that the
  peer received and removes the file for an empty status, and the outbound
  scanner of `binkd-spool::queue` already reads the file as the queue type
  `s` with the flavour `h`. The session announces such an entry with the
  offset equal to the size, and it saves the new status from `M_GOT` or clears
  it for a request of the NR mode, the port of `GOT()`. A file that the peer
  asks for returns to the queue of the sent files after the resend, the port of
  `current_file_was_sent()`: without the return the wait of the sender ends
  before the acknowledgement arrives. Step 3 is done: an ND sender
  waits for the acknowledgement of a file before it starts the next one
  (`wait_for_got()`), and an ND receiver holds a completed file in
  `in_complete` and writes it at the next `M_FILE` or at the end of the
  batch (`finish_file()`, the port of `inb_done()`), the port of the
  `waiting_for_GOT` and `in_complete` states of `protocol.c`. Step 2 is done: the session holds
  the flags (`NrMode`, `NdMode`), the `OPT` negotiation of both roles, the
  `M_FILE "name size time -1"` announcement of an NR sender, the `M_GET`
  handler with the offset and the `NZ` extra, the `M_SKIP` handler, the
  `sent_fls` list, the `off_req_sent` state, and the `GET_FILE_balance`
  counter. The end of the batch waits for the answer of every request and for
  the file in transfer, the port of `sent_fls == 0`, `GET_FILE_balance == 0`,
  and `in.f == 0` of `protocol()`. Step 1 is done: the parser of
  `binkd-config` holds the `-nr` and `-nd` node options, the `-nd` option
  implies `-nr`, the `-d` dump prints the two options, and the session settings
  hold the `nr` and `nd` fields. The corpus pins the parse and the dump
  (`full.conf`, `reload_node.changed.conf`). The steps are in the section
  below.

The NR mode and the ND mode:

The two modes touch three layers: the config, the session, and the spool. The
plan keeps the steps in this order:

1. `binkd-config` holds the `-nr` and `-nd` node options and the `nr` and
   `nd` fields of the session settings. The `-nd` option implies `-nr`. The
   `-d` dump prints the two options. (done)
2. `binkd-proto::session` holds the flags, the `OPT` negotiation, the
   `M_FILE "name size time -1"` announcement, the `M_GET` and `M_SKIP`
   handlers, the `sent_fls` list, and the `GET_FILE_balance` counter. (done)
3. `binkd-proto::session` holds the `waiting_for_GOT` wait of the ND
   sender and the deferred rename of the ND receiver, the port of
   `in_complete`. (done)
4. `binkd-spool` holds the `.stc` status files and the queue type `s`
   of the outbound scanner, and the session announces a status entry and saves
   it from `M_GOT`. (done)
5. The interop matrix covers the two modes against the C binary: the Rust
   client asks for the NR mode and the C server answers the announcement with
   `M_GET` and reports `remote is in NR mode`, and the Rust client asks for
   the ND mode and the C server reports `we are in ND mode`. (done)
   The replay test covers the modes: a recording holds the `nr` and the `nd`
   fields with the `# nr` and `# nd` header lines, the writer records a pair of
   sessions with the modes on, and the replay configures the session from the
   recording. (done)

The C semantics, for the port:

- `NR_flag`: `WANT_NR` (1) we ask for the NR mode, `WE_NR` (2) we
  announce files with offset -1, `THEY_NR` (4) the peer is the NR sender.
- `ND_flag`: `WE_ND` (1) we wait for `M_GOT` after each file,
  `THEY_ND` (2) the peer waits, `CAN_NDA` (4) the peer supports the
  asymmetric mode.
- The caller sets `WANT_NR` for a node with `-nr` or `-nd`, and `THEY_ND`
  for `-nd`. The caller announces `NDA EXTCMD`, `NR` when it wants it,
  and `ND` when it holds `THEY_ND`. The answerer echoes `NR`/`ND` from its
  config and adds `NDA` when exactly one side holds `WE_ND` or `THEY_ND`.
- An NR sender sends `M_FILE "name size time -1"`, sets `off_req_sent`,
  and sends no data until the peer answers. The peer answers with
  `M_GET "name size time offset"` or `M_SKIP "name size time"`.
- `M_GET` moves a file of `sent_fls` back to `out`, seeks to the offset,
  and re-announces the file with that offset. `GET_FILE_balance` counts
  the open requests and guards the end of the session.
- An ND sender waits for `M_GOT` after each file (`waiting_for_GOT`), and
  an ND receiver renames a received file at the next `M_FILE` or at the
  end of the session (`in_complete`), not at once.
- An ND sender keeps a `.stc` status file in the outbound and announces
  the status as a file with offset equal to its size. The outbound scanner
  reads those files as the queue type `s`.
- A session ends when the peer sent its end of the batch, we sent ours, no
  file waits for an acknowledgement, no request is open, and no file is in
  transfer on either side. The C code then starts the next batch when it
  sent or received more than the two `M_EOB` frames of the batch.

Exit criteria:

- The frames match the recorded sessions byte for byte.
- The interop matrix passes for plain, CRAM, CRYPT, GZ, and BZ2.
- The NR mode, the ND mode, and the multi-batch mode pass.

Known gaps, for the start of phase 5:

- The receiver declines a file with the rules of the `skip` directive, the port of
  `skip_test()` of `protocol.c`: a rule that matches the address type and the name
  of the file sends `M_SKIP`, and a destructive rule sends `M_GOT`. (done)
- The receiver always asks for a file from offset 0: the port keeps a received file
  in memory and holds no partial file, so a resumed transfer starts again. The C code
  keeps the partial in the inbound directory and asks from its offset. The APIs are
  ready (`binkd-spool::inbound::inb_fopen`, `inb_done`, and `inb_reject`), and the
  change belongs to the receive path of the session: the file handle of the partial
  lives in the state of the file, each data block writes to it, the announcement
  asks from the size of the partial, and the completion renames it.
- The counters `bytes_sent` and `files_sent` of the report grow at the send path,
  where the C code grows them at the acknowledgement of the peer.
- The test `c_client_to_rust_server_gz` fails now and then under the parallel load
  of the suite: the C client of that test runs through `-P` and `-p`, and the waits
  of the harness need a review. The multi-batch
  mode starts the next batch when the batch held more than the two `M_EOB` frames,
  the port of the end of the batch of `protocol()`: the session counts the command
  frames of the batch (`msgs_in_batch`), the login forgets them, and the end of a
  batch of more than two frames sends the end of the batch again and reads the next
  one. The in-memory test covers a file of the second batch, and the replay of a live
  session of the C binary passes. The frame trace of that replay found the
  divergence: the answerer of the port completed the login at the `ADR` frame of the
  caller when it held no password, so the `OPT` frame of the caller arrived after
  the reset of the batch and the count passed the two `M_EOB` frames. The C caller
  always sends `M_PWD`, with `-` without a password, and the C answerer completes
  the login at that frame, so the `OPT` frame counts before the reset. The port now
  follows the same flow. A trace of the frames of a session is available with the
  `BINKD_TRACE` variable and with `loglevel 10` of the C binary.
- The interop test `frame_sequences_match` compares the frame sequences of the
  two implementations in both directions, in the order of the wire, so a divergence
  of the flow fails the suite. The test runs the plain session and the sessions of
  zlib compression, bzip2 compression, the crypt mode, the NR mode, and the ND
  mode. The session logs the frames itself, before the encryption on the send path
  and after the decryption on the receive path, as the C code logs them, so the
  log of a crypted session is comparable. The settings hold the log in `trace`,
  and `Report.frames` carries it. The parser keeps the frames of the process of the
  session of the C log: the server of the C code forks a process for the session
  after it queues the banner, so the parent and the child both write the frames of
  the banner to the log. The frames of the process that reads the session, the port
  of `recv_block()`, are the frames of the session.

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
7. Minimum supported Rust version: 1.85, stated as `rust-version` in
   `rust/Cargo.toml`. The dependency policy is in `AGENTS.md`.

## 8. Risks

| Risk | Effect | Mitigation |
| --- | --- | --- |
| `protocol.c` holds state in globals | The wire behaviour can change | Model the state as one value. Use the replay tests |
| The config parser has 100 keywords | The parse result can change | Use the corpus and the `-d` dump |
| The core files have `#ifdef` blocks | A port can mix platform behaviour | Keep the platform code at the edges |
| The concurrency model changes | `.bsy` files and the cleanup can differ | Keep the level 4 tests |
| Perl hooks need the Perl interpreter | The hooks can break | Decide in phase 7. Keep the C module first |
| The log text changes | Log parsers can break | Keep the line format. Pin it with a test |
| The `-d` dump changes | Config tools can break | Compare the dump in phase 3 |
| The port has no user feedback for long | The port can drift | Each phase ends with a green suite |

## 9. Verification commands

```
make -C tests                           # the C tests and the C corpus runner
make -C tests clean                      # remove the test build
cd rust && cargo test --workspace         # the Rust tests and the corpus tests
cd rust && cargo clippy --workspace --all-targets
```

The interop tests arrive in phase 4. They need the C binary and the Rust
binary. Build the C binary with the procedure in section 2.1. Build the
Rust binary with:

```
cargo build -p binkd
```

## 10. Milestones

The milestones follow the phases. Each milestone is a mergeable state.

1. Milestone 1: phase 0 and phase 1 (done). The Rust pure core matches
   the C code on the corpus.
2. Milestone 2: phase 2 and phase 3. The Rust code reads and writes
   the spool and parses every config.
3. Milestone 3: phase 4. The Rust code runs a binkp session with the C
   code.
4. Milestone 4: phase 5 and phase 6. The Rust binary runs as a daemon
   and as a library.
5. Milestone 5: phase 7 and phase 8. The Rust code is the default. The
   C code is a reference or is gone.

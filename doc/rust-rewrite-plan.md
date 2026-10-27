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

### Phase 4: protocol (done)

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
- The receiver resumes a partial file, the port of `inb_fopen()`, `inb_done()`,
  and `inb_reject()` of the spool: the session holds the state of the file
  (`TFile`), the handle of the partial file, and the count of the bytes that
  arrived. On `M_FILE` it tests the duplicate with `inb_test()` and answers
  `M_GOT` for a file that it holds, it opens the partial with `inb_fopen()` and
  answers `M_SKIP` when the open fails, and it asks with `M_GET` from the size
  of the partial. Each data block writes to the partial file, the completion
  calls `inb_done()`, which renames it to its true name and returns it, and the
  end of the batch keeps a partial of a peer of the NR mode and calls
  `inb_reject()` otherwise. (done)
- The tests of the resume: `session_with_a_partial_file` drives a receiver that
  holds a partial and asks from its position, `rust_client_to_c_server_resumed`
  resumes a partial of the C server from the port, and
  `rust-server-resumed.session` replays a resumed transfer. The other direction
  of the matrix is not testable: the C client names a mail packet with a random
  name, so a test cannot hold the partial that the port would find. (done)
- The frame tests of the interop suite retry a scenario of the peer: the C binary
  closes the transport at the end of a session, and the socket of the peer may answer
  with a reset before the frames of the last batch are read. The test of a frame runs
  the scenario again for that race.
- The port of `free_port()` of the interop suite is unique in the process: a port of a
  closed listener is free at the moment, and another test of the suite takes it under
  the parallel load.

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

Progress:

- `binkd-log::level` holds the levels of a message and their characters,
  the port of the `marks` table of `vLog()`: the levels 0 to 3 have a
  character of their own, and every other level a space. (done)
- `binkd-log::log` holds the text log, the port of `InitLog()` and
  `vLog()`: the console line of 30 spaces, a carriage return, the
  character of the level, the hour and the minute, and the process id; and the
  file line of the character, the day, the name of the month, the time, the
  process id, and the message. The C code opens and closes the file for every
  message, and the port does the same. The caller supplies the local time,
  because `std` has no local time: the platform layer of the binary owns the
  timezone. A message of the level 0 is fatal and returns `Outcome::Fatal` for
  the caller, the port of the `exit(1)` of `vLog()`. (done)
- `binkd-log::binlog` holds the binary session logs, the port of
  `BinLogStat()`: the T-Mail record of 28 bytes and the FrontDoor record of
  116 bytes, both little-endian, with the status of the T-Mail record and the
  direction that selects the FrontDoor file. (done)
- The tests of the crate cover the two line formats, the masks of `nolog`,
  the fatal level, the fields of the two records, the time offset, and the two
  files.
- `binkd-net::port` holds the port names, the port of `find_port()`: the names
  that the C code uses and the numbers. The C code asks the services database
  of the system, so an unknown name is an error for the port and may resolve
  for the C code. (done)
- `binkd-net::hosts` holds the host list of a node, the port of
  `get_host_and_port()`, and the host name of an address, the port of
  `ftnaddress_to_domain()`. (done)
- `binkd-net::session` holds the settings of a session from the config, the
  port of the reads of `init_protocol()` and `protocol()`: the banner, our
  addresses, the password of the node, the modes, the compression, the spool,
  the `skip` rules, and the ND link status. (done)
- `binkd-net::listener` holds the listen sockets and the accept loop, the port
  of `do_server()`: one `TcpListener` for every `listen` entry, one task for
  every listener, and one task for every accepted connection. The C code forks a
  process and closes the listen sockets in the child; the port holds them in the
  task of the listener, so the sessions share one process. (done)
- The `maxservers` limit is the port of the check of `protocol()`: the session
  settings hold the limit and the count, and the session answers `M_BSY` after the
  banner, and only for a session that the peer called. The listener counts the
  sessions that run and gives the count to every session that it starts. (done)
- The log of the port is shareable: `binkd-log::Log` holds its state in a mutex,
  the port of the process wide variables of the C code, so the listener, the
  caller, and the sessions write to one log.
- `binkd-net::caller` holds the outgoing calls, the port of `do_client()` and
  `call0()`: the node selection of `q_next_node()`, the busy gates of `.bsy` and
  `.csy`, the `.try` bookkeeping of `bad_try()` and `good_try()`, the host list,
  and the timed connect. The C code forks a process for every call; the port
  calls the nodes in turn. (done)
- `Queue::scan_listed()` is the port of `q_scan(SCAN_LISTED, config)` of
  `do_client()`, which holds no queue entry and gives the flavours to the nodes;
  `Queue::scan()` is the port of the scan of the corpus, which holds them.
- The loopback test of two Rust instances passes: the caller and the listener
  connect, both sessions run, and each side writes the packet of the other.
  (done)
- The interop matrix of the network layer passes, the port of the matrix of the
  session layer: `rust_caller_to_c_listener` runs the caller of the port against
  the C listener, and `c_caller_to_rust_listener` runs the C client against the
  listener of the port. Both sides send the packet of their outbound and write the
  packet of the peer, so the tests cover the two roles. (done)
- The frame log of a session is a setting of the port: `session_config()` and the
  caller and the listener take the flag, the port of the frame log of the C code
  at `loglevel 10`, and `Report.frames` carries the frames. The trace found the
  two causes of the loopback below.
- The caller scans the spool before it selects a node, the port of `do_client()`
  of the C code: the scan gives the flavours to the nodes, the port of
  `q_scan(SCAN_LISTED, config)`. Without the scan the port found no node.
- A test that needs the defaults of the C code calls `Config::new()`: the derived
  `Default` of the port gives a zero for every value, so `max_servers` was zero
  and every session of the listener was refused with `M_BSY`. The port of
  `readcfg.c` starts from `lock_config_structure()`, which is `Config::new()`.
- The settings of a session that a manager passes are one value,
  `SessionOptions`: the timeout, the crypt flag, and the trace flag. The listener
  and the caller take the value, the port of the fields of `STATE` that
  `do_server()` and `do_client()` set.
- `binkd-net::proxy` holds the proxy support, the port of `https.c`:
  `proxy_address()` resolves the proxy server, the port of the resolution of
  `call0()`, and `h_connect()` runs the handshake of the HTTP CONNECT proxy
  (without auth, with the basic auth, and with the NTLM auth), of the SOCKS4
  proxy, and of the SOCKS5 proxy (without auth and with the user and the
  password of RFC 1929). The caller wires both, the port of `use_proxy` of
  `call0()`, which skips the proxy for a node of `-noproxy` and for a node with
  a pipe. (done)
- `binkd-net::base64` holds the codec of the two `enbase64()` functions and of
  `debase64()`: the C code stops the decode at the first character outside the
  alphabet. (done)
- `binkd-net::ntlm` holds the NTLM support, the port of `ntlm/helpers.c`: the
  Type 1 and the Type 3 messages, the LM and the NT hash, and the responses.
  `binkd-net::md4` holds the MD4 of `ntlm/md4_dgst.c`, and `binkd-net::des`
  holds the DES of `ntlm/des_enc.c`, `ntlm/set_key.c`, and `ntlm/ecb_enc.c` with
  the tables of `ntlm/spr.h` and `ntlm/set_key.c`. The C code vendors OpenSSL
  and holds `des_check_key` at zero, so the port does the same. (done)
- The tests of the proxy: `crates/binkd-net/tests/proxy_interop.rs` runs the
  caller of the port through a proxy server of the test against the C listener for
  every protocol, and it runs the C client through the same proxy server against
  the listener of the port and compares the request of the C code with the request
  of the port, byte for byte. The test of the NTLM auth compares the two
  messages of the port with the messages that the proxy of the test received.
  The C binary of the tests needs the `--with-proxy` and the `--with-ntlm`
  switches of `configure`; the corpus tests need the plain build. (done)
- The divergences of the port, for the record:
    - A SOCKS4 server that rejects every address of the answer leaves the C code
      with the value zero of `h_connect()`, which is a success. The port answers
      an error, so the caller never runs a session on a socket without a tunnel.
    - The C code waits with `select()` for every single byte of the answer, so a
      slow peer may hold a handshake longer than `nettimeout` in total. The port
      waits for every read.
    - The C code holds no fixed buffer, so the port cannot answer the `-1` error of
      `getNTLM1()` and `getNTLM2()` for a result buffer that is too small.
    - The C code reads the port of the entry of `get_host_and_port()` as a text,
      so a name stays a name and a number a number. The port does the same, so the
      request line of `CONNECT` holds the name like the C code.
- `binkd-net::dns` holds the DNS client of the SRV lookup, the port of the
  `res_search()` and of the `ns_initparse()`/`ns_parserr()` calls of the C
  library: the query name, the search list of `resolv.conf`, the answer, and the
  decompression of a name. The C code asks the resolver library of the system, so
  the port holds its own client and keeps the dependency footprint at zero. The
  settings are the port of `res_init()`. (done)
- `binkd-net::srv` holds the SRV lookup, the port of `srv_gai.c`: the guards
  that answer a plain resolution, the query `_<service>._<tcp|udp>.<node>`, the
  records in the order of the answer, the port of the record that wins over the
  service, the second resolution of the target, and the fallback to a plain
  resolution. The C code never reads the priority and the weight, so the port does
  not follow RFC 2782 either. (done)
- The port of the SRV lookup is testable: the settings of the resolver are a
  value, so `crates/binkd-net/tests/srv.rs` runs the lookup against a server of
  the test on the loopback address. The C test suite holds no test of the SRV
  lookup at all. (done)
- The divergences of the SRV lookup, for the record:
    - The C code asks the services database of the system for a name, so a name
      that the port does not hold is an error for the port. The port holds the
      names of the C code.
    - The resolver library of the C code retries a truncated answer over TCP, and
      it holds a cache. The port holds neither.
    - The C code holds no resolver at all on Windows without the `FTS5004`
      switch, and it never holds one on DOS, Amiga, and OS/2 ICC. The port holds
      no settings on Windows, so it answers a plain resolution there. The port
      matches the default build of the C code on both platforms.
- `binkd-proto::bw` holds the rate limit, the port of the `BW_LIM` code of
  `protocol.c`: the meter of the two directions, the average of the C code with its
  window of ten seconds, the wait of a direction over the limit, the limit of a file
  from the rules of `limit-rate` and the `-bw` of the node, and the limits of the
  session from the listed addresses of the peer and the `defnode`. (done)
- The tests of the rate limit: `bw.rs` holds the arithmetic of the average and of
  the wait, `session.rs` holds the transfer of a file of 4096 bytes at the limit of
  4096 bytes per second, and `interop.rs` holds the same transfer against the C
  server of the `--with-bwlim` build, where the C code of the meter is the oracle.
  (done)
- The `-bw` option of a node and the rules of `limit-rate` are the port of the
  config: the option and the rules answer a plain resolution without the `BW_LIM`
  build of the C code, the port of the `Features` of the port. The C corpus of the
  tests holds no case of them, because the corpus runner of the C code holds the
  plain build. The port covers them with the unit tests of the parser and with the
  interop test of the rate limit. (done)
- The divergences of the rate limit, for the record:
    - The C code reads the wall clock and it resets the meter when the clock steps
      back. The port reads a monotonic clock, so the time cannot step back.
    - The C code holds the descriptor of a direction out of the `select()` set and it
      lowers the timeout of the `select()`. The port waits with the timer of the
      runtime before the read or the write of a block, so the bytes stop in the same
      way.
    - A `send()` that fails lowers the counter of the C code by one, because the C
      code counts the answer before it reads it. The port counts the bytes of a
      successful write only.
    - The tokenizer of the C code eats a single `%` of a value, the port of the
      `%VAR%` substitution of `getwordx2()`, so a rate in a config needs the
      doubled `%%`. The port does the same, and the dump holds the single `%` like
      the C code.
- `binkd-srif` holds the SRIF events, the port of `srif.c`. The C code holds
  three features in the file, and the port keeps one module for each:
    - `binkd-srif::event` holds the `flag` and `exec` events, the port of
      `evt_test()`, of the queue of `evt_queue()`, and of the loop of
      `evt_set()`: the matching of a received file against the patterns of the
      config, the copy of an event of the same type and path, and the newest
      event at the head of the queue. (done)
    - `binkd-srif::args` holds the macros of an `exec` command, the port of
      `run_args()`: `*N`, `*F`, `*A*`, `*A@`, `*A0` to `*A9`, `*A` with any
      other character, `*P`, `*L`, and `*H`, the check of the name of the
      received file, and the refusal of a command that holds `*F` or `*N` with a
      name that the C code does not accept. The port answers the name of the file
      for `*N` on every platform, because the short name of Windows needs the
      `GetShortPathName()` call. (done)
    - `binkd-srif::srif` holds the two files of the exchange, the port of
      `mksrifpaths()`, of `srif_fill()`, and of `parse_response()`: the paths of
      the request and the response file, the content of the request file, and the
      lines of the response file with the three predictors. (done)
- The port passes a narrow value into the crate instead of the whole session, the
  port of the fields of `STATE` that `srif.c` reads. The C code holds the cycle of
  `protocol.c` and `srif.c`, one of the known exclusions of the plan, and the port
  breaks it.
- The port leaves the external program to the caller, the port of the plan that puts
  `run.c` into the binary. The C code runs the program itself with `system()`, so a
  program that hangs hangs the session.
- `binkd-srif::engine` holds the SRIF events of a session, the port of
  `evt_run()` and of the parts of `inb_done()` of `inbound.c` and of `protocol()`
  that call it: the `flag` and `exec` events of a received file, the SRIF exchange
  of a `*S` command, the files that wait for the end of the batch, the run of the
  external program, and the FREQ claims of the peer, the port of `state->delay_EOB`.
  The C code runs the program itself with `run()` of `run.c`, and the port takes a
  `Runner`, the port of the plan that puts `run.c` into the binary. (done)
- The tests of the engine run on a temporary directory with the real spool: the flag
  file at once and delayed, the touch of a flag file that is there, the command at
  once and delayed, the SRIF exchange with a program of the test that answers the
  response file, the deferred exchange of the end of the batch, and the claims of the
  peer. The C test suite holds no test of the SRIF events at all. (done)
- The session wires the events, the port of option A of the plan: it takes a
  `Srif` value like the `files` and the `receive` closures, so `binkd-proto` never
  depends on `binkd-srif`. The port of `protocol.c` calls it at the four points of
  the C code: the `M_NUL FREQ` frame of the peer claims a FREQ, the `M_EOB` frame
  of the peer answers every claim of the batch, the completion of a received file
  runs the events and answers the files of an exchange, the end of a batch sends the
  files of the exchange before our own `M_EOB`, and the end of the session runs the
  events that wait. (done)
- The tests of the wiring: a session of the port with a recording `Srif` sends a
  file, the answerer answers the file of the exchange, and the caller receives it.
  The C test suite holds no such test. (done)
- `binkd-net::srif::Events` holds the real wiring, the port of the calls of
  `protocol.c` to `srif.c`: the engine of `binkd-srif` with the queue of the spool,
  the `Runner` that runs the external program, the port of `run()` of `run.c`, and
  the `binkd_srif::Session` of the fields of the session, the addresses and the
  flags of the peer and the name and the address that the transport knows. The
  listener and the caller create one value for every session. (done)
- The `Runner` runs the command with the shell of the system, the port of
  `system()` of the C code, and it logs the command and its answer. The C code
  blocks the session for the length of the program, and the port does the same: the
  plan puts the long work into the tasks of the sessions, and a task of a session
  never holds another one. (done)
- The test of the wiring runs the caller and the listener of the port against each
  other: the caller sends a request file, the listener holds an `exec` entry with
  the `*S` macro, the external program of the test answers the file of the
  exchange, and the caller receives it. The test needs the right of execution of
  the script, the port of the `system()` of the C code. (done)
- The `M_NUL FREQ` claim of the C peer: the C code announces the claim for a
  session of the protocol version 1.0 only, the port of `do_prescan()` of
  `protocol.c` and of the answer of `PRTCLVER` of `Config.h`. The C code sends
  `binkp/1.1` and it takes `state->major` and `state->minor` from the version
  of the peer, so a session of the port and of the C code holds the version 1.1,
  and the C code never announces the claim. The port holds the version of the
  protocol in the settings, the port of `PRTCLVER`, so a peer may answer the
  version 1.0, and the session answers the end of the batch after the exchange
  then, the port of the `delay_EOB` test of the C code. (done)
- The test of the claim drove the send path of the port, the port of the main
  loop of `protocol.c`: the port sends the files of an exchange and its own end
  of the batch in the send path now, and it reads the frames of the peer after
  the send path, as the C code does. The C code holds its own end of the batch
  back for a peer of the version 1.0 only, the port of the `delay_EOB` test, so
  the test runs the C client of the version 1.0, and the C client claims the
  FREQ and holds its own end of the batch back as well. The port reads the
  header of a frame, decrypts it, and learns the length of the data then, the port
  of `recv_block()`, so the read path holds the decrypted header for the data,
  the port of `pending_header`. (done)
- The port of `netname_()` of `prothlp.c`: the session announces a file of
  the mail and a file of a request with a random name, and it keeps the name of
  every other file, with the week of an arcmail name. The C code takes the kind
  of a file from the queue, and the port takes it from the name, as the scan of
  `q_add_dir()` of `ftnq.c` does. The tests of the network take a file of an
  inbound by its extension then. (done)
- The week of `netname_()` of `prothlp.c` holds the cases of the corpus now,
  the port of the days of the week and of the names that hold no case: the case
  of the day 0 to 6 answers `su`, `mo`, `tu`, `we`, `th`, `fr`, and `sa`, the
  day 7 and an extension of two characters hold no case, and a name of the mail
  holds no case, the port of the random name of the port of the test. (done)
- The port of `state->r_skipped_flag` of `protocol.c`: the session holds the
  queue of the session back when the peer skipped a file of it, the port of
  `SKIP()`, and `OK_SEND_FILES()` reads the flag. The test of the flag runs
  the C client of the version 1.0 against a listener of the version 1.1, so no
  side claims a FREQ and no side holds its own end of the batch back, the port
  of `delay_EOB`, and the C client skips the request file of the port. (done)
- A gap that the test of the flag found: the port counts a file of the session
  at the announcement of it, the port of `current_file_was_sent()`, and the C
  code counts it at the acknowledgement, the port of `GOT()`, so the count of
  the port holds a file that the peer declined. The test of the flag reads the
  files of the report that the port received, the port of the count of the C code
  that the port holds open.
- The port answers the frames that the read ahead holds before it closes the
  session, the port of the read of the loop of `protocol()`: the C code reads one
  frame of the transport at the top of its loop, so the peer answers the file of
  the session after its own end of the batch, and the port reads ahead, so the
  frames that arrived with the frame of the peer wait for the pass of the read.
  The port answers one frame in one place now, the port of `GOT()`, so the count
  of the files of the session holds the acknowledgement of the peer only.
  (done)
- A gap that the test of the flag found in the batch of a peer of the version
  1.1: the C code starts the next batch of a session that holds more than the
  two `M_EOB` frames, the port of the test of `protocol()`, and the port
  starts the next batch of the session too, but the C client ends the session of
  the test with the status of a failure, because its next batch holds no file and
  the port holds its own end of the batch back. The C code holds the same shape,
  so the test of the flag reads the skip of the peer and the flag of the port
  only.
- The port of `do_prescan()` of `protocol.c`: the session scans the spool for
  the peer after the login, the port of `q_scan_addrs()`, and it announces the
  number of the nodes and the size of the files, the port of `q_get_sizes()`,
  and one claim per request file of the queue for a peer of the version 1.0, the
  port of `q_freq_num()`. The send path sends the queue that the scan builds,
  the port of `state->q` of `complete_login()`. (done)
- A bug that the test of the claim found in the spool: the port of `q_add_dir()`
  of `ftnq.c` skipped every file of an outbound directory whose name did not
  answer the address of our own side, which the C code does not do: the C code
  answers the file of the peer, and it skips the file of a node that the config
  does not hold and that the address of our own side does not cover, the port of
  the test of `get_node_info()` and of `is5D()`. The prescan of the port found
  no file of the peer before the fix, so the port announced an empty queue and
  never sent its own claim. (done)

Exit criteria:

- Two Rust instances exchange a packet. (done)
- A Rust instance and a C instance exchange a packet in both roles. (done)
- The proxy, the SRV lookup, and the rate limit match the C code. (the proxy
  passes the interop matrix of `proxy_interop.rs`, the SRV lookup passes the tests
  of `crates/binkd-net/tests/srv.rs`, and the rate limit passes the tests of
  `crates/binkd-proto/tests/session.rs` and of `crates/binkd-proto/tests/interop.rs`)

### Phase 6: binary and library

Deliverables:

- The `binkd` crate with the CLI, the startup order, the signals, the
  pid file, the config reload, and the external program start. (the command
  line, the version text, the platform values, and the paths that read the
  config are done: `-h`, `-v`, `-vv`, `-d`, `-n`, the messages of the errors
  of the command line, and the ends of the config; the startup order, the
  signals, the pid file, the config reload, and the external program start
  arrive with the next slices)
- The public library API and an example. (the library of the slice holds the
  command line, the version text, the platform values, and the startup order;
  the engine and the example arrive with the next slices)
- An integration test with the sample config. (done: the test of
  `crates/binkd/tests/cli.rs` runs the binary of the port and the C oracle and
  checks the text, the error text, and the exit code of every path)
- The daemonize path: the `-D` switch detaches the process, the port of
  `binkd_daemonize()` of `unix/daemonize.c`: the parent ends with the code 0
  at once, and the child runs the daemon in a new process group with the
  standard files of `/dev/null`, the port of the `setsid()` and of the
  `freopen()` calls of the C code. `std` states no `fork()`, so the port runs the
  same program again and the mark of the environment holds the port of the
  `already_daemonized` of the C code. (done)
- The inetd path: the `-i` switch runs one session on the sockets of the
  caller, the port of the `protocol (inetd_socket_in, inetd_socket_out, ...)` of
  `main()` of `binkd.c`, and the program ends with the code 0. The `-a` switch
  holds the remote address of the session, the port of the `remote_addr` of the C
  code. (done)
- The check of the config files on a timer: the daemon runs `checkcfg()` of
  `readcfg.c` on every pass of `CHECKCFG_INTERVAL`, the port of `rescan_delay`,
  and it restarts the listener of the port when a file changed. (done)
- The client manager: the daemon runs the passes of `binkd_net::Caller`, the port
  of `do_client()` and of `clientmgr()` of `client.c`, with `-p` running one pass
  and ending the program, and the `-P` switches writing the polls of the nodes, the
  port of `poll_node()` of `binkd.c`. The port of `call_all()` scans the queue of
  the spool and calls the nodes, the port of `q_scan (SCAN_LISTED, config)` and of
  `q_next_node()`. (done)
- A bug that the test of the poll found in the test itself: the default runtime of
  `#[tokio::test]` holds one thread, and the wait of the test blocked it with
  `std::thread::sleep`, so the listener of the port in a task never ran and the
  daemon answered `NoHost`. The test takes the multithread runtime now. (done)
- The exchange of the files of a poll: the daemon calls the node, sends the packet of
  its outbound, receives the packet of the peer into its inbound, and the peer
  receives the packet of the daemon, the port of `do_client()` of `client.c`. The port
  of `find_tmp_name()` and of `inb_done()` of `inbound.c` holds the two calls of a
  file: `inb_fopen()` answers `Created` and writes the temporary name, and
  `inb_done()` finds the `.hr` file, matches the name, the size, the time, and the
  address, and answers `Found`. (done)
- The listener manager: the daemon starts `binkd_net::listen()` for a run with
  the server role, the port of `do_server()` of `server.c`, and the loop of
  `servmgr()` rebuilds the listener on a reload. (done; the run of the port waits
  for a signal, because the client manager arrives with the next slice)
- A gap that the test of the listener found, and that the port holds now: the
  answerer of the port sent no file of its queue, though the scan of `files_of()`
  found the file of the peer, so the caller received nothing. The port of the session
  already renames a received file, the port of `finish_file()`, so the receive closure
  of the daemon must not run `inb_done()` a second time: the second call answers
  `missing tmp file`. The test of the port holds the exchange of the files in both
  directions now: each side sends the packet of its outbound and receives the packet
  of the peer, and the daemon writes the packet of the caller into its inbound.
  (done)
- A bug that the test of the binary found in the log: the port of `vLog()` of
  `tools.c` wrote no newline after the console line of a message of a level that
  is no error of the config. The C code ends such a line with `\n` and holds no
  newline for a negative level, the level of the errors of the config, whose
  message ends its own line. (done)
- The daemon startup: the `BEGIN` line with the arguments, the busy list, the
  pid file, the streams of `SIGTERM`, `SIGINT`, and `SIGHUP`, the end of the
  daemon on a signal, the check of the config files on `SIGHUP`, and the cleanup
  of `exitfunc()`. (done; the daemon of the slice waits for a signal, because
  the listener and the caller arrive with the next slice. The port takes the
  `signal` feature of `tokio` for the handlers, because `std` states no signal
  of a process: the feature holds the small `signal-hook-registry` crate)

Exit criteria:

- The Rust binary passes every level 4 scenario.
- The example embeds a session in another program.
- The command line options match `binkd.8`. (the options of the slice match the
  C code and `binkd.8`: the switches, the usage text, and the exit codes of
  `-h`, `-v`, `-vv`, `-d`, `-n`, and of the errors of the command line)

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

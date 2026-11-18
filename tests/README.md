# Tests of the Rust implementation

The tests in `rust/` check the Rust code.
The golden corpus in `golden/` holds the value cases.
The Rust tests read the corpus.
The upstream C implementation is the oracle only in the interop jobs.

## Run the Rust tests

```
cd rust
cargo test --workspace
```

The Rust tests read the corpus. A difference in one case fails the test with
the file name and the line number.

## Run the Windows service smoke test

The script `windows-smoke.ps1` needs Windows, an administrator PowerShell, and
the release binary. It installs a temporary service and checks install,
status, stop, start, restart, uninstall, the process, the busy and pid files,
and the log. It removes the service and the temporary directory at the end.

```
cd rust
cargo build --release -p binkd
cd ..
pwsh -NoProfile -ExecutionPolicy Bypass -File tests/windows-smoke.ps1
```

The `windows` job of `.github/workflows/ci.yml` runs the script.

## Run the Windows parity tests

The parity tests are target-gated and run in the `windows` CI job. On a
Windows host:

```
cd rust
cargo test -p binkd-win
cargo test -p binkd-config
cargo test --workspace
```

The cases cover the short-name and volume wrappers, the partition provider,
the Windows feature set, and the Windows `-d` dump. On another system the
cross-target check is:

```
cd rust
cargo check -p binkd-win -p binkd-config -p binkd --target x86_64-pc-windows-gnu
```

## Run the Perl tests

The hooks of Perl need the optional crate `binkd-perl` and the interpreter
of the system:

```
cd rust
cargo test -p binkd --features perl --test perl
cd crates/binkd-perl
cargo test
```

The script `tests/perl/hooks.pl` holds every hook. The hook writes its name to
the file of the environment `BINKD_PERL_MARK`, so a case holds the hooks that
ran. The port cases of `crates/binkd/tests/perl.rs` hold the call hooks, the
session hooks end to end, and the reload hooks. The oracle cases run against
the upstream C binary when the environment names `BINKD_C_PERL`, a binary of
`./configure --with-perl`.

The cases of `crates/binkd-perl/tests/hooks.rs` call every hook and hold the
marks, the return values, the abort text, the new file names, the action, and
the rate limit. The case `two_sessions_do_not_share_the_variables` holds the
per-session clones with two threads, and the cases
`concurrent_calls_do_not_race_on_the_master` and
`two_clones_call_the_log_hook_on_two_threads` hold the log hook on two
threads. The unit test `the_hook_runs_on_two_threads` of `binkd-log` holds the
log without Perl. The port case
`the_port_of_the_perl_clones_of_two_sessions` holds the clones of two sessions
of the daemon. The cases `a_hook_message_reaches_the_peer`,
`the_session_values_reach_the_hook`,
`the_traffic_of_the_files_reaches_the_hook`, and
`the_compression_counters_reach_the_hooks` of
`crates/binkd-proto/tests/session.rs` hold the sink of `msg_send()`, the session
values, the traffic, and the compression counters. The cases
`the_session_variables_reach_the_script`,
`the_traffic_and_compression_variables_reach_the_script`,
`the_session_queue_reaches_the_script`,
`the_script_rebuilds_the_session_queue`,
`the_script_removes_an_entry_from_the_session_queue`,
`the_script_removes_an_entry_of_after_recv`,
`the_script_deletes_an_entry_of_before_recv`, and
`the_script_deletes_an_entry_of_after_sent` of
`crates/binkd-perl/tests/hooks.rs` hold `$start`, `$host`, `$ip`, `$sysname`,
`%opt`, `$traf_mail`, `$traf_file`, `$z_send`, `$z_recv`, and `@queue` in the
script, the values that the script writes back into `@queue`, the deletion of
an entry in `before_send()` and `after_recv()`, and the deletion in
`before_recv()` and `after_sent()` that the upstream C code does not apply.
The cases
`the_hook_rebuilds_the_session_files`,
`the_hook_removes_a_file_from_the_session`, and
`the_hook_removes_a_file_of_after_recv` of
`crates/binkd-proto/tests/session.rs` hold the files that the session rebuilds
and drops from the queue. The case
`the_session_does_not_refresh_the_queue_of_before_recv` holds the upstream C
rule that
`before_recv()` does not refresh the queue, and
`the_session_does_not_refresh_the_queue_of_after_sent` holds the same rule for
`after_sent()`. The port cases
`the_port_of_the_rebuilt_session_queue`,
`the_port_of_the_removed_file_of_the_session_queue`,
`the_port_of_the_removed_file_of_after_recv`,
`the_port_of_the_queue_of_before_recv`, and
`the_port_of_the_queue_of_after_sent` of `crates/binkd/tests/perl.rs` hold the
script of the daemon that adds a file to the queue, the scripts that remove one
in `before_send()` and `after_recv()`, and the scripts that delete `@queue` in
`before_recv()` and `after_sent()` while the file still arrives.
The case `the_port_of_the_reload_of_the_script` of
`crates/binkd/tests/perl.rs` holds the reload of the script beside the new
config: the new script answers `config_loaded()`, a session uses it, the old
script is not called again, and `on_start()` runs only for the script of the
start. The case `the_hook_of_after_recv_runs_before_the_rename` of
`crates/binkd-proto/tests/session.rs` holds the order of `after_recv()`: the
hook sees the partial file and the real name before the rename and may change
the name. The case `the_after_recv_values_reach_the_script` of
`crates/binkd-perl/tests/hooks.rs` holds `$tmpfile` and `$file`. The case
`the_where_of_the_error_follows_the_c_code` of
`crates/binkd-net/src/caller.rs` holds the `$where` map of `on_error()`, and
the oracle and port cases of the call hold `error where=1`. The case
`the_on_log_hook_runs_on_the_master` of `crates/binkd/tests/perl.rs` holds the
decision that `on_log()` runs on the master interpreter. The case
`the_answerer_runs_on_handshake_after_the_adr` of
`crates/binkd-proto/tests/session.rs` holds the order of `on_handshake()` for
the answerer: the hook sees the caller's `@he`, and the answers replace our
address and the password. The oracle and port session cases hold `@he` with the
shared script. The case `the_action_of_after_sent_changes_the_spool` of
`crates/binkd-proto/tests/session.rs` holds `$action` of `after_sent()`:
`d` removes the file, `t` truncates it, and `s` removes it at the end of a
successful session. The oracle and port session cases hold `d` with the shared
script. The case `the_hook_sees_the_two_parts_of_the_message` of
`crates/binkd-proto/tests/session.rs` holds the `s1`/`s2` split of
`on_send()`. The oracle and port cases of the session hold the split with the
shared script. The cases `the_config_variables_reach_the_script` and
`the_config_variables_reach_a_session_clone` of
`crates/binkd-perl/tests/config.rs` hold `%config`, `%domain`, `@addr`,
`@listen`, `@ftrans`, `@overwrite`, `@skip`, `%share`, and `%node`, the main
interpreter and a session clone. The case
`the_oracle_of_the_perl_hook_of_the_start` and the port case
`the_port_of_the_perl_hooks_of_the_session` hold `$config{sysname}`.
The cases of `crates/binkd-perl/tests/xs.rs` hold the XS helpers `Log`,
`aeq`, `arm`, `msg_send`, and `binkd_warn`.

## The golden corpus

A corpus file is a text file with a `# crate:` header and the cases. The
header names the crate of the Rust workspace that owns the file.

A case line has these fields, separated by a tab:

```
<func> <arg>... => <want>
```

A field escapes these characters:

| Escape | Value |
| --- | --- |
| `\\` | Backslash |
| `\t` | Tab |
| `\n` | Newline |
| `\r` | Carriage return |
| `\xHH` | The byte `HH` |

The text `<null>` means a NULL value. Lines that start with `#` are
comments. Empty lines are ignored.

The Rust test runner sets these environment variables for the `%VAR%` cases
of `getword`:

- `BINKD_TEST_VAR=value`
- `BINKD_TEST_LOWER=lower`

The `ftn.tsv` file uses a fixed domain table and a fixed AKA. The table is
in the header of the file, and the Rust test uses the same table.

## The spool corpus

The spool cases live in `tests/golden/spool/`. A case sets up a spool, runs
the operations, and compares the output with the lines after the `---` line.
The test creates the case directory in a temporary directory, so the case
files hold relative paths only.

The lines before `---` are:

| Line | Meaning |
| --- | --- |
| `setup dir <path>` | Create a directory. |
| `setup file <path> <size> <mtime>` | Create a file of the size and the time. |
| `setup text <path> <content>` | Create a file with the content. |
| `setup binary <path> <hex>` | Create a file with the bytes. |
| `setup session <state> [<dir>]` | Set the session state and the directory. |
| `setup session aka <addr>` | Add one of the session addresses. |
| `setup session nfa <n>` | Set the number of the non-busy addresses. |
| `config domain <name> <dir> <z0>` | Add a domain. |
| `config node <addr> <hosts> <obox>` | Add a node. `-` is no box. |
| `config aka <addr>` | Add one of our addresses. |
| `config share <sha> <addr>...` | Add a share chain. |
| `config scalar <name> <value>` | Set a config value. |
| `op scan` | Scan the outbound and the fileboxes. |
| `op sort` | Sort the queue. |
| `op select <addr>` | Select the next file for the address. |
| `op sizes` | Sum the sizes of the files. |
| `op freq` | Count the requests. |
| `op read_try <addr>` | Read the `.try` file. |
| `op write_try <addr> <nok> <nbad> <comment>` | Write the `.try` file. |
| `op bad_try <addr> <error> <where>` | Count a bad attempt. |
| `op good_try <addr> <comment>` | Count a good session. |
| `op remove_try <addr>` | Remove the `.try` file. |
| `op hold <addr> <until>` | Write the `.hld` file. |
| `op process_hld <addr>` | Read the `.hld` file. |
| `op busy_add <addr> <flag>` | Create the busy file. |
| `op busy_test <addr> <flag>` | Test the busy file. |
| `op busy_remove <addr> <flag>` | Remove the busy file. |
| `op busy_remove_all` | Remove every busy file. |
| `op busy_touch` | Touch the busy files. |
| `op mtime <path>` | Print `now` or `old` for the modification time. |
| `op set_mtime <path> <time>` | Set the modification time. |
| `op setup_partial <name> <netname> <size> <time>` | Create partials. |
| `op setup_real <path> <size>` | Create the real file of a transfer. |
| `op inb_test <name> <size> <time>` | Test the inbound file. |
| `op inb_fopen <netname> <size> <time>` | Open the partial file. |
| `op inb_reject <netname> <size> <time>` | Remove the partial file. |
| `op inb_done <netname> <size> <time>` | Rename the partial file. |
| `op set_scalar <name> <value>` | Set a config value. |
| `op set_session <state>` | Set the session state. |
| `op dump` | Print the queue. |
| `op nodes` | Print the nodes. |
| `op files` | Print the files under the case root, sorted. |
| `op hex <path>` | Print the bytes of a file. |
| `op text <path>` | Print the text of a file. |
| `op exists <path>` | Print 0 or 1. |

The lines after `---` are the output of the operations:

| Line | Meaning |
| --- | --- |
| `queue <path> <flvr> <action> <type> <size> <sent>` | A queue entry. |
| `node <addr> <mail> <files> <busy> <hold>` | A node. |
| `file <path> <size>` | A file that remains. |
| `sizes <netsize> <filessize>` | The sum of the sizes. |
| `freq <n>` | The number of requests. |
| `select <path>` | The file that `select` returned. |
| `try <addr> <nok> <nbad>` | The values of the `.try` file. |
| `hex <path> <hex>` | The bytes of a file. |
| `text <path> <text>` | The text of a file. |
| `exists <path> <0\|1>` | The file exists. |

A case must not hold two queue entries with the same sort key. The upstream
C code keeps the `readdir()` order there, and that order is not the same on
every file system. The Rust port keeps the same order as the upstream C code.

The `inbound_minfree` case runs the non-negative `minfree` path. The focused
checks are:

```sh
cargo test -p binkd-spool inbound::tests
cargo test -p binkd-spool --test spool_corpus
cargo test -p binkd the_free_space
```

## The config corpus

The config cases live in `tests/golden/config/`. A case is a config file
`<name>.conf` and the expected `-d` dump `<name>.expect`.

The upstream C binary makes the expectations:

```
cd tests/golden/config
binkd -d <name>.conf > <name>.expect
```

The configs use relative paths, so the upstream C binary runs in the case
directory. The directories of the cases (`in`, `outb`, `box`, `inbox`,
`bbox`) are in the repository, and the `include` fragments live in `parts/`.

The Rust test `crates/binkd-config/tests/dump_corpus.rs` parses each case and
compares the dump with the expectation. When the `BINKD_C` variable names the
upstream C binary, the test also runs it and checks that the expectation is
current:

```
cd rust
BINKD_C=/path/to/binkd cargo test -p binkd-config --test dump_corpus
```

The config corpus holds successful `-d` dumps only. The partition and
missing-inbox rejection paths are unit tests in `binkd-config`; the corpus
format has no expected-error case.

## The reload corpus

A reload case is a config file `<name>.conf` and a changed variant
`<name>.changed`. The Rust test copies the corpus into a temporary directory,
loads the case, writes the variant over the case file, and runs the reload. It
compares the dump of the new config with `<name>.changed.expect`. The upstream
C binary made that expectation with `binkd -d <name>.conf` on the changed
file.

The test also checks that a check without the `-C` option does nothing, that a
second check sees no change, and that a broken file keeps the old config.

To check the reload of the upstream C daemon by hand:

```
cd tests/golden/config
binkd -c -C <name>.conf &        # client only, reload on change
# change <name>.conf, wait for the rescan delay, then stop the daemon
grep "Reloading configuration" <log>
```

## The interop tests

The interop tests run the upstream C binary and the Rust code against each
other over the loopback address, in both roles. They are the back-to-back
suite of the plan. Clone the upstream C reference, check out the pinned
commit, apply the patch, and build the plain reference with zlib and bzip2:

```sh
git clone https://github.com/pgul/binkd.git upstream-c
git -C upstream-c checkout 5a84e8596a96ca8b1f4ec69d9a608cd264aa06ba
git -C upstream-c apply "$PWD/ci/upstream-c.patch"
cd upstream-c
cp mkfls/unix/* .
chmod +x mkinstalldirs
./configure
make
```

Set `BINKD_C` to the path of the binary and run every test that starts the C
process:

```sh
cd rust
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-proto --test interop
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-net --test interop
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-proto --test record
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-config --test dump_corpus
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-config --test reload_corpus
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd --test cli
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd --test daemon
```

Without `BINKD_C` the tests report that they are skipped. Only the interop
cases of the `binkd-proto` file need the C binary; the Rust side is the
library, not the Rust binary. The cases of `crates/binkd/tests/engine.rs` hold
the two roles of the Rust library against each other.

The binary-level test `crates/binkd/tests/interop_binary.rs` starts the real
upstream C and Rust binaries. It runs the C server with the Rust client, the
Rust server with the C client, and the C server with the C client. The
generated configs fix `tzoff -3h`, and the test checks that the received `TIME`
line of the C server and of the C client carries `-0300`. The frame test of
the protocol interop suite holds the `NUL TIME` frame after `NDL` and before
`VER` in the C and the Rust sequence:

```sh
cd rust
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd --test interop_binary
```

The binary interop test accepts `BEGIN, binkd/` and the Windows
`BEGIN standalone, binkd/` line. It writes config paths with `/`,
because the config tokenizer reads `\` as an escape.

### The platform jobs

The `interop` job checks out the upstream C reference with
`.github/actions/upstream-c`, builds the plain binary, and runs the base
matrix and the binary-level test.

The `interop-macos` job checks out the upstream C reference with the action
and builds the UNIX binary on a macOS runner:

```sh
cd upstream-c
cp mkfls/unix/* .
chmod +x mkinstalldirs
./configure
make
cd ../rust
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-proto --test interop -- --test-threads=1
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-net --test interop -- --test-threads=1
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-proto --test record -- --test-threads=1
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-config --test dump_corpus
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-config --test reload_corpus
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd --test cli
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd --test daemon -- --test-threads=1
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd --test interop_binary -- --test-threads=1
```

The `interop-windows` job checks out the upstream C reference with the action
and builds it with MSYS2/MinGW64. From the `upstream-c` directory in a MINGW64
shell:

```sh
make -f mkfls/nt95-mingw/Makefile \
  CC=gcc ZLIB=1 BZLIB2=1 \
  OUTDIR=bin/mingw32-zlib-bzlib2 \
  CFLAGS='-std=gnu89 -mthreads -pipe -funsigned-char -Wall -Wno-char-subscripts -Wno-comment -O2 -s' \
  LFLAGS='-static'
```

The binary is
`upstream-c/bin/mingw32-zlib-bzlib2/binkd-mingw-zlib-bzlib2.exe`. Set
`BINKD_C` to that path and run the protocol, network, record, and
binary tests with `--test-threads=1`. Add
`C:\msys64\mingw64\bin` to `PATH` in the test step, so a toolchain
DLL is found if the link is not fully static. The command-line
`CFLAGS` replaces the makefile `CFLAGS +=` line, so the obsolete
`-mno-cygwin` flag is not passed. `LFLAGS='-static'` links zlib and
bzip2 into the executable.

The Windows job does not run `dump_corpus`, `reload_corpus`, `cli`,
or `daemon`: the C Windows build defines `AMIGADOS_4D_OUTBOUND`,
accepts `aso`, and has a different feature set from the Rust
Windows build. The Rust Windows parity tests stay in the `windows`
job.

### The special builds

The special builds stay on Ubuntu and use the action
`.github/actions/upstream-c`. `interop-proxy` builds the proxy
C binary and runs `proxy_interop`; `interop-bwlim` builds the
rate-limit C binary and runs
`rust_client_to_c_server_rate_limited`; `interop-v10` builds the
protocol 1.0 C binary and runs
`c_client_claims_freq_to_rust_server` and
`c_client_skips_the_request_of_rust_server`. The macOS and Windows
jobs leave `BINKD_C_PROXY`, `BINKD_C_BWLIM`, and `BINKD_C_V10`
unset, so those tests skip.

BSD is build-supported but not CI-tested. The upstream C reference and the
Rust workspace build on BSD, and no CI runner is configured. The
record `doc/decisions/platform-ci.md` holds the matrix.

The Rust test `crates/binkd-net/tests/proxy_interop.rs` runs the proxy matrix
for the network layer. Set `BINKD_C_PROXY` to the path of an upstream C binary
that the `configure` script built with `--with-proxy` and `--with-ntlm`:

```sh
cd upstream-c
cp mkfls/unix/* .
chmod +x mkinstalldirs
./configure --with-proxy --with-ntlm
make
cd ../rust
BINKD_C_PROXY=$PWD/../upstream-c/binkd cargo test -p binkd-net --test proxy_interop
```

Without `BINKD_C_PROXY` the tests report that they are skipped. The corpus
tests need the plain build of the upstream C binary, so keep the two builds
apart.

The Rust test `crates/binkd-proto/tests/interop.rs` runs a transfer of the port
against the C server with the rate limit. Set `BINKD_C_BWLIM` to the path of an
upstream C binary that the `configure` script built with `--with-bwlim`:

```sh
cd upstream-c
cp mkfls/unix/* .
chmod +x mkinstalldirs
./configure --with-bwlim
make
cd ../rust
BINKD_C_BWLIM=$PWD/../upstream-c/binkd \
    cargo test -p binkd-proto --test interop \
    rust_client_to_c_server_rate_limited
```

Without `BINKD_C_BWLIM` the test reports that it is skipped.

The interop test `c_client_claims_freq_to_rust_server` of
`crates/binkd-proto/tests/interop.rs` runs the C client of the protocol
version 1.0 against the listener of the port: the C client claims a FREQ, the
external program of the port answers the file of the exchange, and the port holds
its own end of the batch back until the C client answers the claim. Set
`BINKD_C_V10` to the path of an upstream C binary that the `make` command built
with the `PRTCLVER` define of the upstream `Config.h`:

```sh
cd rust
BINKD_C_V10=$PWD/../upstream-c-v10/binkd cargo test -p binkd-proto --test interop
```

The build of that binary, in a second clone of the upstream tree:

```sh
make CFLAGS='-O2 -DPRTCLVER=\"1.0\"'
```

Without `BINKD_C_V10` the test reports that it is skipped. The plain build
advertises the version 1.1, and the upstream C code holds its own end of the
batch back for a peer of the version 1.0 only, so the test needs the build of
the version 1.0.

The Rust test `crates/binkd-net/tests/srv.rs` runs the SRV lookup of the port
against a server of the test on the loopback address. The test needs no C binary,
because the upstream C code asks the resolver library of the system. The test
supplies the settings of the resolver, so it exercises the query, the records
of the answer, and the fallback.

The matrix of `doc/rust-rewrite-plan.md`:

| Client | Server | State |
| --- | --- | --- |
| Rust | C | plain, zlib, bzip2, and CRYPT |
| C | Rust | plain, zlib, bzip2, and CRYPT |
| C | C | the binary baseline |
| Rust | Rust | `crates/binkd-proto/tests/session.rs` |

The compression of a file has no corpus case: the bytes of a zlib or bzip2
stream depend on the version of the library. The unit tests of
`crates/binkd-proto/src/compress.rs` cover a round trip of both methods, and
the interop tests cover the C and the Rust streams against each other.

The upstream C client needs the poll file in the outbound directory of the
domain. `-P` alone adds the poll and starts the daemon, which never exits; pass
`-P <node>` and `-p` in one call so the daemon creates the poll, runs the
clients, and exits.

## The recorded sessions

The Rust test `crates/binkd-proto/tests/record.rs` reads the recordings of
`tests/sessions/`. A recording holds the bytes of the two directions of one
session: `s` for the bytes that the recorded side sent and `r` for the bytes that
it received. The test replays the bytes of the peer into a fresh session and
compares every byte that our side sends with the recording.

A recording holds a `# time` line after `# role`: the fixed banner time of
the session as `<year> <month> <day> <hour> <minute> <second>
<offset_minutes>`. The replay fills `BannerTime` from those seven values, and
the recorder that sends `TIME` writes the same value, so the frame and the
bytes stay deterministic.

The recordings come from the Rust session itself, one for each role, and each
covers the full batch: the banner, the login, the file of the caller, and the
`M_GOT` frame of the answerer. The test `write_the_recordings` writes them and
carries the `#[ignore]` attribute: run it after a change of the banner, of a frame,
or of the flow of the batch, then commit the files.

The replay of a live session of the upstream C binary runs with `BINKD_C`: the
test records the C client that polls the Rust server, then replays the bytes.
The answerer of that recording must hold the address that the C client calls,
because the upstream C code abandons a session of a remote without the called
AKA.

## Run the local time tests

The focused cases of the local clock, the `TIME` frame, and the binary log:

```sh
cd rust
cargo test -p binkd-log binlog::tests
cargo test -p binkd-ftn --test corpus
cargo test -p binkd-proto banner
cargo test -p binkd-proto --test session the_time_frame_is_sent
cargo test -p binkd-net the_banner_time
cargo test -p binkd --test cli the_version_date_is_local
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd-proto --test interop
BINKD_C=$PWD/../upstream-c/binkd cargo test -p binkd --test interop_binary
```

## Run the syslog tests

The focused cases of the log sink, the syslog backend, and the keyword. They
need no live syslog daemon:

```sh
cd rust
cargo test -p binkd-log a_sink_receives_the_level_and_text
cargo test -p binkd-log a_hook_rewrites_the_message_of_the_sink
cargo test -p binkd-log a_suppressing_hook_leaves_the_sink_empty
cargo test -p binkd-log a_nolog_mask_suppresses_the_sink
cargo test -p binkd-log a_level_above_the_file_level_skips_the_sink
cargo test -p binkd syslog::tests
cargo test -p binkd-config the_syslog_facility_is_parsed
cargo test -p binkd-config the_syslog_facility_of_auth_and_authpriv
cargo test -p binkd-config the_syslog_facility_is_case_sensitive
cargo test -p binkd-config the_mark_facility_is_rejected
cargo test -p binkd-config the_syslog_keyword_shows_the_c_fallback
```

The cases of `binkd-log` use a fake sink, and the cases of `binkd` cover the
facility and priority mapping. No live syslog daemon is required. No golden
case parses a `syslog <name>` line; the dump holds only the fallback, because
the upstream C parser `read_syslog_facility()` has an inverted argument check.
The Rust parser and dump cases are the only coverage of the keyword.

## What the corpus covers

| File | Crate | Cases |
| --- | --- | --- |
| `pmatch.tsv` | `binkd-ftn` | `xpmatch` |
| `getword.tsv` | `binkd-ftn` | `getwordx2` |
| `crypt.tsv` | `binkd-ftn` | ZipCrypto keys and buffers |
| `cram.tsv` | `binkd-ftn` | HMAC-MD5 and CRAM-MD5 |
| `ftn.tsv` | `binkd-ftn` | FTN addresses and domains |
| `tools.tsv` | `binkd-ftn` | The pure helpers of `tools.c` |
| `pkt.tsv` | `binkd-ftn` | Packet headers |
| `flo.tsv` | `binkd-spool` | Flow file lines |
| `spool/queue_scan.tsv` | `binkd-spool` | The queue scan and the sort |
| `spool/queue_points.tsv` | `binkd-spool` | Point directories |
| `spool/queue_zone.tsv` | `binkd-spool` | Zone directories |
| `spool/filebox.tsv` | `binkd-spool` | Fileboxes |
| `spool/bsy.tsv` | `binkd-spool` | `.bsy` and `.csy` files |
| `spool/try_hld.tsv` | `binkd-spool` | `.try` and `.hld` files |
| `spool/try_remove.tsv` | `binkd-spool` | `remove-try-files` |
| `spool/busy.tsv` | `binkd-spool` | `.bsy` and `.csy` flags |
| `spool/busy_touch.tsv` | `binkd-spool` | `bsy_touch` delay |
| `spool/inbound_test.tsv` | `binkd-spool` | `inb_test` and the rename styles |
| `spool/inbound_done.tsv` | `binkd-spool` | `inb_done` rename |
| `spool/inbound_reject.tsv` | `binkd-spool` | `inb_reject` and `inb_fopen` |
| `spool/inbound_minfree.tsv` | `binkd-spool` | `inb_fopen` and `minfree` |
| `spool/inbound_pkthdr.tsv` | `binkd-spool` | `check_pkthdr` |
| `config/minimal.conf` | `binkd-config` | The defaults of the `-d` dump |
| `config/full.conf` | `binkd-config` | The `-d` dump of the values |
| `config/styles.conf` | `binkd-config` | The `-d` dump of the styles |
| `config/reload_basic.conf` | `binkd-config` | The reload of the values |
| `config/reload_node.conf` | `binkd-config` | The reload of the node table |

The `tz_off` cases of `tools.tsv` cover the truncation toward zero for
positive and negative seconds values. The explicit offset path is exact; the
system offset for `tzoff -1` comes from the platform layer and the binary
log takes it as an argument.

The corpus does not cover the modules that need the network, the process
managers, or the config parser. See `doc/rust-rewrite-plan.md` for the
plan that adds those tests.

## Add a spool corpus case

1. Write the case in `golden/spool/`.
2. Run `cargo test` in `rust/` to check the case against the Rust code.

## Add a corpus case

1. Add the line to the file of the function in `golden/`.
2. Run `cargo test` in `rust/` to check the case against the Rust code.

A new corpus file needs the `# crate:` header. The Rust test of the crate
selects its cases by that name, so no case stays unchecked.

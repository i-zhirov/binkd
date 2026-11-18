# Decision: the Perl hooks

Status: approved. The port keeps the hooks in `binkd-perl`, one optional crate:
a Rust port of `perlhooks.c` over a small C shim for the Perl C API. The crate
alone holds the exception of the lint `unsafe_code`. A build of the port holds
the crate only with the feature `perl`, so the default build holds no FFI and no
`unsafe`.

## Context

`perlhooks.c` holds the hooks of the process and of the session, and the
`perl-hooks` and `perl-strict` directives name a script of the operator:
`perl_on_start()`, `perl_on_call()`, `perl_on_error()`, `perl_on_handshake()`,
`perl_after_handshake()`, `perl_after_session()`, `perl_before_recv()`,
`perl_after_recv()`, `perl_before_send()`, `perl_after_sent()`, `perl_on_log()`,
`perl_on_send()`, `perl_on_recv()`, `perl_setup_rlimit()`, `perl_need_reload()`,
`perl_config_loaded()`, and `perl_on_exit()`.

The upstream C code links the hooks only with `./configure --with-perl`, whose
default is no. The oracle of the interop cases is an upstream build with the
hooks: `./configure --with-perl` answers
`Compilation flags: clang, zlib, bzlib2, perl.` The former C test suite built
without the hooks, and no case of the corpus names a hook.

## Options

1. Link the C `perlhooks.c` into the Rust binary with the C ABI.
2. Port the hooks to a Rust binding of Perl.
3. Drop the hooks behind a build flag.
4. Port the hooks to Rust over a minimal C shim of the Perl C API.

## Decision

The port takes option 4: a Rust port of `perlhooks.c` over a minimal C shim of
the Perl C API.

The team first approved option 1. The implementation of the first hook showed
that the link pulls the C core, so the decision moved to option 4 before the
port of the hooks.

Option 1 is not possible as stated. `perlhooks.c` is not a module that the port
can link alone. It reads the C config, the session state, and the C log. It
calls `readcfg.c`, `protocol.c`, `ftnaddr.c`, `ftnq.c`, and `tools.c` through
the process-wide values `current_config`, `cfg->perl`, and `cfg->perl_ok`. A
link of the module pulls the C core and its global state into the Rust binary.
That contradicts the goal of the port and the rules of `AGENTS.md`.

The shim holds only what Rust cannot state:

- the macros of the stack of Perl: `dSP`, `PUSHMARK`, `SPAGAIN`, and `POPs`;
- the lifecycle of the interpreter: `PERL_SYS_INIT3`, `perl_alloc`,
  `perl_construct`, `perl_parse`, `perl_destruct`, and `perl_free`;
- the calls of the C API that the port reaches through the macros;
- the XS functions `Log`, `aeq`, `arm`, `msg_send`, and `binkd_warn`, which
  extract the arguments of Perl and call the Rust logic.

The hook logic stays in Rust and follows `perlhooks.c` function by function. The
C code remains the oracle.

Option 2 adds a dependency of the port of Perl. Option 3 breaks the config
compatibility of the operators who hold a hook. The shim of option 4 keeps the
config, the hook API, and the C oracle, and it adds no dependency.

The maintainer approved the exception of the lint for `binkd-perl`. `AGENTS.md`
holds the record of the approval and the port of the two rules: the crate
`binkd-perl` holds `unsafe_code = "allow"` of its own, and every other crate
keeps `forbid`. The crate is optional, and the default build of the workspace
holds it not.

## The first slice

The hooks that the C oracle exercises: no test of the C tree names a hook, so
the first slice must build an oracle of `--with-perl` and a script that a
config names. The hook of the log, `perl_on_log()`, is the first target: the
port holds the log as a value, so a test may hold the message and the level of
the hook without a process of Perl, and the oracle of `--with-perl` holds the
same hook over the script.

The first slice is done. `binkd-log` holds the trait `Hook` and the method
`Log::set_hook()`, the port of the call of `perl_on_log()` in `vLog()`.
`binkd-perl` holds the port of `perl_on_log()`, and `binkd` loads the script and
installs the hook in the startup path of the binary.

The XS helpers are done too: `Log`, `aeq`, `arm`, `msg_send`, and
`binkd_warn`. The shim registers them with `xs_init`, and it sets `binkd_warn`
as the warn and die hook, the port of `perl_init()`. The helper `msg_send`
holds a `MessageSink`: the session of `binkd-proto` supplies the queue of the
binkp messages, and the binary supplies no sink until the session slice.

## The callback hooks

The callback hooks are done. `binkd-perl` holds the typed values of the
session, the call, the error, the files, and the rate limit, and the hook
methods of `perlhooks.c`. The methods set the Perl variables, call the sub, and
apply the answer of the C code: the abort text of the handshake, the skip of a
file, the new name of a received file, the new name of a sent file, the action
of a sent file, and the rate limit.

`binkd-proto` holds the trait `Hooks` and the values of the trait. The session
calls the trait at the C call points: the handshake, the files, the messages,
the rate limit, and the end of the session. `run_with_hooks()` carries the
hooks; `run_with()` passes `None`, so every case that names no hook stays the
same. The binary implements the trait over the interpreter with the adapter
`ProtoHook`, and `LogHook` shares the same interpreter, so one mutex protects
the calls.

`binkd-net` calls `on_call()` and `on_error()` in the caller. `binkd` calls
`on_start()`, `on_exit()`, and `config_loaded()` at the startup and the end,
and `need_reload()` in `Reloader::check()`. The binary sets the features of the
parse of a reload, so the `perl-*` keywords pass.

The session supplies the sink of `msg_send()` to its clone, so a script may
queue a binkp message from a hook and the session sends it after the hook, the
port of `msg_send2()`. The case `a_hook_message_reaches_the_peer` of
`crates/binkd-proto/tests/session.rs` holds the path.

The session values are done. `SessionInfo` carries the host and the address of
the peer and our address and port, the session holds the start, and the
`M_NUL` frames of the peer set `$sysname`, `$sysop`, and `$location`. The
`%opt` hash holds `ND`, `NR`, `MD`, `crypt`, and `GZ`. The cases
`the_session_variables_reach_the_script` and
`the_session_values_reach_the_hook` hold the values.

The traffic and compression values are done. The caller fills the queue sizes
of the session and the session sums the files of the answerer; the send and
receive methods set `$z_send` and `$z_recv`. The cases
`the_traffic_of_the_files_reaches_the_hook`,
`the_compression_counters_reach_the_hooks`, and
`the_traffic_and_compression_variables_reach_the_script` hold the values.

`@queue` is done for reading and writing. The session builds one hash for
every file of the session with `file`, `size`, `time`, `sent`, `flvr`,
`action`, `type`, and `addr`. The hook may change the hashes, add an entry, or
remove an entry. The port of `refresh_queue()` reads the array back, and the
send path rebuilds the files of the session from the entries with the port of
`queued_file()`. The hooks of `after_handshake()`, `before_send()`, and
`after_recv()` refresh the queue, as in the C code. `before_recv()` and
`after_sent()` do not refresh the queue, so a change of `@queue` there has no
effect. The return value of `before_recv()` accepts or skips the incoming file.
The cases
`the_session_queue_reaches_the_script`, `the_script_rebuilds_the_session_queue`,
`the_script_removes_an_entry_from_the_session_queue`,
`the_script_removes_an_entry_of_after_recv`,
`the_traffic_of_the_files_reaches_the_hook`,
`the_hook_rebuilds_the_session_files`,
`the_hook_removes_a_file_from_the_session`,
`the_hook_removes_a_file_of_after_recv`,
`the_session_does_not_refresh_the_queue_of_before_recv`,
`the_script_deletes_an_entry_of_before_recv`,
`the_session_does_not_refresh_the_queue_of_after_sent`,
`the_script_deletes_an_entry_of_after_sent`,
`the_port_of_the_rebuilt_session_queue`,
`the_port_of_the_removed_file_of_the_session_queue`,
`the_port_of_the_removed_file_of_after_recv`,
`the_port_of_the_queue_of_before_recv`, and
`the_port_of_the_queue_of_after_sent` hold the queue.

The cases of `crates/binkd-perl/tests/hooks.rs` call every hook and hold the
mark of the script, the return values, the abort text, the new file names, the
action, the rate limit, and the default of a missing sub. The port cases of
`crates/binkd/tests/perl.rs` hold the call hooks and the reload, and the
session end to end.

The state of the hooks is per thread or per interpreter: the recursion guards
of the log and of the log hook are thread-local, the context of the helpers is
thread-local, the master interpreter is behind a mutex, and every session holds
a clone. The only process-wide state is the one-time start of the Perl
library.

The sessions run on clones of the master interpreter, the port of
`perl_init_clone()` and `perl_done_clone()`. `Hooks::clone_for_session()` calls
`perl_clone`, and the clone holds its own globals: the subs and the script
stay shared, the values of the script do not. `binkd_proto::Factory` gives the
hooks of one session, and the listener and the caller hold one clone per
session. The log hook stays on the master interpreter. The cases
`two_sessions_do_not_share_the_variables` and
`the_port_of_the_perl_clones_of_two_sessions` hold the separation of the
values.

The config variables of `perl_setup()` are done. The port sets `%config`,
`%domain`, `@addr`, `@listen`, `@ftrans`, `@overwrite`, `@skip`, `%share`, and
`%node` on the master interpreter before the body of the script runs, and on
every session clone, as the C code does. The names, the values, and the
optional keys follow the C code: `VK_ADD_HASH_int` leaves a zero value out, and
`VK_ADD_HASH_str` writes an undefined value for a missing string. The cases
`the_config_variables_reach_the_script` and
`the_config_variables_reach_a_session_clone` of
`crates/binkd-perl/tests/config.rs` hold every variable, and the oracle and
port cases of `on_start()` hold `$config{sysname}`.

The `action` of `after_sent()` is done. The port reads `$action` back when the
hook answers a nonzero value, as the C code does, and it applies the action to
the file of the spool: `d` removes the file, `t` truncates it, and `s` removes
it at the end of a successful session. The case
`the_action_of_after_sent_changes_the_spool` of
`crates/binkd-proto/tests/session.rs` holds the three actions, and the oracle
and port session cases hold `d` with the shared script.

The answerer side of `on_handshake()` is done. The caller runs the hook in its
banner, the port of `banner()`, before its `ADR`. The answerer runs the hook
after the `ADR` of the caller, the port of `ADR()`, so `@he` holds the caller's
addresses, and it sends its own `ADR` after the hook, the port of `delay_ADR`.
The hook may replace our addresses and the password then. The case
`the_answerer_runs_on_handshake_after_the_adr` of
`crates/binkd-proto/tests/session.rs` holds the order, and the oracle and port
session cases hold `@he` with the shared script.

The `s1`/`s2` split of `on_send()` is done. The port sends the messages that
the C code builds from two strings with `send_text2()`, so the hook reads
`$s1` and `$s2` apart, and the wire holds their concatenation. The cases
`the_hook_sees_the_two_parts_of_the_message` of
`crates/binkd-proto/tests/session.rs`, and the oracle and port cases of the
session hold the split.

The reload of the script is done. `Daemon::reload_check()` loads the script of
the new config, calls its `config_loaded()`, and applies the new hooks to the
listener and the client manager, the port of `perl_init(new_config, 0)` and of
`perl_config_loaded()` of `checkcfg()`. It does not call `on_start()`, and an
old script gets no second `config_loaded()`. A new config without a script
drops the hooks. A strict build keeps the old config when the new script fails.
The case `the_port_of_the_reload_of_the_script` of
`crates/binkd/tests/perl.rs` holds the whole path.

The last gaps of the port are closed:

- `after_recv()` runs **before** the rename, the port of the
  `perl_after_recv()` call of `inb_done()`. `$tmpfile` names the partial file
  and `$file` the real name. The hook may change `$file`, and the session uses
  the new name; a result of 2 discards the partial file. The case
  `the_hook_of_after_recv_runs_before_the_rename` of
  `crates/binkd-proto/tests/session.rs` holds the order and the new name, and
  the case `the_after_recv_values_reach_the_script` of
  `crates/binkd-perl/tests/hooks.rs` holds the two values.
- `$where` of `on_error()` holds the `BAD_*` value of the call, the port of
  `enum bad_try_type` of `ftnq.h`. `binkd-proto` exports the values, and the
  caller maps the failed call: a connection failure is `BAD_CALL`, a busy peer
  is `BAD_MBSY`, a peer `M_ERR` or a protocol failure is `BAD_MERR`, an I/O
  failure or a timeout is `BAD_IO`. The case
  `the_where_of_the_error_follows_the_c_code` of
  `crates/binkd-net/src/caller.rs` holds the map, and the oracle and port
  cases of the call hold `error where=1` with the shared script.
- `on_log()` runs on the **master** interpreter, not on the clone of a session.
  The log of the daemon is process wide, and the adapter of the log serializes
  the calls, so the hook has one interpreter and one set of globals. The C
  code runs the interpreter of the current thread; the port chooses the master
  because a log line may come from any thread and a session clone belongs to
  one session. The case `the_on_log_hook_runs_on_the_master` of
  `crates/binkd/tests/perl.rs` holds the choice: a session hook sets a global
  and logs, and the hook sees the master value.

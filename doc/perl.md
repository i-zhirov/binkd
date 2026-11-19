# Perl hooks

The Perl hooks are an optional integration with the system Perl
interpreter. They are the port of `perlhooks.c` of the upstream C
implementation. A Perl script can change the session, the queue, the log
lines, and the outgoing messages.

The hooks are not in the default build. The split keeps the default binary
free of the Perl C API, the C shim, and the interpreter dependency.

## Build

The `rbinkd` crate holds the optional feature `perl`:

```sh
cargo build --release -p rbinkd --features perl
```

The feature adds the optional dependency `rbinkd-perl`. The crate
`rbinkd-perl` is excluded from the workspace. Thus `cargo build --workspace`
and `cargo test --workspace` do not compile it.

The build needs these items:

- a C compiler
- the system `perl`
- the Perl development files and headers
- the Perl module `ExtUtils::Embed`

`build.rs` asks Perl for the compiler and linker flags. It compiles
`crates/rbinkd-perl/src/shim.c`, archives the object as
`librbinkd_perl_shim.a`, and links the binary with the Perl library. On
macOS, it also uses the SDK path from `xcrun`. The CI job `perl` installs
`libperl-dev` on Ubuntu and runs the tests below.

## Unsafe boundary

The workspace forbids `unsafe` code. Two crates hold the only exceptions:

- `rbinkd-win` uses `unsafe` for the Windows service and file-system FFI.
  It is a target-specific dependency and compiles on Windows only.
- `rbinkd-perl` uses `unsafe` for the Perl C API. It is the optional crate
  of this document.

Perl is therefore not the only crate that uses `unsafe`. It is the only
*optional* crate that uses `unsafe`.

The boundary of `rbinkd-perl` is the C API of Perl. The Rust code holds raw
`PerlInterpreter` and `SV` pointers. It calls the functions of `shim.c`, and
`shim.c` calls the exported Rust helpers `rbinkd_perl_xs_*`. The shim owns
the Perl stack macros and the XS functions. The crate also holds
`unsafe impl Send for Hooks`, because one interpreter belongs to one thread
at a time. Locks and per-session values keep that rule.

Do not add `unsafe` outside these two exceptions. A change to the Perl
crate must keep the raw pointers and the calls of the shim in the FFI
layer.

## Runtime model

The process starts the Perl library once with `PERL_SYS_INIT3`. It loads
the script into a master interpreter. The script defines the hooks, and the
master holds the process values.

Each session receives a clone of the master. A clone has its own globals,
so two sessions do not share `$host`, `@queue`, or other script variables.
A clone uses the script, the subs, and the config of the master. The
interpreter of a value belongs to one thread at a time. The adapter shares
a value between threads only through a mutex.

The master runs `on_start()` when the process starts. The daemon calls
`config_loaded()` after it loads the script. `on_exit()` runs at shutdown.
The log hook `on_log()` runs on the master. `LogHook` serializes its calls
with a mutex, and a thread-local guard prevents a recursive call. The other
hooks run on the clone of the session.

`on_log()` can replace the message and the level, or suppress the line. A
script without a hook keeps the default behavior. A session clone queues a
binkp message with `msg_send()`, and it writes a line with `Log()` or
`binkd_warn()`.

The interpreter and the clone are freed when their values drop. The
cleanup sets the Perl context first, so a `DESTROY` method and a warning
see the right values.

## Configuration

A build with the feature accepts these directives:

| Directive | Meaning |
| --- | --- |
| `perl-hooks file` | The Perl script of the process |
| `perl-strict` | Stop the start when the script fails |
| `perl-var name value` | A value in `%config` |
| `perl-dll file` | Accepted and dumped for compatibility |

The Rust build links Perl at build time. It does not load the file of
`perl-dll` at run time.

A default build rejects these names as unknown keywords. The `-d` dump of
a default build omits them, and the dump of a feature build includes them.
Comment the `perl-*` lines out before you use a config with a default
build. The UNIX sample has active `perl-hooks` and `perl-var` lines.

A missing or broken script stops the start with code 1 when the config has
`perl-strict`. Without `perl-strict`, the daemon logs the error and
disables the hooks. On reload, a broken strict script keeps the old
config. Without `perl-strict`, the reload removes the hooks. A script
without a hook keeps the default behavior.

The script can define these subs:

`on_start`, `on_exit`, `on_call`, `on_error`, `on_handshake`,
`after_handshake`, `after_session`, `before_recv`, `after_recv`,
`before_send`, `after_sent`, `on_log`, `on_send`, `on_recv`,
`setup_rlimit`, `need_reload`, and `config_loaded`.

The globals `%config`, `%domain`, `@addr`, `@listen`, `@ftrans`,
`@overwrite`, `@skip`, `%share`, and `%node` hold the config. The session
globals include `$host`, `$ip`, `$sysname`, `%opt`, and `@queue`.

The script can call the helpers `Log`, `aeq`, `arm`, and `msg_send`. The
warn and die hook keeps the name `binkd_warn` for compatibility. The
upstream `perlhooks.txt` documents the variables and the return values of
the hooks.

## Example

This short script writes one line at start and prefixes every log line.
Save it as `hooks.pl` and name it in the config:

```perl
# Save as hooks.pl and set `perl-hooks /path/to/hooks.pl`.
sub on_start {
    Log(3, "example: started as $config{sysname}");
    return 1;
}

sub on_log {
    s/^/example: /;
    return 1;
}

1;
```

The `perl-hooks` line of the config points to the file:

```text
perl-hooks /path/to/hooks.pl
```

Build with the `perl` feature. The case `the_doc_example_runs` extracts
this block and runs it, so the example must stay runnable.

## Tests

The default test suite does not build the Perl crate. Run the two optional
suites on a system with the Perl development files:

```sh
cargo test -p rbinkd --features perl -- --test-threads=1
cargo test --manifest-path crates/rbinkd-perl/Cargo.toml -- --test-threads=1
```

The first command tests the binary with the hooks. The second command tests
the crate, the C shim, and the example of this page. The script
`tests/perl/hooks.pl` defines every hook. The cases of
`crates/rbinkd/tests/perl.rs` compare the port with the upstream C binary
when `BINKD_C_PERL` names a build of `./configure --with-perl`.

The CI job `perl` installs `libperl-dev`, runs both suites, builds the C
oracle with `--with-perl`, and runs the binary suite against it.

## Limitations

- The default build rejects the `perl-*` directives.
- The Rust build links Perl at build time and ignores `perl-dll` at run
  time.
- The script runs in the daemon process with its privileges. The
  integration does not sandbox the script.
- One interpreter belongs to one thread at a time. The adapter uses locks
  and per-session clones, so two sessions do not share the interpreter
  state.
- The `perl` CI job runs on Ubuntu. A local build needs the same Perl
  development files on its system.

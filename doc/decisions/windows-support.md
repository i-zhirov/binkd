# Decision: Windows support

Status: accepted and implemented. The Rust port supports Windows NT and later.
The Windows service mode is in scope. The tray icon is out of scope. The team
accepted the record, and the `binkd-win` crate holds the service mode and the
safe file-system helpers of the Windows build.

## Context

The C code holds the Windows layer in `nt/` and in the `WIN32` blocks of
`binkd.c`:

- `nt/service.c`: the service control manager. It holds install, uninstall,
  start, stop, restart, and status, the control events, and the service
  cleanup.
- `nt/w32tools.c`: the service name, the arguments, the registry, and the
  icon.
- `nt/breaksig.c`: the `CTRL_SERVICESTOP_EVENT` and
  `CTRL_SERVICERESTART_EVENT` events.
- `nt/win9x.c`: the Windows 9x service.
- `nt/tray.c`: the tray icon.
- `nt/WSock.c`, `nt/dirwin32.c`, `nt/getfree.c`, `nt/sem.c`: the sockets, the
  directory, the free space, and the semaphores.

`binkd.c` holds the `-t` commands, the `-S` name, the `-T` switch, the call of
`service()`, the `tell_start_ntservice()` check, and the `BEGIN service` line.

The Rust workspace targets UNIX and Windows. `std` and the crates `binkd-net`
and `binkd-log` cover the sockets, the files, and the threads of both. The
service mode is the Windows layer; the `binkd-win` crate holds it.

## Decision

1. Windows NT and later is a supported target of the Rust port. Windows 9x/Me is
   out of scope, because the systems hold no current toolchain of the workspace
   and the C layer of `nt/win9x.c` is a separate service model.
2. The Windows service mode is in scope. The port holds the `-t` commands
   (`install`, `uninstall`, `start`, `stop`, `restart`, and `status`), the `-S`
   name, the service name that the C code derives from the display name, the
   records of the config path and the arguments in the registry, the status
   reports to the service control manager, the stop and restart control events,
   the cleanup on the service end, and the `BEGIN service` line.
3. The tray icon is out of scope. The port holds no `nt/tray.c`, no icon, and no
   `-T` switch. The command line of the port answers the unknown switch error
   for `-T`, and the release notes state the difference.
4. The service mode lives in a new crate, `binkd-win`. The crate is a
   target-specific dependency of `binkd` and compiles on Windows only. It holds
   the safe Rust API of the service control manager over the `windows-sys` crate
   with an exact version. The crate alone holds `unsafe_code = "allow"` for the
   FFI of the Windows API; the rest of the workspace keeps `forbid`. This is the
   second exception of the workspace, beside `binkd-perl`, and the record
   `doc/decisions/perl-hooks.md` is the first.
5. The `binkd` binary of Windows uses the crate. The binary parses `-t` and
   `-S` before the config, answers the service commands, and, when the service
   control manager starts it, runs the daemon and reports the service state. A
   service stop or restart reaches the daemon as the exit or the hangup event,
   the port of the `CTRL_SERVICESTOP_EVENT` and `CTRL_SERVICERESTART_EVENT` of
   `nt/breaksig.c`. The service cleanup removes the busy flags and the pid file
   and ends the interpreter, the port of `exitfunc()`.
6. The C code of `nt/` stays in the tree as the reference of the port, the
   decision of `doc/decisions/legacy-platforms.md`. The port does not link the C
   service module.
7. The pure parts of the service mode get unit tests: the parse of `-t` and
   `-S`, the service command, and the service name. The service control manager
   itself has a Windows CI job that runs the
   [smoke test](#windows-scm-smoke-test). The C code stays the oracle of the
   service commands by its source, and the release notes state the smoke test
   result.
8. The release notes state the Windows differences: no tray icon, no `-T`
   switch, and no Windows 9x/Me.

## Implementation order

1. The `-t` and `-S` switches and the service name in `binkd-win`, the port of
   the Windows part of `parseargs()`.
2. The service control manager wrapper: open the manager, install, uninstall,
   start, stop, restart, query the status, and the service dispatcher.
3. The service main of the `binkd` binary: the daemon start, the status
   reports, and the stop and restart events.
4. The registry records of the config path, the arguments, and the environment,
   the port of `store_data_into_registry()`.
5. The Windows CI job and the service smoke test. The job builds the release
   binary and runs the [smoke test](#windows-scm-smoke-test) below.

## Status

The crate `binkd-win` holds the pure command values and the service dispatcher,
the port of `nt/service.c` and of the Windows part of `nt/w32tools.c`. It also
holds the safe path and volume wrappers of the parity work. The `binkd` binary
parses `-t` and `-S` before the config, installs the service with the program
and the arguments in the registry, and reports the service state. The Windows
job of `.github/workflows/ci.yml` runs the check, the clippy, the workspace
tests, the release build, and the [smoke test](#windows-scm-smoke-test) below.
The target-gated parity tests are in the same workspace suite.

## Parity of the Rust build

The Windows Rust build also ports the Windows behavior outside the service
control manager:

- The partition check of `check_config()` reads the volume identity through
  the `Partition` provider of `binkd-config`. The Windows binary supplies the
  safe `binkd-win::storage::volume_serial()` wrapper; Unix reads the device
  number. `binkd-config` holds no `binkd-win` dependency.
- The `*N` macro of `run_args()` asks the `ShortName` lookup of the SRIF
  expansion. The Windows binary supplies
  `binkd-win::storage::short_path()`; the other systems answer no lookup. A
  failed lookup logs `GetShortPathName() fails <code>` and falls back to the
  validated name, as the C code does.
- The parser, the reloader, the dump, and the binary use one `Features` set.
  The Windows set accepts the proxy, socks, rate-limit, and compression
  keywords. It rejects `aso`, `syslog`, `zlib-dll`, and `bzlib2-dll`.
- The `windows-sys` file-system feature holds the `GetShortPathNameW()` and
  `GetFileInformationByHandle()` calls. The unsafe calls stay in `binkd-win`.

The C Windows build differs in these points:

- It defines `AMIGADOS_4D_OUTBOUND` and accepts the `aso` keyword; the Rust
  port rejects it.
- It has no syslog backend; the Rust port also rejects `syslog`.
- It accepts `limit-rate` only with `BW_LIM`; the Rust Windows build accepts
  it.
- It accepts proxy and socks with `HTTPS`; the Rust Windows build accepts them
  too.
- It accepts the compression keywords only with `WITH_ZLIB` or `WITH_BZLIB2`;
  the Rust Windows build accepts them.
- It can load zlib and bzip2 dynamically with `ZLIBDL`; the Rust port cannot.
- Its `stat()` `st_dev` is the drive number, while the Rust provider uses the
  volume serial. The common case is the same; a path on the same volume
  compares equal either way.

## Windows SCM smoke test

The script `tests/windows-smoke.ps1` runs the service checks on a Windows
runner in CI. The `windows` job of `.github/workflows/ci.yml` builds the
release binary and runs the script. The script creates a temporary config and
a unique service, runs the checks, and removes the service and the temporary
directory.

Run the script on a local Windows host in an administrator PowerShell:

```powershell
cd rust
cargo build --release -p binkd
cd ..
pwsh -NoProfile -ExecutionPolicy Bypass -File tests/windows-smoke.ps1
```

The script checks:

- Install: the command returns 0, the service is running, the binary path is
  `binkd.exe`, and the registry working directory is the test directory.
- Status: the running, stopped, and not-installed messages.
- Stop: the service is stopped, the process is gone, the busy and pid files
  are absent, and the log holds `exitfunc()`.
- Start: the service starts, and a direct `sc.exe stop` and `sc.exe start`
  stop and start the service.
- Restart: the service is running again.
- Uninstall: the service is absent, the status prints the not-installed
  message, and the process is gone.
- Log output: the log holds at least four `BEGIN service` lines and four
  `exitfunc()` lines, no `BEGIN standalone`, no `FATAL`, and no
  `error in configuration`.
- Console output: each `-t` command prints its expected message.

The service name is unique. The script uses `-c` to run the client role only,
so the daemon binds no port. The test config sets `loglevel 8`, an absolute
log path, a pid file, and the spool and inbound directories.

The script stops and removes the service when a check fails. The Windows job
fails when the script exits with a nonzero code. Record the Windows version,
the binary version, and the result in the release notes.

## Consequences

- The Rust binary is the default on Windows NT and later, the same as on UNIX.
- The tree keeps the C Windows service and tray code as the reference, and the
  `nt/` layer stays for the platforms that the C code alone supports.
- The workspace holds a second exception of `unsafe_code` for the Windows FFI.
  The crate is small, and a review of the crate covers the whole unsafe surface.
- A user of the C `-T` switch and of the tray icon gets a known difference. The
  release notes name it.
- The Windows build accepts the proxy and rate-limit keywords, and refuses
  `aso`, `syslog`, `zlib-dll`, and `bzlib2-dll`. The record names the C
  differences above.
- Windows 9x/Me stays a C-only platform, the decision of
  `doc/decisions/legacy-platforms.md`.

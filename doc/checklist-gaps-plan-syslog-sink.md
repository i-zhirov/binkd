# Syslog Sink Implementation Plan

> Execute this plan with the `executing-plans` skill. Each task is a
> checkbox; mark it done when its changes are implemented and its
> Verify command passes.

- **Date**: 2026-11-16
- **Status**: Implemented
- **Design**: [rust-rewrite-plan.md](rust-rewrite-plan.md)
- **Roadmap**: [checklist-gaps-roadmap.md](checklist-gaps-roadmap.md) —
  slice: `syslog-sink`
- **Depends on**: —
- **Manual prerequisites**: —
- **Goal**: add a platform-free log sink to `binkd-log`, a Unix syslog
  backend in the binary, the C priority mapping, and the `syslog`
  facility from the config.

## Constraints

- `binkd-log` holds the `Sink` boundary and no platform FFI. The Unix
  backend is in the `binkd` binary.
- The sink receives the final message and the final level after the Perl
  hook and after the `nolog` filter.
- The sink runs only when the level is at most `loglevel`, the port of
  the `current_loglevel` condition in `vLog()`.
- The C priority mapping is exact:
    - level 0 -> `LOG_ERR`;
    - level 1 -> `LOG_WARNING`;
    - level 2 -> `LOG_NOTICE`;
    - levels 3 to 6 -> `LOG_INFO`;
    - all other levels -> `LOG_DEBUG`.
- The process name is `binkd`. The syslog record holds the process id,
  the port of `LOG_PID` of `openlog()`.
- A `syslog` facility value below zero disables the sink. The C
  `syslog_facility` default is `-1`.
- Use `syslog = "=7.0.0"` as a Unix-only dependency of the `binkd`
  crate. Do not add it to a pure crate or to the Windows build.
- A failed local syslog connection disables the sink. The daemon stays
  alive and the text log still works.
- The C parser `read_syslog_facility()` has an inverted argument check,
  so the C binary rejects a correct `syslog <name>` line. The port
  accepts it. A golden C oracle case cannot cover this line; use a Rust
  parser and dump test.
- The C `vLog()` passes the original format string and `va_list` to
  `vsyslog()`, not the formatted `buf`. The port has no format string
  at this layer, so the sink receives the filtered text. This is the same
  text as the console and the file. Record this difference in the
  design and flag it in the handover.
- The `syslog` keyword stays in the config keyword table of the Unix
  feature set. The Windows binary has no backend.
- No new CI job. The Linux, macOS, and Windows workspace jobs run the
  new tests.
- No transient plan, design, roadmap, task, requirement, or slice
  references in code, identifiers, test names, strings, or comments.

## Open Risks And Dependencies

- **Dependency:** `syslog = "=7.0.0"` adds `hostname`, `libc`, `log`,
  and a `time` requirement to the Unix binary. The crate declares no
  `rust-version`; verify the 1.85 MSRV.
- **Live backend:** the tests use a fake sink. A test host may have no
  `/dev/log` or `/var/run/syslog`, so the backend is tested by its
  facility and priority helpers, not by a live daemon.
- **C parser:** the inverted check in `read_syslog_facility()` prevents
  a C oracle case for `syslog`. The Rust parser and dump test are the
  only coverage.
- **C message text:** the C syslog branch sends the original format
  string and arguments, while the hook and the file see the formatted
  buffer. The port sends the formatted, filtered text to every output.
- **Record header:** the `syslog` crate formats an RFC 3164 header. On
  Unix its formatter uses UTC and a different day width from the C
  `vsyslog()` local header. The priority, facility, process, and text
  are the contract; the timestamp header is a recorded difference.
- **Reload:** the C `openlog()` runs once, so a reload that changes the
  facility keeps the first facility. The port recreates the sink on
  reload. A failed new connection keeps the old sink; a disabled
  facility clears it.
- **Silent fallback:** a failed `syslog::unix()` connection disables the
  first sink without a log message, to avoid recursion through the log.
- **Parser table:** the C `facilitynames` table has `ftp`, `security`,
  and `mark`. The port adds `ftp` and `security`; `mark` has no
  `syslog::Facility` variant, so the port rejects it with a clear
  error.

## Tasks

- [x] **Task 1: Add the log sink boundary**
    - **Implements:** design Phase 5 `binkd-log`; the `vLog()` syslog
      branch of `tools.c`; roadmap `syslog-sink`
    - **Modify `rust/crates/binkd-log/src/log.rs`**
        - Add `pub trait Sink: Send + Sync` with
          `fn write(&self, level: Level, text: &str)`. The trait and its
          method have `///` docs naming the `vLog()` syslog branch.
        - Add `sink: RwLock<Option<Arc<dyn Sink>>>` to `Log`.
        - Add `pub fn set_sink(&self, sink: Option<Arc<dyn Sink>>)` with
          a doc comment.
        - Add a private `fn sink(&self) -> Option<Arc<dyn Sink>>`.
        - Call the sink after the console and file output, inside the
          `!suppressed` branch, when `level <= inner.level`. Clone the
          sink and drop the `inner` guard before the call, so a sink
          cannot deadlock the log.
        - The sink call is before the `Outcome::Fatal` return.
        - Add these tests in the module:
            - `a_sink_receives_the_level_and_text` — a fake sink records
              `(INFO, "hello")`.
            - `a_hook_rewrites_the_message_of_the_sink` — the hook changes
              the text and the level; the sink records the changed values.
            - `a_suppressing_hook_leaves_the_sink_empty` — a hook that
              answers false leaves the sink empty.
            - `a_nolog_mask_suppresses_the_sink` — a matching mask leaves
              the sink empty.
            - `a_level_above_the_file_level_skips_the_sink` — the log
              level is `INFO`, a `DEBUG` message leaves the sink empty.
        - The fake sink is a local `Mutex<Vec<(Level, String)>>` value.
    - **Modify `rust/crates/binkd-log/src/lib.rs`**
        - Re-export `Sink` with the other log items.
    - **Verify:** `cd rust && cargo test -p binkd-log`.

- [x] **Task 2: Add the Unix syslog backend**
    - **Implements:** design Phase 6 and the `vLog()` syslog branch;
      roadmap `syslog-sink`
    - **Modify `rust/crates/binkd/Cargo.toml`**
        - Add `[target.'cfg(unix)'.dependencies]` with
          `syslog = "=7.0.0"`.
        - Update `rust/Cargo.lock`.
    - **Create `rust/crates/binkd/src/syslog.rs`** with `#[cfg(unix)]`
      at the module boundary in `lib.rs`:
        - `pub(crate) fn configure(log: &Log, facility: i32)` — a
          negative value calls `log.set_sink(None)`. A non-negative
          value builds a new sink when `SyslogSink::new(facility)`
          answers `Some`. A failed new connection keeps the old sink.
        - `fn facility(value: i32) -> Option<syslog::Facility>` — map
          the C facility values: 0 kern, 8 user, 16 mail, 24 daemon,
          32 auth, 40 syslog, 48 lpr, 56 news, 64 uucp, 72 cron,
          80 authpriv, 88 ftp, 96 ntp, 104 audit, 112 alert, 120 clock,
          128 to 184 local0 to local7. Other values are `None`.
        - `fn priority(level: Level) -> syslog::Severity` — the exact
          mapping in Constraints.
        - `fn write_severity<W: std::io::Write>(
          logger: &mut syslog::Logger<W, syslog::Formatter3164>,
          severity: syslog::Severity, text: &str)` — one exhaustive
          match on `Severity`; it calls the `err`, `warning`, `notice`,
          `info`, or `debug` method and ignores its result.
        - `struct SyslogSink` with
          `Mutex<syslog::Logger<syslog::LoggerBackend,
          syslog::Formatter3164>>`.
        - `SyslogSink::new(facility: i32) -> Option<SyslogSink>` —
          build `Formatter3164 { facility, hostname: None,
          process: "binkd".into(), pid: std::process::id() }` and call
          `syslog::unix(formatter)`. Answer `None` on error.
        - `impl Sink for SyslogSink` — lock the logger, call
          `write_severity` with `priority(level)`, and return when the
          lock is poisoned. The priority mapping is used on the write
          path.
        - Unit tests:
            - `the_facility_of_a_config_value` — 0, 8, 24, 32, 80, 128,
              and 184 map to the expected variants; 200 is `None`. Use
              `matches!` because `Facility` has no `PartialEq`.
            - `the_priority_of_a_level` — every branch of the C table,
              with `matches!` because `Severity` has no `Debug` or
              `PartialEq`.
            - `the_write_path_uses_the_priority` — a
              `Logger<Vec<u8>, Formatter3164>` with facility
              `LOG_KERN`; `write_severity` for levels 0, 1, 2, 3, and 7
              writes the expected `<PRI>` values 3, 4, 5, 6, and 7.
            - `the_disabled_facility_removes_the_sink` — a fake sink on
              a `Log`, then `configure(&log, -1)`, then a message leaves
              the fake sink empty.
    - **Modify `rust/crates/binkd/src/lib.rs`**
        - Add `#[cfg(unix)] pub(crate) mod syslog;`.
    - **Verify:** `cd rust && PATH=/usr/bin:$PATH cargo test -p binkd
      syslog::tests` and `cargo +1.85 check -p binkd` and
      `cargo check -p binkd --target x86_64-pc-windows-gnu`.

- [x] **Task 3: Wire the config, the startup, and the reload**
    - **Implements:** design Phase 5 and 6; the config `syslog`
      directive of `readcfg.c`; roadmap `syslog-sink`
    - **Modify `rust/crates/binkd-config/src/parser.rs`**
        - Correct `syslog_arg()` — compare the facility name exactly,
          as the C `strcmp()` does. Map `auth` and `security` to 32,
          `authpriv` to 80, and `ftp` to 88. The current code lowercases
          the name and maps both auth names to 32.
        - Reject `mark` with the existing incorrect-facility error.
          The C `facilitynames` table accepts it, but `syslog` 7.0.0 has
          no `Facility` variant and the backend cannot send it.
        - Add `the_syslog_facility_is_parsed` — the existing `base()`
          helper plus `syslog local0` parses to
          `config.syslog_facility == 128`.
        - Add `the_syslog_facility_of_auth_and_authpriv` — `auth` is
          32, `authpriv` is 80, `ftp` is 88, and `security` is 32.
        - Add `the_syslog_facility_is_case_sensitive` — `LOG_LOCAL0`
          is an error that names the incorrect facility.
        - Add `the_mark_facility_is_rejected` — `syslog mark` is an
          error because `syslog` 7.0.0 has no backend for it.
        - Add `the_syslog_keyword_shows_the_c_fallback` — the same
          config dumps a `syslog` line with
          `[this item cannot be displayed]`, the C fallback text.
        - The tests use `Features::unix_default()`.
        - The tests state in a comment that the C parser rejects the
          line and no golden corpus case exists.
    - **Modify `rust/crates/binkd/src/start.rs`**
        - After `log.init(...)`, call
          `crate::syslog::configure(log, config.syslog_facility)` under
          `#[cfg(unix)]`, before `Daemon::new(...)`, so the `BEGIN` line
          can reach syslog.
    - **Modify `rust/crates/binkd/src/daemon.rs`**
        - In `reload_check()`, after `self.log.init(...)`, call
          `crate::syslog::configure(&self.log, config.syslog_facility)`
          under `#[cfg(unix)]`. A failed reload keeps the old config and
          leaves the old sink.
    - **Modify `rust/crates/binkd/src/lib.rs`**
        - Keep the module Unix-only. Windows has no backend.
    - **Verify:** `cd rust && cargo test -p binkd-config
      the_syslog_facility_is_parsed`, `cargo test -p binkd-config
      the_syslog_keyword_shows_the_c_fallback`,
      `cargo test -p binkd syslog::tests`,
      `cargo check -p binkd --all-targets`, and
      `cargo clippy -p binkd --all-targets -- -D warnings`.

- [x] **Task 4: Update the documents and run the full verification**
    - **Implements:** the documentation obligation of the design and the
      repository
    - **Modify `AGENTS.md`**
        - Add `syslog` to the Rust dependencies in Technical Context.
        - In Dependency Management, add `syslog = "=7.0.0"` as a
          Unix-only dependency of `binkd` and the reason: the safe local
          syslog backend.
        - In Testing, name the fake sink tests, the facility and priority
          tests, and the Rust-only config parser test.
    - **Modify `doc/rust-rewrite-plan.md`**
        - In Phase 5, record the `Sink` boundary and the fake sink tests.
        - In Phase 6, record `binkd::syslog`, the Unix-only backend, the
          facility and priority mapping, and the startup and reload calls.
        - Record the known differences: the filtered text, the RFC 3164
          header, the C parser rejection, the reload recreation, the
          silent connection fallback, and the rejected `mark` name.
        - Update the parser-inversion note in Phase 3 and record
          `syslog = "=7.0.0"` with the Unix target under section 7,
          item 7.
    - **Modify `tests/README.md`**
        - Add the fake sink, facility, priority, and parser test commands.
        - State that no live syslog daemon is required. A golden C case
          cannot parse a `syslog <name>` line; the dump holds the fallback.
    - **Modify `doc/checklist-gaps-roadmap.md`**
        - At approval, set slice 4 to `Status: Planned` and its `Plan`
          link to this file.
        - After the full verification, set the plan and slice to
          `Status: Implemented`.
    - **Verify:**
      `cd rust && cargo fmt --all --check`,
      `cargo clippy --workspace --all-targets`,
      `cargo test --workspace`,
      `cargo +1.85 check --workspace`,
      `cargo check -p binkd --target x86_64-pc-windows-gnu`,
      then `cd .. && make -C tests`, and `git diff --check`.
      When `BINKD_C` is set, run the existing interop suites.

## Done When

- A fake sink receives the final level and the final text.
- A hook and a `nolog` mask suppress the sink before it runs.
- The C priority table and the facility values have tests.
- A disabled facility sends nothing and removes an old sink.
- The `syslog` config line parses and the dump keeps the C fallback
  text.
- The Unix binary configures the sink at startup and after a reload.
- The Windows build has no backend and the Windows workspace tests pass.
- The C tests, the Rust workspace, the MSRV check, and the existing CI
  jobs pass.

# Windows Parity Implementation Plan

> Execute this plan with the `executing-plans` skill. Each task is a
> checkbox; mark it done when its changes are implemented and its
> Verify command passes.

- **Date**: 2026-11-16
- **Status**: Draft
- **Design**: [rust-rewrite-plan.md](rust-rewrite-plan.md), with the
  decisions [windows-support.md](decisions/windows-support.md) and
  [legacy-platforms.md](decisions/legacy-platforms.md)
- **Roadmap**: [checklist-gaps-roadmap.md](checklist-gaps-roadmap.md) —
  slice: `windows-parity`
- **Depends on**: —
- **Manual prerequisites**: a Windows runner for the target-gated
  tests. The local checks on another system are cross-target checks
  only.
- **Goal**: close the Windows-specific behavior gaps: the partition
  check, the `*N` short name, the Windows keyword feature set, and the
  rejection of `aso`, with the C Windows differences recorded.

## Constraints

- The workspace MSRV stays 1.85. Do not use a nightly-only API.
- `binkd-config` stays platform-neutral. It reads the volume identity
  through a provider function; it does not depend on `binkd-win`.
- `binkd-srif` stays pure. It receives the short-name lookup as a
  function; it does not depend on `binkd-win`.
- The unsafe Windows FFI stays in `binkd-win`. The crate already holds
  the `unsafe_code` exception of `doc/decisions/windows-support.md`.
- Add only the `windows-sys` feature that the new calls need. Do not add
  an external dependency.
- The C code is the reference. The Windows C build is not an oracle in
  the CI, so the differences are recorded from the C source and the
  Windows makefiles.
- The Rust Windows feature set accepts the proxy and rate-limit
  keywords. It rejects `aso`. It has no syslog backend and no dynamic
  zlib or bzip2 loader.
- The parser, the reloader, and the binary use the same feature set.
  The binary dumps with `dump_with()` and that set. Do not leave a
  parser or reloader call site on `Features::unix_default()` by
  accident.
- A failed volume query or short-name query is not fatal to the daemon.
  The partition check follows the C error text where it is defined; a
  short-name failure falls back to the validated name, as the C code
  does.
- No transient plan, design, roadmap, task, requirement, or slice
  references in code, identifiers, test names, strings, or comments.
  The documentation can name the design records.

## Open Risks And Dependencies

- **Nightly API blocker:**
  `std::os::windows::fs::MetadataExt::volume_serial_number` is
  nightly-only on Rust 1.85 and on the current stable release. Raising
  the MSRV does not enable it. The roadmap's chosen API cannot be used.
  This plan uses a safe `binkd-win` wrapper and a provider function in
  `binkd-config` instead. The `binkd-config` crate still does not depend
  on `binkd-win`; the binary supplies the provider. The roadmap decision
  bullet and the Windows decision record must say this.
- **Windows runner:** the target-gated tests and the dump test run only
  in the Windows CI job. A local run on macOS can compile the target
  but cannot execute those tests.
- **8.3 names:** `GetShortPathNameW` may answer the long name when 8.3
  generation is disabled on the volume. The test asserts a successful
  path and the fallback, not a particular `~1` spelling.
- **C Windows oracle:** there is no C Windows binary in the CI. The C
  Windows makefiles define `HTTPS`, `AMIGADOS_4D_OUTBOUND`, and the
  optional `WITH_ZLIB`, `WITH_BZLIB2`, `ZLIBDL`, `WITH_PERL`, and
  `BW_LIM` switches. The C source and the makefiles are the reference
  for the recorded differences.
- **Partition scope:** the C `check_config()` also checks the personal
  outbox and the outbound path. This slice adds the partition checks
  that the roadmap names and the node inbox existence check; it does
  not port the outbox checks.
- **Windows path roots:** a relative path and a path on another volume
  are answered as the C `stat()` answers them: a query for a missing
  path is `None`, and a query for an existing path uses the volume of
  that path. The identity is the volume serial, while the C Windows
  `st_dev` is the drive number; the common case agrees, and the
  difference is recorded.
- **C feature differences:** the C Windows build accepts `aso`; this
  port rejects it. The C Windows build has no syslog backend and no
  rate limit unless built with `BW_LIM`; this port accepts the
  `limit-rate` keyword. The C Windows build can load zlib and bzip2
  dynamically; this port cannot. The C Windows build accepts the proxy
  and socks keywords; this port accepts them as well.

## Tasks

- [x] **Task 1: Add the safe Windows path and volume helpers**
    - **Implements:** design Phase 6 platform values; the `*N` call of
      `run_args()` of `srif.c`; the volume check of `check_config()` of
      `readcfg.c`; roadmap `windows-parity`
    - **Modify `rust/crates/binkd-win/Cargo.toml`**
        - Add the `Win32_Storage_FileSystem` feature to `windows-sys`.
          Keep the exact version `=0.61.2` and the existing features.
    - **Create `rust/crates/binkd-win/src/storage.rs`**
        - Add `pub fn short_path(path: &Path) -> Result<String, u32>`:
          call `GetShortPathNameW` with a growing UTF-16 buffer; return
          the path as a `String`; return `GetLastError()` when the call
          answers zero or would overflow. Use the same path form that
          the caller supplied, including a relative path.
        - Add `pub fn volume_serial(path: &Path) -> Option<u32>`: open
          the path with `FILE_FLAG_BACKUP_SEMANTICS` so a directory is
          accepted, and with `FILE_SHARE_READ | FILE_SHARE_WRITE |
          FILE_SHARE_DELETE`, call `GetFileInformationByHandle`, and
          answer `dwVolumeSerialNumber`. Close the handle on every
          path, including the error path; a small RAII guard is
          acceptable. A failed open or call answers `None`.
        - Keep the `unsafe` blocks in this module and document the
          safe contract. The crate already allows `unsafe_code`.
        - Unit tests, all under `#[cfg(windows)]`:
            - `the_short_path_of_a_missing_path_fails` — a path under a
              missing directory answers `Err` with a nonzero code.
            - `the_short_path_of_a_temporary_file_is_a_path` — a
              temporary file answers `Ok` and a nonempty path. Do not
              assert a `~1` spelling; 8.3 generation may be disabled.
            - `the_volume_serial_of_a_temporary_directory` — a
              temporary directory answers `Some`, and two directories
              on the same volume answer the same value.
            - `the_volume_serial_of_a_missing_path` — a missing path
              answers `None`.
    - **Modify `rust/crates/binkd-win/src/lib.rs`**
        - Add `#[cfg(windows)] pub mod storage;`.
    - **Verify:** on Windows, `cd rust && cargo test -p binkd-win`;
      elsewhere, `cd rust && cargo check -p binkd-win --target
      x86_64-pc-windows-gnu`.
    - **Implementation status:** done. The cross-target check and strict
      clippy pass locally. The Windows-runner test gate remains: the
      four target-gated tests must run in the `windows` CI job.

- [x] **Task 2: Add the partition provider and the volume checks**
    - **Implements:** the `check_config()` volume checks of `readcfg.c`;
      design Phase 6 Windows parity; roadmap `windows-parity`
    - **Modify `rust/crates/binkd-config/src/parser.rs`**
        - Add `pub type Partition = fn(&Path) -> Option<u64>` and return
          the type from the crate root.
        - Add `partition: Partition` to `Parser`; initialize it in
          `Parser::with_ports()` with the platform default:
          `MetadataExt::dev()` on Unix, and `None` on Windows. Add
          `pub fn set_partition(&mut self, partition: Partition)` with
          docs.
        - Replace the two `check_directories()` bodies with one
          function on every platform. It uses the provider:
            - Resolve every path that it reads against `self.base_dir`
              when the path is relative, the port of the C code's
              process CWD. The parser corpus sets `base_dir` and runs
              from another directory, so the raw relative path would
              fail.
            - when `temp-inbound` is not empty and its identity is
              `Some`, compare the resolved `inbound` and
              `inbound-nonsecure` when their identity is `Some`;
            - answer the C text `Inbound and temp-inbound must be in
              the same partition` and `Unsecure-inbound and
              temp-inbound must be in the same partition`;
            - walk every node with a nonempty `ibox`; a missing or
              non-directory `ibox` answers the C text `Inbox for
              {ibox} does not exist (link {address})`; when
              `temp-inbound` has an identity, a different `ibox`
              identity answers `Inbox and temp-inbound must be in the
              same partition (link {address})`;
            - a provider answer of `None` skips the comparison, the
              port of a failed `stat()`.
        - Do not call a Windows API here. The default provider on
          Windows answers `None`; the binary installs the real one.
        - Tests in the module, all with a fake `Partition` function:
            - `the_inbound_and_temp_inbound_share_a_partition` — the
              same identity parses.
            - `the_inbound_and_temp_inbound_differ` — the error names
              the first C text.
            - `the_unsecure_inbound_and_temp_inbound_differ` — the
              error names the unsecure text.
            - `the_node_inbox_and_temp_inbound_differ` — a node `ibox`
              on another identity answers the link error.
            - `the_missing_node_inbox_is_an_error` — the missing `ibox`
              answers `Inbox for {ibox} does not exist (link
              {address})`.
            - `the_partition_provider_is_optional` — a provider that
              answers `None` accepts the config, the port of a failed
              `stat()`.
            - `the_relative_paths_use_the_base_dir` — a relative
              `ibox` resolves under `set_base_dir`, and the config
              parses when the directory exists.
        - The test config uses the existing `base()` helper plus
          `temp-inbound`, `inbound-nonsecure`, and one `node` line with
          an `ibox`. The relative-path case uses `set_base_dir()` and
          the `base()` helper.
        - The config corpus holds only successful `-d` dumps. The new
          rejection cases are unit tests; record that exception in
          `tests/README.md` in Task 6.
    - **Modify `rust/crates/binkd-config/src/reload.rs`**
        - Add `partition: Partition` to `Reloader`, initialize it from
          the platform default, add `pub fn set_partition(...)`, and
          pass it to the parser in `check()`. This keeps a reload under
          the same check as the first parse.
    - **Modify `rust/crates/binkd-config/src/lib.rs`**
        - Re-export `parser::Partition`.
    - **Verify:** `cd rust && cargo test -p binkd-config`.
    - **Implementation status:** done. The parser and reloader tests pass,
      and the Windows-target check and strict clippy pass. The binary
      installs the Windows provider in Task 4; the platform-neutral tests
      cover the seam with a fake provider.

- [x] **Task 3: Add the short-name seam to the SRIF expansion**
    - **Implements:** the `*N` branch of `run_args()` of `srif.c`;
      design Phase 6 SRIF; roadmap `windows-parity`
    - **Modify `rust/crates/binkd-srif/src/args.rs`**
        - Add `pub type ShortName = fn(&str) -> Result<String, u32>`.
          The error is the Windows error code of a failed short-name
          query.
        - Add `pub fn expand_with(command, filename, session,
          short_name: Option<ShortName>) -> Expanded`; keep `expand()`
          as `expand_with(..., None)`, the non-Windows behavior.
        - Add `pub short_name_error: Option<u32>` to `Expanded`.
        - In the `*N` branch, call the provider with the original
          `filename`:
            - `Ok(path)` answers the path;
            - `Err(code)` answers the validated `name`, records the
              code, and continues, the C fallback to `fn`;
            - `None` answers the validated `name`.
        - Keep the abort rule `!name_ok && used_name`. `*F` always sets
          `used_name`. `*N` sets it only when there is no provider or the
          provider answers `Err`, the C `use_fn` flag. A successful
          short path does not use the validated name, so an invalid
          received name with a successful short path does not abort, as
          in the C Windows branch.
        - Update the module docs: `*N` is the short name on Windows and
          the validated name when no provider answers or the provider
          fails.
        - Tests:
            - `the_short_name_of_a_file` — the provider answers
              `13B700~1.PKT`; `*N` answers it; `*F` still answers the
              long name.
            - `the_short_name_falls_back_to_the_name` — the provider
              answers `Err(5)`; the command holds the long name,
              `short_name_error == Some(5)`, and an invalid name aborts.
            - `the_short_name_of_an_invalid_name_does_not_abort` — the
              provider answers `Ok`; the command holds the short path
              and does not abort, the C Windows branch.
            - `the_short_name_is_absent_outside_windows` — `expand()` with
              no provider answers the name and reports no error; an
              invalid name with `*N` aborts, the C non-Windows branch.
    - **Modify `rust/crates/binkd-srif/src/engine.rs`**
        - Add `pub short_name: Option<ShortName>` to `Context`.
        - Use `args::expand_with(command, filename, session,
          context.short_name)` in both command paths.
        - When `short_name_error` is `Some(code)`, log at level 2:
          `GetShortPathName() fails {code}`, the C log text, before the
          security check and the run.
        - Add the field to every test `Context` with `None`; add a test
          that a failing provider logs the C text and runs the command
          with the long name.
    - **Verify:** `cd rust && cargo test -p binkd-srif`.
    - **Implementation status:** done. The expansion and engine tests pass,
      including the failed-lookup log. The `binkd-net` session contexts
      hold `None` until Task 4 passes the platform lookup.

- [x] **Task 4: Wire the short name and the partition provider through
  the binary**
    - **Implements:** the platform seam of the design; the `*N` call
      and the volume check; roadmap `windows-parity`
    - **Modify `rust/crates/binkd-net/src/session.rs`**
        - Add `pub short_name: Option<ShortName>` to `SessionOptions`,
          default `None`, with `pub fn with_short_name(...)`. Use the
          `binkd-srif` alias, not a second function type.
        - This is the same seam as `free_space`; the managers carry it
          into a session without a platform dependency. The field is a
          public API extension; external struct literals must add it or
          use the builder. Record that in Task 6.
    - **Modify `rust/crates/binkd-net/src/srif.rs`**
        - Re-export `pub use binkd_srif::args::ShortName;`.
        - Add `short_name: Option<ShortName>` to `Events`; keep the
          `Events::new` signature and add
          `pub fn with_short_name(...)`; pass the field into the three
          `Context` values in `on_file`, `flush`, and `finish`.
    - **Modify `rust/crates/binkd-net/src/caller.rs` and
      `rust/crates/binkd-net/src/listener.rs`**
        - Call `.with_short_name(options.short_name)` on each
          `Events::new` value. The listener and the caller are the
          normal session paths; the inetd path is not the only one.
    - **Modify `rust/crates/binkd/src/platform.rs`**
        - Import `binkd_net::srif::ShortName` and add
          `pub fn short_name() -> Option<ShortName>`: on Windows answer
          `Some(short_path)` that calls
          `binkd_win::storage::short_path(Path::new(path))`; elsewhere
          answer `None`.
        - Add `pub fn partition(path: &Path) -> Option<u64>`: on
          Windows answer `binkd_win::storage::volume_serial(path)` as
          `u64`; on Unix answer `MetadataExt::dev()`; elsewhere answer
          `None`.
        - Add target-gated tests: the Windows provider answers the
          volume of a temporary directory; the Unix provider answers
          the device of a temporary directory; `short_name` is `None`
          outside Windows.
    - **Modify `rust/crates/binkd/src/start.rs`**
        - After `Parser::new()`, call
          `parser.set_partition(platform::partition)` on every platform.
          The Unix value is the same as the parser default; the Windows
          value enables the check.
    - **Modify `rust/crates/binkd/src/daemon.rs`**
        - In `session_options()`, set `short_name =
          platform::short_name()`.
        - Call `.with_short_name(platform::short_name())` on the inetd
          `binkd_net::srif::Events::new` value as well.
        - In the daemon construction, call
          `reloader.set_partition(platform::partition)` after
          `Reloader::new`, so a reload uses the same provider.
    - **Modify `rust/crates/binkd/src/engine.rs`**
        - Set `short_name = platform::short_name()` in both
          `SessionOptions` values of `serve()` and `poll()`.
    - **Verify:** `cd rust && cargo test -p binkd-net`,
      `cargo test -p binkd`, `cargo check -p binkd --target
      x86_64-pc-windows-gnu`, and the Windows tests in CI. Search for
      every production `Events::new` call and every production
      `SessionOptions` value; none may be left without
      `with_short_name()` or the field. The test call sites may rely
      on the `None` default.
    - **Implementation status:** done. The short-name provider reaches the
      caller, listener, inetd, and engine paths; the partition provider
      reaches the parser and reloader. The Windows-runner test gate
      remains for the target-gated tests.

- [x] **Task 5: Add the Windows feature set and the dump test**
    - **Implements:** the `Features` of `readcfg.c`; the parser, reload,
      and dump; roadmap `windows-parity`
    - **Modify `rust/crates/binkd-config/src/keywords.rs`**
        - Add `pub fn windows_default() -> Features` with
          `zlib: true`, `https: true`, `bwlim: true`, `perl: false`,
          `amiga: false`, `zlibdl: false`, `syslog: false`.
        - Add `pub fn platform_default() -> Features`: the Windows set on
          Windows, `unix_default()` elsewhere.
        - Add tests:
            - `the_windows_default_has_the_rust_keywords` — `proxy`,
              `socks`, `limit-rate`, `zlevel`, `zminsize`, `zallow`, and
              `zdeny` are present; `aso`, `syslog`, `zlib-dll`,
              `bzlib2-dll`, and `perl-hooks` are absent.
            - `the_platform_default_follows_the_target` — the set matches
              the target.
    - **Modify `rust/crates/binkd-config/src/parser.rs`**
        - Replace `Features::unix_default()` with
          `Features::platform_default()` in `Parser::with_ports()`.
        - Add `#[cfg(unix)]` to the five `syslog` tests, including the
          dump-fallback test; they parse a Unix-only keyword. Add the
          Windows-only test that `proxy`, `socks`, and `limit-rate`
          parse, and `aso` and `syslog` are unknown.
        - In `the_bw_option_of_a_node`, set `features.bwlim = false` on
          the second parser that expects the rejection. On Windows the
          platform default has `bwlim = true`, so the plain-build
          rejection must state the set explicitly.
    - **Modify `rust/crates/binkd-config/src/reload.rs`**
        - Replace the default with `Features::platform_default()`.
    - **Modify `rust/crates/binkd-config/src/dump.rs`**
        - Keep `dump()` on `Features::unix_default()`. The golden
          corpus is the C Unix dump and holds the `syslog` line; a
          Windows default there would break the corpus. The binary uses
          `dump_with()` with the parser's features instead.
        - Update the `dump()` doc comment: it is the Unix golden-corpus
          helper, and the binary uses `dump_with()` for the platform
          set.
        - Add a `#[cfg(windows)]` test
          `the_windows_dump_has_the_rust_keywords` with
          `dump_with(config, &Features::windows_default())`: assert the
          `proxy`, `socks`, `limit-rate`, and `zlevel` lines are present;
          assert `aso`, `syslog`, `zlib-dll`, `bzlib2-dll`, and
          `perl-hooks` are absent.
    - **Modify `rust/crates/binkd/tests/cli.rs`**
        - Gate the existing `dump_with_d` with `#[cfg(all(unix, not(feature
          = "perl")))]`; its fixture is the C Unix dump without the
          Perl lines.
        - Add `dump_with_d_perl` under `#[cfg(all(unix, feature =
          "perl"))]`: assert the output holds `perl-hooks`, `perl-dll`,
          `perl-strict`, and `perl-var`; when `BINKD_C_PERL` is set,
          compare with the C Perl dump.
        - Add a `#[cfg(windows)]` test for `-d`: assert the output holds
          `proxy`, `socks`, `limit-rate`, and `zlevel`, and omits
          `aso`, `syslog`, `zlib-dll`, and `bzlib2-dll`.
    - **Modify `rust/crates/binkd/src/start.rs` and
      `rust/crates/binkd/src/daemon.rs`**
        - In `start.rs`, build one `let mut features =
          binkd_config::Features::platform_default()` before the parser;
          set `features.perl = cfg!(feature = "perl")`; pass it to the
          parser. The unconditional assignment avoids the `unused_mut`
          lint of the default build.
        - Dump with `binkd_config::dump_with(&config, &features)`, not
          the platform default again, so the dump holds the same
          keywords as the parser, including the Perl feature.
        - Keep the `Daemon::new()` signature and add
          `pub fn with_features(mut self, features: Features) -> Daemon`;
          set the `Reloader` features there, and remove the
          `#[cfg(feature = "perl")]` block that sets `unix_default()`.
          Build the daemon with `Daemon::new(...).with_features(features)`
          in `start.rs`. This avoids a break of the public constructor.
        - The reloader, the parser, the dump, and the binary then use
          one feature set.
    - **Verify:** `cd rust && cargo test -p binkd-config`, the Windows
      dump tests in CI, `cargo test -p binkd --test cli`,
      `cargo clippy --workspace --all-targets -- -D warnings`, and the
      `cargo check --target x86_64-pc-windows-gnu` check for `binkd`.
    - **Implementation status:** done. The Unix parser, reloader, and
      binary use the platform set; the default and Perl CLI dump tests
      pass. The Windows dump and keyword tests are target-gated and run
      in the `windows` CI job.

- [ ] **Task 6: Update the documents and run the full verification**
    - **Implements:** the design and decision documentation obligation;
      roadmap `windows-parity`
    - **Modify `doc/decisions/windows-support.md`**
        - Record the parity work: the volume check, the short name, the
          Windows feature set, and the rejection of `aso`.
        - Record the C Windows differences: C accepts `aso`; C has no
          syslog backend; C has `zlib-dll` and `bzlib2-dll` with
          `ZLIBDL`; C accepts `limit-rate` only with `BW_LIM`; C accepts
          proxy and socks with `HTTPS`; C accepts the compression
          keywords only with `WITH_ZLIB` or `WITH_BZLIB2`. State the
          Rust choices.
        - State that the partition provider uses `binkd-win` and that
          `binkd-config` holds no Windows dependency. Record that the
          Windows identity is the volume serial, while the C Windows
          `st_dev` is the drive number; the common case agrees.
        - Record the public API extension of `SessionOptions`: the
          `short_name` field; struct-literal callers must add it or use
          the builder.
        - Update the Status section and the test list.
    - **Modify `README.md`**
        - State the Windows known differences: no tray icon, no `-T`,
          no Windows 9x/Me, no `aso`, no syslog, no dynamic zlib or
          bzip2 loader, and the accepted proxy and rate-limit keywords.
    - **Modify `doc/rust-rewrite-plan.md`**
        - In Phase 6, record the `storage` module of `binkd-win`, the
          provider seam, the `*N` path, and the Windows feature set.
        - In the Windows service bullet of Phase 6 and the known
          differences of Phase 8, add the C Windows feature
          differences.
        - In section 7, record the MSRV decision: the std volume API is
          nightly-only on every current stable release, so the port
          uses the safe wrapper and the provider.
    - **Modify `AGENTS.md`**
        - In Technical Context and the Project Structure, state that
          `binkd-win` holds the service FFI and the safe file-system
          helpers.
        - In Testing, name the Windows target tests and the Windows dump
          test.
        - In Dependency Management, add the `Win32_Storage_FileSystem`
          feature of `windows-sys` and the reason.
    - **Modify `tests/README.md`**
        - Add the Windows test commands: `cargo test -p binkd-win`,
          `cargo test -p binkd-config`, and the Windows workspace suite.
        - State that the target-gated tests run in the `windows` CI job.
        - State that the config corpus holds only successful dumps, so
          the partition and missing-inbox rejection paths are unit
          tests.
    - **Modify `doc/checklist-gaps-roadmap.md`**
        - Verify the already-updated "Windows partition check"
          decision bullet names the provider seam and the safe
          `binkd-win` wrapper, not the nightly API.
        - The slice is already `Planned` with this plan link; verify
          that link. After the full verification, set the plan and
          slice to `Status: Implemented`.
    - **Verify:**
      `cd rust && cargo fmt --all --check`,
      `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace`,
      `cargo test -p binkd --features perl -- --test-threads=1`
      with `BINKD_C_PERL`,
      `cargo +1.85 check --workspace`,
      `cargo check -p binkd --target x86_64-pc-windows-gnu`,
      `cargo check -p binkd-win --target x86_64-pc-windows-gnu`,
      then `cd .. && make -C tests`, and `git diff --check`.
      On Windows, run `cargo test -p binkd-win`, `cargo test
      -p binkd-config`, and `cargo test --workspace` in the `windows`
      CI job. Run the service smoke test when the release binary is
      built.

## Done When

- The Windows parser accepts `proxy`, `socks`, `limit-rate`, and the
  compression keywords, and rejects `aso` and `syslog`. The Windows
  `-d` output holds the accepted set and omits the rejected words.
- The parser and reloader check `temp-inbound`, `inbound`,
  `inbound-nonsecure`, and every node `ibox` through the partition
  provider.
- The Windows provider answers the volume serial through the safe
  `binkd-win` wrapper, and the Unix provider answers the device
  identity.
- A `*N` command uses the Windows short path, falls back to the
  validated name on failure, and logs the C failure text.
- The Windows workspace suite passes the partition, short-name, feature,
  and dump tests.
- The C Windows differences are recorded in the decision record and the
  README.
- The C tests, the Rust workspace, the MSRV check, the Windows target
  check, and the existing CI jobs pass.

# Local Time And TIME Frame Implementation Plan

> Execute this plan with the `executing-plans` skill. Each task is a
> checkbox; mark it done when its changes are implemented and its
> Verify command passes.

- **Date**: 2026-11-16
- **Status**: Implemented
- **Design**: [rust-rewrite-plan.md](rust-rewrite-plan.md)
- **Roadmap**: [checklist-gaps-roadmap.md](checklist-gaps-roadmap.md) —
  slice: `local-time-and-time-frame`
- **Depends on**: `platform-seam-free-space` (Implemented)
- **Manual prerequisites**: —
- **Goal**: use the system local time and the `tzoff` directive in the
  logs, the binary logs, `-v`, and the `TIME` banner, and send the C
  `M_NUL TIME` frame from both session roles.

## Constraints

- The explicit `tzoff` value is in seconds. Its offset is
  `tzoff / 60` minutes with integer truncation toward zero, the C
  `tz_off()` rule.
- `tzoff == -1` selects the system local offset. A failed local-offset
  lookup selects UTC and zero minutes.
- `DateTime.epoch` stays the real epoch. The displayed `DateTime` fields
  are system local time. Add `year` and `offset_minutes` to `DateTime`.
- The text log fields and `-v` use the local civil fields. The `tzoff`
  directive does not change them.
- The `TIME` value uses the local civil fields of the session, the
  explicit or system offset, and the C format:
  `Day, DD Mon YYYY HH:MM:SS ±HHMM`. The day has the C `%2d` width, and
  the offset has the form `%c%02d%02d`.
- The `TIME` frame is `M_NUL TIME <value>`. Send it after `NDL` and
  before `VER` in both roles. Ignore the `TIME` frame of the peer.
- Use `time = "=0.3.44"` with the `local-offset` feature in the `binkd`
  crate only. No pure crate uses `time`.
- The `binkd-log::binlog::write()` uses `BinLogConfig.tzoff` and takes
  the system offset in minutes as its second offset input, the port of
  `tz_off()`.
- Every in-process session test and every recording that sends `TIME`
  uses a fixed `BannerTime`. The binary tests fix only the offset with
  `tzoff -3h`. The recorded sessions store the fixed value as `# time`.
- No new CI job. The Linux, macOS, and Windows workspace jobs run the
  new code.
- No `unsafe` code outside the existing exceptions.
- No public API uses a placeholder or a transient plan reference.

## Open Risks And Dependencies

- **Dependency:** `time = "=0.3.44"` adds `time-core`, `deranged`,
  `num-conv`, `num_threads`, and `powerfmt` to the binary crate. The
  exact version and the `local-offset` feature need the 1.85 MSRV
  check.
- **System offset:** a container, a Windows service, or a host without
  zone data may fail the local-offset lookup. The fallback is UTC, but
  the C binary may resolve the system zone. The `-v` local-zone test is
  Unix-only.
- **DST:** the `time` crate reports the offset at the session time. The
  C `tz_off()` uses `mktime()` with `tm_isdst = 0`. A DST transition may
  differ. The explicit `tzoff` path is exact and has corpus cases.
- **Recordings:** the `TIME` frame changes every recorded session and
  every C frame trace. Regenerate `tests/sessions/` and update the C
  frame parser before the recording tests can pass.
- **Binary log caller:** the binary has no production caller of
  `binlog::write()` in this slice. The function uses the config offset,
  but a future caller must supply the system offset. The tests cover
  the function.
- **Windows value path:** the Windows build compiles and the workspace
  tests run, but the system-offset value is not asserted on Windows.
  The explicit-offset tests cover the format.

## Tasks

- [x] **Task 1: Add the local clock and the DateTime fields**
    - **Implements:** roadmap `local-time-and-time-frame`; design §4.1
      log format; checklist item 1; the `safe_localtime()` behavior of
      `vLog()` and `__DATE__`
    - **Modify `crates/rbinkd/Cargo.toml`**
        - Add `time = { version = "=0.3.44", features =
          ["local-offset"] }` to `[dependencies]`.
        - Update `Cargo.lock`.
    - **Modify `crates/binkd-log/src/log.rs`**
        - `DateTime.year: i32` — the local year, the port of
          `tm_year + 1900`.
        - `DateTime.offset_minutes: i32` — the local offset from UTC in
          minutes, the value that `tz_off()` needs for `tzoff == -1`.
        - Update the docs of `DateTime` to state that the civil fields
          and the year are local.
        - Update every `DateTime` literal in the crate. The fixed test
          time uses `year: 2020` and `offset_minutes: 0`.
    - **Modify `crates/rbinkd/src/platform.rs`**
        - `now()` — use `time::OffsetDateTime::now_local()`. On an
          error, use `OffsetDateTime::now_utc()`. Fill `epoch` from
          `unix_timestamp()`, the civil fields from the local value,
          `year` from `year()`, and `offset_minutes` from
          `offset().whole_minutes()`.
        - `date()` — format `time.year` instead of the year of the UTC
          epoch, and state in its doc that the fields are local.
        - Keep `civil_from_days()` and its tests.
        - Update `the_date_of_a_time` — add `year: 2027` and
          `offset_minutes: 0` to the fixed `DateTime`. The expected text
          stays `Jan 25 2027 23:57:08`.
        - Add `the_local_offset_of_now` — `now().offset_minutes` is in
          `-720..=840`, and `now().year` is in `1970..=9999`.
    - **Modify every `DateTime` literal outside the log crate**
        - `crates/rbinkd/src/platform.rs`
        - `crates/binkd-net/src/caller.rs`
        - `crates/binkd-net/src/listener.rs`
        - `crates/binkd-net/src/srif.rs`
        - `crates/binkd-net/tests/interop.rs`
        - `crates/binkd-net/tests/loopback.rs`
        - `crates/binkd-net/tests/proxy_interop.rs`
        - `crates/binkd-net/tests/srif.rs`
        - `crates/binkd-srif/src/engine.rs`
        - `crates/rbinkd/tests/daemon.rs`
        - `crates/rbinkd/tests/perl.rs`
        - `crates/binkd-perl/tests/on_log.rs`
        - The time `1_000_000_000` uses `year: 2001` and
          `offset_minutes: 0`. The time `0` uses `year: 1970` and
          `offset_minutes: 0`.
    - **Modify `crates/rbinkd/tests/cli.rs`**
        - Add `run_port_env(args, dir, env)` — run the port with the
          extra environment values.
        - Add `the_version_date_is_local` with `#[cfg(unix)]`: call
          `time::OffsetDateTime::now_local()`. If it fails, print a
          skip and return. Run `-v` with `TZ=Etc/GMT+12` and with
          `TZ=Pacific/Kiritimati`. Parse the date with
          `version_shape()`, and assert that the two dates differ.
    - **Verify:** `cargo test -p binkd-log` and
      `cargo test -p rbinkd platform::tests` and
      `cargo test -p rbinkd --test cli the_version_date_is_local`.
      Also run `cargo +1.85 check -p rbinkd`.

- [x] **Task 2: Add BannerTime and the tzoff offset rule**
    - **Implements:** design §4.1 log format; the `banner()` and
      `tz_off()` behavior of `protocol.c` and `tools.c`; roadmap
      `local-time-and-time-frame`
    - **Create `crates/binkd-proto/src/banner.rs`**
        - `#[derive(Clone, Copy, Debug, PartialEq, Eq)]` on
          `pub struct BannerTime`.
        - `BannerTime` holds the fields `year: i32`, `month: u32`,
          `day: u32`, `hour: u32`, `minute: u32`, `second: u32`,
          `weekday: u32`, and `offset_minutes: i32`.
        - `BannerTime::new(year, month, day, hour, minute, second,
          offset_minutes) -> BannerTime` — compute `weekday` from the
          date with the Sakamoto algorithm. Sunday is 0.
        - `BannerTime::text(&self) -> String` — render the C value. Use
          `Sun`, `Mon`, `Tue`, `Wed`, `Thu`, `Fri`, `Sat` and the
          three-letter English months. Use `{:2}` for the day,
          `{:02}` for the time fields, and `+` or `-` with
          `abs(offset_minutes) / 60` and `% 60` for the offset.
        - Unit tests:
            - `the_time_text_of_a_negative_offset` —
              `BannerTime::new(2026, 11, 15, 9, 5, 7, -180).text()` is
              `Sun, 15 Nov 2026 09:05:07 -0300`.
            - `the_time_text_of_a_positive_offset` —
              `BannerTime::new(2024, 2, 29, 23, 59, 59, 330).text()` is
              `Thu, 29 Feb 2024 23:59:59 +0530`.
            - `the_time_text_of_zero` —
              `BannerTime::new(1970, 1, 1, 0, 0, 0, 0).text()` is
              `Thu,  1 Jan 1970 00:00:00 +0000`.
    - **Modify `crates/binkd-proto/src/lib.rs`**
        - Add `pub mod banner;`.
        - Re-export `BannerTime`.
    - **Modify `crates/binkd-proto/src/session.rs`**
        - `SessionConfig.banner_time: Option<BannerTime>` — the fixed
          banner of the session. Add it after `inbound`.
    - **Modify the `SessionConfig` literals**
        - `crates/binkd-proto/src/record.rs`
        - `crates/binkd-proto/tests/session.rs`
        - `crates/binkd-proto/tests/record.rs`
        - `crates/binkd-proto/tests/interop.rs`
        - Use `banner_time: Some(BannerTime::new(2026, 11, 15, 9, 5,
          7, 0))` in the helpers. The `record` replay overrides this
          value from the recording.
    - **Modify `crates/binkd-net/src/session.rs`**
        - `session_config(config, node, crypt, trace, time: &DateTime)`
          — add the local session time.
        - Set `banner_time` to `BannerTime::new(time.year,
          time.month, time.day, time.hour, time.minute, time.second,
          offset_minutes)`.
        - The offset is `binkd_ftn::text::tz_off(config.tzoff) as i32`
          when `config.tzoff != -1`. Otherwise it is
          `time.offset_minutes`.
        - Add unit tests with a fixed `DateTime` for 2026-11-15
          09:05:07:
            - `the_banner_time_of_an_explicit_offset` — `tzoff` is
              `-10800`; the text ends with `-0300`.
            - `the_banner_time_of_the_system_offset` — `tzoff` is `-1`
              and `time.offset_minutes` is `120`; the text ends with
              `+0200`.
            - `the_banner_time_truncates_seconds` — `tzoff` is `90`;
              the text ends with `+0001`. `tzoff` is `-90`; the text
              ends with `-0001`.
    - **Modify the `session_config()` call sites**
        - `crates/binkd-net/src/listener.rs` — pass `&now()`.
        - `crates/binkd-net/src/caller.rs` — pass `&now()`.
        - `crates/binkd-net/src/session.rs` tests — pass the
          fixed `DateTime`.
        - `crates/binkd-net/tests/loopback.rs` — pass the fixed
          `time()` to the direct call.
        - `crates/rbinkd/src/daemon.rs` — pass
          `&platform::now()`.
        - `crates/rbinkd/tests/daemon.rs` — pass the fixed
          `DateTime` to the direct call.
    - **Modify `tests/golden/tools.tsv`**
        - Add `tz_off 59 => 0`, `tz_off -59 => 0`, `tz_off 90 => 1`,
          `tz_off -90 => -1`, `tz_off 3599 => 59`, and
          `tz_off -3599 => -59`.
        - The Rust corpus runner already dispatches `tz_off`; no Rust
          code change is needed.
    - **Verify:** `cargo test -p binkd-proto banner` and
      `cargo test -p binkd-net the_banner_time` and
      `cargo test -p binkd-ftn --test corpus` and
      `cargo check -p rbinkd --all-targets`.
      Also run `cd .. && make -C tests`.

- [x] **Task 3: Send the TIME frame**
    - **Implements:** design §4 Level 2 session layer; the `banner()`
      behavior of `protocol.c`; roadmap `local-time-and-time-frame`
    - **Modify `crates/binkd-proto/src/session.rs`**
        - In `handshake()`, after the `NDL` frame and before the `VER`
          frame, send `send_text2(Command::Nul, "TIME ", &text)` when
          `banner_time` is `Some`. Read the text before the await.
        - Do not change `handle_nul()`. The peer `TIME` stays ignored.
    - **Modify `crates/binkd-proto/src/frame.rs`**
        - `Frame::log_name()` — add `TIME` to the `NUL` kinds. The
          frame name is `NUL TIME`; the value is not part of the name.
        - Add a unit test that a `TIME` frame has the name
          `NUL TIME`.
    - **Modify `crates/binkd-proto/tests/session.rs`**
        - Add `the_time_frame_is_sent` — set `server_config.trace =
          true`, run a session, and assert that the server report
          holds a **sent** `NUL TIME` frame. Test the `sent` flag, not
          only the name; the peer frame is also in the report.
    - **Modify `crates/binkd-proto/tests/interop.rs`**
        - Add `"TIME"` to the `NUL` kinds of `c_frames_sent()` and
          `c_frames_received()`.
        - The C frame sequence must match the Rust sequence with the
          new frame.
    - **Verify:**
      `cargo test -p binkd-proto --test session the_time_frame_is_sent`
      and `cargo test -p binkd-proto --test interop`.
      The interop test needs the C binary:
      `BINKD_C=/path/to/binkd cargo test -p binkd-proto --test interop`.

- [x] **Task 4: Store the banner time in the recordings and the C interop**
    - **Implements:** design §4 test strategy; the recorded sessions and
      the C↔Rust binary suite; roadmap `local-time-and-time-frame`;
      checklist item 4
    - **Modify `crates/binkd-proto/src/record.rs`**
        - `Recording.banner_time: Option<BannerTime>` — the fixed
          banner of the recording.
        - `Recording::new()` sets it to `None`.
        - `parse()` reads `# time <year> <month> <day> <hour> <minute>
          <second> <offset_minutes>` into `BannerTime::new()`. Invalid
          numbers are an `InvalidData` error.
        - `to_text()` writes the `# time` line after `# role`.
        - Add `banner_time_round_trip` — a recording with a
          `BannerTime` parses to the same value.
    - **Modify `crates/binkd-proto/tests/record.rs`**
        - `replay()` — set `config.banner_time =
          recording.banner_time`.
        - `write_the_recordings()` — set `side.banner_time =
          Some(BannerTime::new(2026, 11, 15, 9, 5, 7, 0))`, set the
          same value on the peer, and set `recording.banner_time =
          side.banner_time`.
        - `record_a_c_binary_session()` — copy the server
          `banner_time` into the returned recording.
    - **Regenerate `tests/sessions/*.session`**
        - Run
          `cargo test -p binkd-proto --test record -- --ignored write_the_recordings`.
        - The files gain the `# time` line and the `TIME` bytes in both
          directions.
    - **Modify `crates/rbinkd/tests/interop_binary.rs`**
        - Add `tzoff -3h` to the four generated configs.
        - Add `assert_log_line(dir, name, first, second)` — true when
          one log line holds both texts. The existing `assert_log()`
          checks only one text and cannot prove the received `TIME`
          line has the offset.
        - In `the_rust_client_calls_the_c_server`, call the helper on
          `c-server.log` with `rcvd msg NUL TIME` and `-0300`.
        - In `the_c_client_calls_the_rust_server`, call the helper on
          `c-client.log` with `rcvd msg NUL TIME` and `-0300`.
        - The C↔C baseline keeps the same config values.
    - **Verify:**
      `cargo test -p binkd-proto --test record` and
      `BINKD_C=/path/to/binkd cargo test -p rbinkd --test interop_binary`.
      Without `BINKD_C`, the C binary test skips.

- [x] **Task 5: Wire the binary-log offset, update the docs, and run the
  full verification**
    - **Implements:** design §4.1 log format; the `BinLogStat()` and
      `tz_off()` behavior of `binlog.c` and `tools.c`; roadmap
      `local-time-and-time-frame`; the documentation obligation
    - **Modify `crates/binkd-log/src/binlog.rs`**
        - `write(config, stat, system_offset_minutes)` — compute
          `tz_minutes` as `system_offset_minutes` when
          `config.tzoff == -1`, else
          `binkd_ftn::text::tz_off(config.tzoff as i32)`.
        - Use `tz_minutes` in both record constructors.
        - Update the doc of `BinLogConfig.tzoff` and `write()`.
        - Add `write_uses_the_explicit_offset` — `tzoff` is `10800`;
          the T-Mail start time is `stat.start_time + 10800`.
        - Add `write_uses_the_system_offset` — `tzoff` is `-1`; the
          system offset is `120`; the start time is
          `stat.start_time + 7200`.
        - Add `write_truncates_an_explicit_offset` — `tzoff` is `90`;
          the start time is `stat.start_time + 60`.
    - **Modify `AGENTS.md`**
        - In Technical Context, add `time` to the Rust dependencies.
        - In Dependency Management, add `time` (`=0.3.44`,
          `local-offset`, the `binkd` crate only) and the reason: the
          safe local offset for the logs, `TIME`, and `-v`.
        - In Testing, name the fixed `# time` recording metadata and
          the `tz_off` corpus cases.
    - **Modify `doc/rust-rewrite-plan.md`**
        - In §4.2, replace the `tz_off()` difference with the local
          clock rule and the explicit/system offset rule.
        - In Phase 5, record `DateTime.year` and
          `DateTime.offset_minutes`, the `binlog` offset, and the
          local text log.
        - In Phase 4, record `binkd-proto::banner` and the
          `M_NUL TIME` frame. The frame comes after `NDL` and before
          `VER`.
        - In the test list, record the fixed banner time, the C frame
          sequence, and the binary interop `tzoff` check.
        - In Dependency Management, record `time = "=0.3.44"` and the
          `local-offset` feature.
    - **Modify `tests/README.md`**
        - In the recorded sessions, document the `# time` line and its
          seven values.
        - In the interop tests, name the `TIME` frame and the binary
          test `tzoff -3h` check.
        - In the golden corpus, name the `tz_off` edge cases.
        - Add the focused commands of this plan.
    - **Modify `doc/checklist-gaps-roadmap.md`**
        - At approval, set slice 3 to `Status: Planned` and its `Plan`
          link to this file.
        - After the full verification, set slice 3 to
          `Status: Implemented` and set this plan to
          `Status: Implemented`.
    - **Verify:**
      `cargo fmt --all --check`,
      `cargo clippy --workspace --all-targets`,
      `cargo test --workspace`,
      `cargo +1.85 check --workspace`,
      `cargo check -p rbinkd --target x86_64-pc-windows-gnu`,
      then `cd .. && make -C tests`, and `git diff --check`.
      When `BINKD_C` is set, also run the three interop suites.

## Deviations

- `BannerTime` has public fields. Recording serialization reads all seven
  values, and private fields would need extra accessors.
- The `BannerTime` weekday uses `div_euclid()` on an `i64` intermediate.
  This keeps year 0, negative years, and the full `i32` year range
  correct; the added tests cover year 0 and year -1.
- `UtcOffset::whole_minutes()` returns `i16`; the code casts it to the
  `i32` field with `i32::from()`.
- The `binkd-ftn` corpus runner already dispatches `tz_off`, so only the
  corpus data changed. The plan no longer names a Rust code change for
  the corpus.
- The design dependency note is under the existing dependency-policy
  item in section 7. The design has no separate Dependency Management
  section.
- `run_port_env` is Unix-only. Its only caller is Unix-only, and the
  Windows cross-target clippy rejects the dead helper without the gate.
- The C reference for the interop runs was built outside the workspace.
  No C build artifact is left in the tree.

## Done When

- The local text log and `-v` show the system local year and time.
- A fixed `DateTime` and a `tzoff` value produce the C `TIME` text for
  positive, negative, zero, truncating, and system offsets.
- Both session roles send `M_NUL TIME` after `NDL` and before `VER`.
- The C frame sequence and the Rust frame sequence match.
- The recorded sessions replay with the fixed banner time.
- The C and Rust binary interop logs show `TIME` with `-0300` for
  `tzoff -3h`.
- `BinLogConfig.tzoff` changes the binary-log start time.
- The C tests, the Rust workspace, the MSRV check, the Windows target
  check, and the existing CI jobs pass.

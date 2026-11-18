# Closing the binkd Checklist Gaps Roadmap

> Each slice gets its own plan, written when the slice starts. Execute a
> slice with the `executing-plans` skill. The C tree named in the completed
> slices is now upstream; see `doc/decisions/c-out-of-tree.md`.

- **Date**: 2026-11-15
- **Status**: Implemented
- **Design**: [rust-rewrite-plan.md](rust-rewrite-plan.md), with the
  decisions [legacy-platforms.md](decisions/legacy-platforms.md) and
  [windows-support.md](decisions/windows-support.md)
- **Strategy**: The remaining work has independent deliverables. The
  binary interop slice comes first because it closes checklist item 4
  with test-only changes and gives the later slices a real C↔Rust
  baseline. The free-space slice follows because it establishes the
  platform-provider seam that the local-time slice reuses. The syslog
  and Windows slices are then independent. The final slice adds the
  remaining platform CI and closes the docs.

## Decisions To Make In The Slice Plans

These are the choices the roadmap leaves to the just-in-time plan of the
named slice. They are recorded here so the slices do not make different
choices.

- **Free space**: use a safe platform API, not a new `unsafe` crate. The
  slice plan pins the exact crate and version; the current candidates
  are `fs2` for the cross-platform path or a thin `nix` wrapper on
  Unix. The value is kilobytes, as in `unix/getfree.c` and
  `nt/getfree.c`.
- **Local time**: use the safe `time` crate pinned to a version whose
  `rust-version` is at most 1.85 (`0.3.44` at the time of this
  roadmap), or a platform wrapper with the same MSRV.
- **Syslog**: keep the FFI out of `binkd-log`; add a sink boundary there
  and a Unix-only backend in the binary. The slice plan pins the exact
  backend crate and version.
- **Windows features and ASO**: do not implement the Amiga-style
  outbound. Reject `aso` on Windows, accept the proxy and rate-limit
  keywords, and document the divergence from the C Windows build. The
  Windows dump must use the same `Features` as the parser.
- **Windows short names**: add a safe `binkd-win` wrapper over
  `GetShortPathNameW`; do not move `binkd-win` into a lower crate.
- **Windows partition check**: use a provider function in
  `binkd-config`; the binary supplies the safe `binkd-win` wrapper
  over the Windows volume serial. The std
  `MetadataExt::volume_serial_number` API is nightly-only, so the
  wrapper is required. `binkd-config` holds no `binkd-win` dependency.
- **BSD CI**: add a FreeBSD job only if the runner action is accepted;
  otherwise document that BSD is build-supported but not CI-tested.

## Slices

### Slice 1 — binary-interop

Add the missing binary-to-binary C/Rust sessions and the C↔C baseline.
The tests use `CARGO_BIN_EXE_rbinkd` for the Rust binary and `BINKD_C`
for the C binary. They start the C server and the Rust client, the Rust
server and the C client, and two C binaries. Each case exchanges one
packet in each direction and asserts the inbound files, the `BEGIN` and
session log lines, and the exit codes. The CI `interop` job runs the new
test after the existing library-level matrix. This slice has no platform
decision and closes checklist item 4 while the other slices are still
unplanned.

- **Status:** Implemented
- **Depends on:** —
- **Requirements:** Checklist item 4; design §4 Level 4, the
  `C | C`, `Rust | C`, and `C | Rust` rows
- **Manual prerequisites:** a plain C reference build for local runs
- **Plan:** [the plan](checklist-gaps-plan-binary-interop.md)
- **Done when:** the new test passes with `BINKD_C` locally and in the
  CI `interop` job, covers both binary directions, and the C↔C baseline
  row is no longer a documented gap

### Slice 2 — platform-seam-free-space

Thread a free-space provider from the binary platform layer to
`binkd-proto::inb_fopen` and replace the `FreeSpace::default()` value.
Implement the provider for Unix and Windows, preserve the C
kilobyte/integer-division behavior, and make `minfree` and
`minfree-nonsecure` reject and allow a transfer as the C code does. Add
unit tests for the policy and a provider test with a real temporary
directory, plus a corpus case for a non-negative `minfree`.

- **Status:** Implemented
- **Depends on:** —
- **Requirements:** Checklist item 1; design §3 file-system effects,
  §4.1 equivalence policy
- **Manual prerequisites:** —
- **Plan:** [the plan](checklist-gaps-plan-platform-seam-free-space.md)
- **Done when:** a full test transfer is rejected when the computed free
  kilobytes are below the threshold and succeeds above it; the
  `temp-inbound` minimum rule is covered; the pure crates still make no
  platform calls

### Slice 3 — local-time-and-time-frame

Add the local time and UTC offset to the platform layer, keep
`DateTime.epoch` as the real epoch, derive the displayed fields from the
local time, apply the `tzoff` directive, and send the C `M_NUL TIME`
frame. Update the log and `-v` paths, add fixed-epoch tests for explicit
and system offsets, and compare the new frame against the C binary in
the interop suite.

- **Status:** Implemented
- **Depends on:** Slice 2
- **Requirements:** Checklist item 1; design §4.1 log format, the
  `banner()` and `tz_off()` behavior of `protocol.c` and `tools.c`
- **Manual prerequisites:** —
- **Plan:** [the plan](checklist-gaps-plan-local-time-and-time-frame.md)
- **Done when:** the fixed-offset tests match the C civil fields, the
  `TIME` frame matches the C frame, and the Windows and Unix offset
  paths both pass

### Slice 4 — syslog-sink

Add a log sink boundary to `binkd-log`, a Unix syslog backend in the
binary, the C priority mapping, and the `syslog` facility from the
config. Keep `binkd-log` free of platform FFI. Add a fake sink that
asserts the level and text, a test that the hook and `nolog` filters
run before the sink, and a parser/dump case for the enabled feature.

- **Status:** Implemented
- **Depends on:** —
- **Requirements:** Checklist item 1; the `syslog` branch of `vLog()`
  in `tools.c`
- **Manual prerequisites:** —
- **Plan:** [the plan](checklist-gaps-plan-syslog-sink.md)
- **Done when:** a fake sink receives the expected priority and message
  on Unix, a disabled `syslog` sends nothing, Windows has no backend,
  and the parser and dump use the same feature set

### Slice 5 — windows-parity

Close the Windows-specific behavior gaps:

- check `temp-inbound`, `inbound`, `inbound-nonsecure`, and node `ibox`
  volumes through the partition provider;
- implement the `*N` short name through `binkd-win` and the SRIF
  expansion seam;
- add a Windows `Features` default for the keywords the Rust build
  accepts, and use it in the parser, reloader, dump, and binary;
- reject `aso` and document the C Windows divergence.

Add target-gated unit tests and a Windows dump test; run them in the
`windows` CI job.

- **Status:** Implemented
- **Depends on:** —
- **Requirements:** Checklist item 1;
  [windows-support.md](decisions/windows-support.md);
  [legacy-platforms.md](decisions/legacy-platforms.md)
- **Manual prerequisites:** a Windows runner for the target-gated tests
- **Plan:** [the plan](checklist-gaps-plan-windows-parity.md)
- **Done when:** the Windows workspace suite passes the partition,
  short-name, feature, and dump tests; the C Windows differences are
  recorded in the decision record and the README

### Slice 6 — modern-platform-ci-and-release

Run the new parity tests on the matching platform jobs, add the FreeBSD
job or record the accepted absence, and update the docs:

- add the FreeBSD workspace test job, or the decision that BSD stays
  build-only;
- add the C interop run to macOS if the C build is clean there;
- add the Windows C reference build and C↔Rust binary test if the
  MinGW build is clean, or record the CI gap;
- update `AGENTS.md`, `README.md`, `tests/README.md`, the rewrite plan,
  and the decisions with the final platform matrix and known differences;
- run the full C and Rust suites, the Windows smoke test, and the CI
  matrix.

- **Status:** Implemented
- **Depends on:** Slices 1–5
- **Requirements:** Checklist items 1 and 4; design §3 success
  criteria, §4 test strategy, Phase 8
- **Manual prerequisites:** GitHub CI; a Windows runner for the C
  reference and the smoke test
- **Plan:** [plan](checklist-gaps-plan-modern-platform-ci-and-release.md)
- **Done when:** the full matrix is green, every checklist item is
  accounted for, the platform differences are documented, and the
  `windows-smoke.ps1` run has passed on a real runner

## Definition Of Done

- All six slices are implemented and their plans are marked done.
- The four checklist items are green: modern platform support, both
  server and client modes, both modes in library form, and the
  C↔Rust binary integration suite in CI.
- The C and Rust suites, the binary interop matrix, the Windows smoke
  test, and the platform jobs pass.
- The known differences are in the decisions and the README; no
  unchecked platform behavior remains in the checklist.
- No open Critical or Important review findings.

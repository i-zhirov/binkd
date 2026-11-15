# Free-Space Provider Implementation Plan

> Execute this plan with the `executing-plans` skill. Each task is a
> checkbox; mark it done when its changes are implemented and its
> Verify command passes.

- **Date**: 2026-11-15
- **Status**: In Progress
- **Design**: [rust-rewrite-plan.md](rust-rewrite-plan.md)
- **Roadmap**: [checklist-gaps-roadmap.md](checklist-gaps-roadmap.md) —
  slice: `platform-seam-free-space`
- **Depends on**: —
- **Manual prerequisites**: —
- **Goal**: thread a free-space provider from the binary platform layer
  to `binkd-proto::inb_fopen` and preserve the C kilobytes policy.

## Constraints

- The free space is in kilobytes, as in `unix/getfree.c` and
  `nt/getfree.c`; the conversion from bytes is integer division by 1024.
- A failed free-space query assumes enough space and answers `u64::MAX`,
  the value of the C `ULONG_MAX` error path.
- Use `fs2 = "=0.4.3"` through its safe `available_space()` API; add it
  to the `binkd` crate only. No workspace crate adds `unsafe`.
- The provider type is `fn(&Path) -> u64`, the free kilobytes of a
  directory.
- The absent provider answers `u64::MAX`; the binary always supplies the
  platform provider.
- The secure limit is `minfree`; the non-secure limit is
  `minfree-nonsecure`; `-1` disables the check.
- The need is
  `ceil((size - partial_size) / 1024) + limit`; a partial longer than the
  file is removed and retried.
- `temp-inbound` takes the minimum of the inbound and temporary-inbound
  free values, and only when the partial path starts with the temporary
  directory.
- The C and Rust corpus runners read the new case; the pure crates
  (`binkd-ftn`, `binkd-spool`, `binkd-proto`, `binkd-net`, `binkd-log`)
  make no platform call.
- No new CI job; the existing Linux, macOS, and Windows workspace jobs
  run the new tests.

## Tasks

- [x] **Task 1: Add the provider type and prove the policy**
    - **Implements:** design §3 file-system effects; roadmap
      `platform-seam-free-space`; the C `inb_fopen()` rule.
    - **Modify `rust/crates/binkd-spool/src/inbound.rs`**
        - `FreeSpaceFn` — `pub type FreeSpaceFn = fn(&Path) -> u64;`
        - `assume_enough` — `pub fn assume_enough(_: &Path) -> u64` returns
          `u64::MAX`.
        - Add unit tests in the module:
            - `the_minfree_rejects_a_small_disk` — secure session,
              `minfree = 11`, `FreeSpace { inbound: 10, temp_inbound: 10 }`,
              1-byte file; `inb_fopen` answers `None`.
            - `the_minfree_allows_a_large_disk` — the same case with
              `inbound = 12`; `inb_fopen` answers `Some`.
            - `the_minfree_nonsecure_holds_its_rule` — non-secure session,
              `minfree = 0`, `minfree-nonsecure = 11`; free 11 rejects,
              free 12 accepts.
            - `the_temp_inbound_takes_the_minimum` — `temp_inbound` is the
              case directory's `tmp`; free `inbound = 100`, `temp = 5`;
              `minfree = 10`; a file under `tmp` rejects; the same file with
              free `temp = 100` accepts.
        - The tests use a temporary directory, a `TFile`, a `SpoolConfig`,
          an `Inbound`, and a deterministic `rnd`; they remove the
          directory.
    - **Create `tests/golden/spool/inbound_minfree.tsv`**
        - The case `inbound_minfree`; `config scalar minfree 10` and
          `minfree_nonsecure 10`; a non-secure session; a 1-byte
          `13b70064.pkt`; the expected `inb_fopen 13b70064.pkt 1`.
    - **Modify `rust/crates/binkd-spool/tests/spool_corpus.rs`**
        - The `inb_fopen` operation passes
          `FreeSpace { inbound: u64::MAX, temp_inbound: u64::MAX }`, so the
          Rust runner accepts the non-negative case like the C runner with
          real free space.
    - **Modify `tests/stubs.c`**
        - The `getfree` stub answers `ULONG_MAX`, so the C unit-test harness
          assumes enough space like the C error path.
    - **Verify:** `cd rust && cargo test -p binkd-spool` and
      `make -C tests`.

- [x] **Task 2: Thread the provider through the session settings**
    - **Implements:** design §3 file-system effects; the platform seam of
      roadmap `platform-seam-free-space`.
    - **Modify `rust/crates/binkd-proto/src/session.rs`**
        - `SessionConfig.free_space: FreeSpaceFn` — the provider of the
          inbound directories; default `assume_enough` in every literal.
        - `Session::free_space_values(&self) -> FreeSpace` — call the
          provider for `config.inbound`; call it for
          `config.spool.temp_inbound` when the string is not empty, else
          use the inbound value; answer the two values.
        - The `start_file_recv()` call to `inb_fopen()` uses
          `self.free_space_values()` instead of `FreeSpace::default()`.
        - Update the `SessionConfig` literals in
          `rust/crates/binkd-proto/src/record.rs`,
          `rust/crates/binkd-proto/tests/session.rs`,
          `rust/crates/binkd-proto/tests/record.rs`, and
          `rust/crates/binkd-proto/tests/interop.rs` with
          `free_space: binkd_spool::inbound::assume_enough`.
        - Add `the_free_space_provider_reaches_the_inbound` to
          `rust/crates/binkd-proto/tests/session.rs`: a provider that
          answers 0, `minfree = 1`, a file transfer; the session answers
          `SKIP` and receives no file.
    - **Modify `rust/crates/binkd-net/src/session.rs`**
        - `SessionOptions.free_space: FreeSpaceFn` with the
          `assume_enough` default.
        - `SessionOptions::with_free_space(provider)` — set the provider.
        - `session_config()` initializes the new `SessionConfig.free_space`
          with `assume_enough`.
    - **Modify `rust/crates/binkd-net/src/listener.rs` and
      `rust/crates/binkd-net/src/caller.rs`**
        - Set `settings.free_space = options.free_space` after
          `session_config()`.
    - **Modify `rust/crates/binkd-net/tests/loopback.rs`**
        - Add `the_free_space_provider_of_the_options_reaches_the_session`:
          the server options use `with_free_space(|_| 0)`, the server
          spool has `minfree = 1`, the client sends one file; the report
          answers `skipped` and the server receives no file.
    - **Verify:** `cd rust && cargo test -p binkd-proto --test session the_free_space_provider_reaches_the_inbound`
      and
      `cargo test -p binkd-net --test loopback the_free_space_provider_of_the_options_reaches_the_session`.

- [x] **Task 3: Implement the Unix and Windows provider in the binary**
    - **Implements:** design §4.1 equivalence policy; the platform seam of
      roadmap `platform-seam-free-space`.
    - **Modify `rust/crates/binkd/Cargo.toml`**
        - Add `fs2 = "=0.4.3"` to `[dependencies]`; update
          `rust/Cargo.lock`.
    - **Modify `rust/crates/binkd/src/platform.rs`**
        - `free_space(path: &Path) -> u64` — `fs2::available_space(path)`
          divided by 1024; a failed query answers `u64::MAX`.
        - Add `the_free_space_of_a_directory` — a temporary directory
          answers more than zero.
        - Add `the_free_space_of_a_missing_directory` — a missing path
          answers `u64::MAX`.
    - **Modify `rust/crates/binkd/src/daemon.rs`**
        - `session_options()` sets `options.free_space =
          platform::free_space`.
    - **Modify `rust/crates/binkd/src/engine.rs`**
        - Both `SessionOptions` values set the platform provider.
    - **Verify:** `cd rust && cargo test -p binkd platform::tests` and
      `cargo check -p binkd --target x86_64-pc-windows-gnu`.

- [x] **Task 4: Update the documents and run the full verification**
    - **Implements:** the documentation obligation of the design and the
      repository.
    - **Modify `AGENTS.md`**
        - Add `fs2` (`=0.4.3`, the `binkd` crate only) to the dependency
          list and the reason.
    - **Modify `doc/rust-rewrite-plan.md`**
        - Record the provider and the `fs2` choice in the file-system
          effects and the dependency notes; remove the note that the
          free-space value is the default.
    - **Modify `tests/README.md`**
        - Name the new spool corpus case and the focused test commands.
    - **Verify:**
      `cd rust && cargo fmt --all --check`,
      `cargo clippy --workspace --all-targets`,
      `cargo test --workspace`,
      `cargo +1.85 check --workspace`,
      `cargo check -p binkd --target x86_64-pc-windows-gnu`,
      `cd .. && make -C tests`, and `git diff --check`.

## Deviations

- Task 1: the C spool unit-test harness answered 0 from its `getfree()`
  stub, but the new non-negative `minfree` case needs the C error-path
  value. `tests/stubs.c` now answers `ULONG_MAX`, and the Task 1 file
  list names the file.

## Done When

- A full transfer is rejected when the computed free kilobytes are below
  the threshold and succeeds above it.
- The `minfree`, `minfree-nonsecure`, and `temp-inbound` minimum rules
  have unit tests and the non-negative `minfree` corpus case passes in
  both runners.
- The provider uses a real temporary directory on Unix and Windows.
- The pure crates make no platform call.
- The Linux, macOS, and Windows CI jobs are green.

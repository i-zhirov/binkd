# Binary C and Rust Interop Implementation Plan

> Execute this plan with the `executing-plans` skill. Each task is a
> checkbox; mark it done when its changes are implemented and its
> Verify command passes.

- **Date**: 2026-11-15
- **Status**: Implemented
- **Design**: [rust-rewrite-plan.md](rust-rewrite-plan.md)
- **Roadmap**: [checklist-gaps-roadmap.md](checklist-gaps-roadmap.md) —
  slice: `binary-interop`
- **Depends on**: —
- **Manual prerequisites**: a plain C reference build and `BINKD_C`
  for local runs
- **Goal**: prove the C and Rust binaries interoperate in both roles
  and establish the C↔C baseline.

## Constraints

- The test file is `crates/rbinkd/tests/interop_binary.rs`.
- The Rust binary path comes from `env!("CARGO_BIN_EXE_rbinkd")`.
- The C binary path comes from `BINKD_C`; the tests skip when it is
  absent.
- The server address is `2:5047/100@fidonet`; the client address is
  `2:5047/13@fidonet`; the password is `secret`.
- The server packet is `13b7000d.cut`; the client packet is
  `13b70064.cut`; each side sends one packet in each direction.
- The test asserts the exact inbound bytes, the client exit code 0, and
  a `BEGIN, binkd/` line in the server and client logs.
- The C↔C baseline uses two C binaries and the same packet fixture.
- No production code and no new dependency are added by this slice.
- The CI step runs in the existing `interop` job.

## Tasks

- [x] **Task 1: Add the binary harness and the two C/Rust directions**
    - **Implements:** checklist item 4; design §4 Level 4, the
      `Rust | C` and `C | Rust` rows; roadmap slice `binary-interop`
    - **Create `crates/rbinkd/tests/interop_binary.rs`**
        - `fn rust_binary() -> PathBuf` — returns
          `env!("CARGO_BIN_EXE_rbinkd")`.
        - `fn c_binary() -> Option<PathBuf>` — reads `BINKD_C` and
          answers `None` when the file is absent.
        - `fn free_port() -> u16` — uses a process-unique atomic
          counter, as `binkd-proto/tests/interop.rs` does, not a closed
          listener.
        - `fn temp_dir(name: &str) -> PathBuf` — makes `outb` and `in`
          under the system temporary directory and removes the old one.
        - `fn write_c_server_config(dir: &Path, port: u16) -> PathBuf`
          — writes `binkd.conf` with `domain fidonet <dir>/outb 2`,
          `sysname`, `sysop`, `location`, `nodeinfo`, the server
          `address`, `inbound in`, `iport`, `listen
          127.0.0.1:<port>`, `node 2:5047/13@fidonet 127.0.0.1 secret`,
          `log c-server.log`, `loglevel 8`, and `conlog 0`.
        - `fn write_rust_server_config(dir: &Path, port: u16) ->
          PathBuf` — the same directives for the Rust binary, without
          `iport`, with `log rust-server.log`.
        - `fn write_c_client_config(dir: &Path, port: u16) -> PathBuf`
          — `domain fidonet <dir>/outb 2`, the client `address`,
          `inbound in`, `sysname`, `sysop`, `location`, `nodeinfo`,
          `node 2:5047/100@fidonet 127.0.0.1:<port> secret`,
          `zallow *`, `log c-client.log`, `loglevel 8`, and `conlog 0`.
        - `fn write_rust_client_config(dir: &Path, port: u16) ->
          PathBuf` — the same directives with `log rust-client.log`.
        - `struct Server { child: Child }` with
          `fn c(dir: &Path, port: u16) -> Server` and
          `fn rust(dir: &Path, port: u16) -> Server` constructors and a
          `Drop` that kills and waits for the child.
        - `fn run_c_client(dir: &Path, node: &str) -> ExitStatus` —
          runs `binkd -P <node> -p binkd.conf`.
        - `fn run_rust_client(dir: &Path, node: &str) -> ExitStatus` —
          runs the same arguments with `rust_binary()`.
        - `fn wait_for_port(port: u16) -> bool` — retries the loopback
          connection for a bounded time.
        - `fn wait_for_content(dir: &Path, content: &[u8]) -> bool` —
          scans the inbound directory for a file with the exact bytes.
        - `fn assert_log(dir: &Path, name: &str, text: &str)` — reads
          `c-server.log`, `c-client.log`, `rust-server.log`, or
          `rust-client.log` and asserts the line.
        - `fn packet(dir: &Path, name: &str, size: usize) -> Vec<u8>` —
          creates the file and answers its bytes.
        - `the_rust_client_calls_the_c_server` — C server and Rust
          client; both packets; both inbound directories; both logs;
          client exit code 0.
        - `the_c_client_calls_the_rust_server` — Rust server and C
          client; the same assertions.
    - **Verify:**
      `BINKD_C=/path/to/binkd cargo test -p rbinkd --test interop_binary`

- [x] **Task 2: Add the C↔C baseline and the CI step**
    - **Implements:** checklist item 4; design §4 Level 4, the `C | C`
      row; roadmap slice `binary-interop`
    - **Modify `crates/rbinkd/tests/interop_binary.rs`**
        - `the_c_client_calls_the_c_server` — starts the C server and
          runs the C client; asserts both inbound packets, both logs,
          and the client exit code 0.
    - **Modify `.github/workflows/ci.yml`**
        - Add a `Run the binary interop suite` step to the `interop`
          job, with `BINKD_C` set to the built C binary and
          `cargo test -p rbinkd --test interop_binary`.
    - **Verify:**
      `BINKD_C=/path/to/binkd cargo test -p rbinkd --test interop_binary`
      and review the `interop` job step.

- [x] **Task 3: Document the binary matrix**
    - **Implements:** checklist item 4; design §4 Level 4; roadmap
      slice `binary-interop`
    - **Modify `tests/README.md`**
        - In `## The interop tests`, add the
          `crates/rbinkd/tests/interop_binary.rs` command and state that
          it covers C server ↔ Rust client, Rust server ↔ C client, and
          C server ↔ C client.
        - In the matrix, change the `C | C` row from the C test suite
          to the new binary baseline.
    - **Modify `doc/rust-rewrite-plan.md`**
        - Change the Level 4 `C | C` row from the C test suite to the
          new binary baseline, and add the test file to the Phase 6
          integration-test deliverable.
    - **Modify `AGENTS.md`**
        - In the Testing section, name the binary interop test beside
          the existing C/Rust interop suite.
    - **Verify:** `cargo fmt --all --check`; `rg` the new test path in
      the three documents.

## Verification

- Local, with the C reference:
  `BINKD_C=/path/to/binkd cargo test -p rbinkd --test interop_binary`.
- Without `BINKD_C`, the test reports a skip and passes.
- The full workspace still passes: `cargo test --workspace`.
- Formatting and lint: `cargo fmt --all --check` and
  `cargo clippy -p rbinkd --all-targets`.
- CI: the `interop` job runs the new test with the built C binary.

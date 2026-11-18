# Modern Platform CI and Release Implementation Plan

> Execute this plan with the `executing-plans` skill. Each task is a
> checkbox; mark it done when its changes are implemented and its
> Verify command passes.
>
> The plan is approved. The roadmap status and plan link are updated
> with the plan.

- **Date**: 2026-11-18
- **Status**: In Progress
- **Design**: [rust-rewrite-plan.md](rust-rewrite-plan.md), with
  [legacy-platforms.md](decisions/legacy-platforms.md) and
  [windows-support.md](decisions/windows-support.md)
- **Roadmap**: [checklist-gaps-roadmap.md](checklist-gaps-roadmap.md) —
  slice: `modern-platform-ci-and-release`
- **Depends on**: Slices 1–5 (all Implemented)
- **Manual prerequisites**: GitHub Actions with macOS and Windows runners;
  no accepted FreeBSD runner action is present, so the plan records the
  accepted build-supported absence
- **Goal**: add C↔Rust interop coverage on macOS and Windows, resolve the
  FreeBSD CI question, and document the final platform matrix.

## Constraints

- No production code changes. The only code changes are test-only:
   1. in `rust/crates/binkd/tests/interop_binary.rs`, a config path helper
      writes `/` separators, because the config tokenizer reads `\` as an
      escape, and the `BEGIN` assertion accepts both `BEGIN, binkd/` and
      the Windows `BEGIN standalone, binkd/` line;
   2. in `rust/crates/binkd-net/tests/interop.rs`, the `sending
      ./outb/13b70064.cut as` assertion normalizes `\` to `/` before the
      comparison, because the MinGW C log writes `.\outb\...`.
- No C source change, no dependency change, and no new workflow file. The
  CI change is in `.github/workflows/ci.yml` only.
- The new CI job names are exactly `interop-macos` and `interop-windows`.
  The existing jobs `rust`, `msrv`, `c`, `perl`, `interop`,
  `interop-proxy`, `interop-bwlim`, `interop-v10`, `macos`, and `windows`
  keep their names.
- The external action versions are exactly `actions/checkout@v4`,
  `dtolnay/rust-toolchain@stable`, and `msys2/setup-msys2@v2`.
- The macOS runner is `macos-latest`. The C reference uses the existing
  UNIX steps exactly:
  `cp mkfls/unix/* .`, `chmod +x mkinstalldirs`, `./configure`, `make`.
  `BINKD_C` is `${{ github.workspace }}/binkd`.
- The Windows runner is `windows-latest`. The MSYS2 environment is
  `MINGW64`; the packages are `make`, `mingw-w64-x86_64-gcc`,
  `mingw-w64-x86_64-binutils`, `mingw-w64-x86_64-zlib`, and
  `mingw-w64-x86_64-bzip2`.
- The Windows C build command is exactly:
  `make -f mkfls/nt95-mingw/Makefile CC=gcc ZLIB=1 BZLIB2=1
  OUTDIR=bin/mingw32-zlib-bzlib2
  CFLAGS='-std=gnu89 -mthreads -pipe -funsigned-char -Wall -Wno-char-subscripts
  -Wno-comment -O2 -s' LFLAGS='-static'`.
  The command-line `OUTDIR` pins the documented output path; the makefile
  would otherwise append `-binkd-zlib-bzlib2`. The command-line `CFLAGS`
  replaces the makefile `CFLAGS +=` line, so the obsolete `-mno-cygwin`
  flag is not passed.
- The Windows C binary is
  `bin/mingw32-zlib-bzlib2/binkd-mingw-zlib-bzlib2.exe`. `BINKD_C` is
  `${{ github.workspace }}\bin\mingw32-zlib-bzlib2\binkd-mingw-zlib-bzlib2.exe`.
  The PowerShell test step prepends `C:\msys64\mingw64\bin` to `PATH`.
- The macOS job runs the base protocol, network, record, config, CLI,
  daemon, and binary tests. The Windows job runs only the base protocol,
  network, record, and binary tests. The network, record, daemon, and
  binary tests use `--test-threads=1`.
- The special builds stay Ubuntu-only:
  `BINKD_C_PROXY` with `binkd-net/tests/proxy_interop.rs`,
  `BINKD_C_BWLIM` with `rust_client_to_c_server_rate_limited`, and
  `BINKD_C_V10` with `c_client_claims_freq_to_rust_server` and
  `c_client_skips_the_request_of_rust_server`. The macOS and Windows jobs
  leave the variables unset, so those tests skip.
- The Windows C build does not run `dump_corpus`, `reload_corpus`, `cli`,
  or `daemon`. The C Windows feature set, the `aso` keyword, and the C
  Windows paths are not the oracle of the Rust Windows build.
- FreeBSD is build-supported but not CI-tested. The repository holds no
  accepted FreeBSD runner action. The decision record
  `doc/decisions/platform-ci.md` states the absence and the exact files
  that carry it.
- No transient plan, slice, requirement, or task reference appears in CI
  names, comments, test names, or code strings. Documentation may link the
  roadmap and the decision records.
- The plan sets the roadmap slice to `In Progress` and links this plan.
  Task 5 changes only the slice status to `Implemented` after the local and
  CI verification passes.

## Target Platform Matrix

| Platform | Jobs | C build | Rust suite |
| --- | --- | --- | --- |
| Linux | `rust`, `msrv`, `c`, `perl`, `interop`, `interop-proxy`, `interop-bwlim`, `interop-v10` | plain and variants | yes |
| macOS | `macos`, `interop-macos` | UNIX plain | yes |
| Windows | `windows`, `interop-windows` | MinGW plain | yes |
| BSD | none | build-supported | build-supported |
| OS/2, DOS, Amiga, Windows 9x/Me | none | C only | no |

The interop coverage:

| Platform | Job | Base | Binary | Config, CLI, daemon | Proxy | Rate limit | Protocol 1.0 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Linux | `interop` | yes | yes | yes | no | no | no |
| Linux | `interop-proxy` | no | no | no | yes | no | no |
| Linux | `interop-bwlim` | no | no | no | no | yes | no |
| Linux | `interop-v10` | no | no | no | no | no | yes |
| macOS | `interop-macos` | yes | yes | yes | no | no | no |
| Windows | `interop-windows` | yes | yes | no | no | no | no |
| BSD | none | no | no | no | no | no | no |

## Open Risks And Dependencies

- **Modern MinGW:** the current C sources may fail the modern GCC because
  GCC 15 defaults to `gnu23`; the build pins `-std=gnu89`. The plan does not
  patch C source. If the build still fails, stop and report the compiler
  error; a C source change needs its own decision and its own CI and corpus
  check.
- **macOS compression headers:** the default `./configure` must find zlib
  and bzip2. The base C build was verified locally with the default
  libraries. If a future runner image drops a header, the bzip2 interop
  test fails and the job must install the library explicitly in a separate
  change.
- **macOS corpus:** the `dump_corpus` and `reload_corpus` cases compare
  the C and Rust dumps byte for byte. The base C build was verified locally
  on macOS; if a runner-image difference changes a dump, stop and report;
  do not weaken the oracle.
- **Windows log and paths:** the C Windows and Rust Windows binaries log
  `BEGIN standalone, binkd/`, and the config tokenizer reads `\` as an
  escape. The test-only helpers cover both; no production difference is
  claimed.
- **Windows frame log:** the C Windows server uses threads rather than
  `fork`, so the per-process-id filter of `c_frames_sent()` is a no-op. The
  Windows log should hold one banner sequence. If a duplicate banner line
  appears and the frame comparison fails, add a test-only Windows filter
  that keeps the session sequence; do not weaken the UNIX assertion.
- **Windows C feature set:** the C Windows build accepts `aso` and has no
  syslog. It is not the oracle of the config dump, the CLI, or the daemon.
  The Rust Windows parity tests remain in the `windows` job.
- **Windows session behavior:** the C Windows server uses threads rather
  than `fork`. The base protocol and network tests read the C log by process
  id and do not assume a fork. The network test normalizes the C client
  path separator. If another test assumes a fork, stop and report; do not
  weaken the assertion.
- **BSD:** no accepted FreeBSD runner action is in the repository. The plan
  records the accepted absence. A later FreeBSD job needs a new decision
  and an accepted action.
- **CI duration:** the Windows job installs MSYS2 packages and builds the C
  reference. The job has no service install and needs no administrator.

## Tasks

- [x] **Task 1: Add the macOS C-reference interop job**
    - **Implements:** roadmap slice `modern-platform-ci-and-release`;
      design §4 Level 4; checklist item 4; the final platform matrix.
    - **Modify `.github/workflows/ci.yml`**
        - Insert the `interop-macos` job after `interop-v10` and before
          `macos`:
          ```yaml
          interop-macos:
            runs-on: macos-latest
            env:
              BINKD_C: ${{ github.workspace }}/binkd
            steps:
              - uses: actions/checkout@v4
              - uses: dtolnay/rust-toolchain@stable
              - name: Build the C reference
                run: |
                  cp mkfls/unix/* .
                  chmod +x mkinstalldirs
                  ./configure
                  make
              - name: Run the C and Rust interop suite
                run: |
                  cd rust
                  cargo test -p binkd-proto --test interop -- --test-threads=1
                  cargo test -p binkd-net --test interop -- --test-threads=1
                  cargo test -p binkd-proto --test record -- --test-threads=1
                  cargo test -p binkd-config --test dump_corpus
                  cargo test -p binkd-config --test reload_corpus
                  cargo test -p binkd --test cli
                  cargo test -p binkd --test daemon -- --test-threads=1
              - name: Run the binary interop suite
                run: cd rust && cargo test -p binkd --test interop_binary -- --test-threads=1
          ```
        - The `interop` test file skips `rust_client_to_c_server_rate_limited`
          because `BINKD_C_BWLIM` is unset, and the two `c_client_*`
          protocol 1.0 tests because `BINKD_C_V10` is unset. Do not set
          either variable in this job.
        - The `daemon` test runs serially because it binds a port and sends
          signals. The corpus and CLI tests use the default parallelism.
    - **Verify:** on a macOS host, from the repository root:
      ```sh
      cp mkfls/unix/* .
      chmod +x mkinstalldirs
      ./configure
      make
      cd rust
      BINKD_C=$PWD/../binkd cargo test -p binkd-proto --test interop -- --test-threads=1
      BINKD_C=$PWD/../binkd cargo test -p binkd-net --test interop -- --test-threads=1
      BINKD_C=$PWD/../binkd cargo test -p binkd-proto --test record -- --test-threads=1
      BINKD_C=$PWD/../binkd cargo test -p binkd-config --test dump_corpus
      BINKD_C=$PWD/../binkd cargo test -p binkd-config --test reload_corpus
      BINKD_C=$PWD/../binkd cargo test -p binkd --test cli
      BINKD_C=$PWD/../binkd cargo test -p binkd --test daemon -- --test-threads=1
      BINKD_C=$PWD/../binkd cargo test -p binkd --test interop_binary -- --test-threads=1
      ```
      Review the YAML diff to confirm the job name and the `BINKD_C` value.

- [x] **Task 2: Add the Windows C-reference build and interop job**
    - **Implements:** roadmap slice `modern-platform-ci-and-release`;
      design §4 Level 4; checklist item 4; `windows-support.md`; the final
      platform matrix.
    - **Modify `.github/workflows/ci.yml`**
        - Insert the `interop-windows` job after `interop-macos` and before
          `macos`:
          ```yaml
          interop-windows:
            runs-on: windows-latest
            env:
              BINKD_C: '${{ github.workspace }}\bin\mingw32-zlib-bzlib2\binkd-mingw-zlib-bzlib2.exe'
            steps:
              - uses: actions/checkout@v4
              - uses: dtolnay/rust-toolchain@stable
              - uses: msys2/setup-msys2@v2
                with:
                  msystem: MINGW64
                  update: true
                  install: >-
                    make
                    mingw-w64-x86_64-gcc
                    mingw-w64-x86_64-binutils
                    mingw-w64-x86_64-zlib
                    mingw-w64-x86_64-bzip2
              - name: Build the C reference
                shell: msys2 {0}
                run: |
                  make -f mkfls/nt95-mingw/Makefile \
                    CC=gcc ZLIB=1 BZLIB2=1 \
                    OUTDIR=bin/mingw32-zlib-bzlib2 \
                    CFLAGS='-std=gnu89 -mthreads -pipe -funsigned-char -Wall -Wno-char-subscripts -Wno-comment -O2 -s' \
                    LFLAGS='-static'
                  test -f bin/mingw32-zlib-bzlib2/binkd-mingw-zlib-bzlib2.exe
              - name: Run the C and Rust interop suite
                shell: pwsh
                run: |
                  $env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
                  cd rust
                  cargo test -p binkd-proto --test interop -- --test-threads=1
                  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
                  cargo test -p binkd-net --test interop -- --test-threads=1
                  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
                  cargo test -p binkd-proto --test record -- --test-threads=1
                  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
              - name: Run the binary interop suite
                shell: pwsh
                run: |
                  $env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
                  cd rust
                  cargo test -p binkd --test interop_binary -- --test-threads=1
                  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
          ```
        - Do not add `dump_corpus`, `reload_corpus`, `cli`, or `daemon` to
          this job. Do not set `BINKD_C_PROXY`, `BINKD_C_BWLIM`, or
          `BINKD_C_V10`.
        - The `-static` link and the `PATH` prepend are both present. The
          `PATH` line is a safety measure for a toolchain DLL and does not
          replace the static zlib and bzip2 link.
    - **Modify `rust/crates/binkd/tests/interop_binary.rs`**
        - Add `fn config_path(path: &Path) -> String` beside `write_config`.
          It returns `path.display().to_string().replace('\\', "/")` and has
          a doc comment that the config tokenizer reads `\` as an escape.
        - Use `config_path(&dir.join("outb"))` for the `domain fidonet {} 2`
          argument in `write_c_server_config`, `write_rust_server_config`,
          `write_c_client_config`, and `write_rust_client_config`.
        - Replace `fn assert_log` with
          `fn assert_begin_log(dir: &Path, name: &str)`. It reads the log
          and asserts that one line contains either `BEGIN, binkd/` or
          `BEGIN standalone, binkd/`. Its doc comment states that the UNIX
          builds of both binaries log `BEGIN, binkd/`, and the Windows
          builds log `BEGIN standalone, binkd/` when not a service, the
          `WIN32` branch of `binkd.c` and the `#[cfg(windows)]` branch of
          `daemon.rs`. The old helper has no other caller, so it must not
          stay unused.
        - Replace these six calls with `assert_begin_log` and drop the text
          argument:
            - `assert_log(&server_dir, "c-server.log", "BEGIN, binkd/")`
            - `assert_log(&client_dir, "rust-client.log", "BEGIN, binkd/")`
            - `assert_log(&server_dir, "rust-server.log", "BEGIN, binkd/")`
            - `assert_log(&client_dir, "c-client.log", "BEGIN, binkd/")`
            - `assert_log(&server_dir, "c-server.log", "BEGIN, binkd/")`
            - `assert_log(&client_dir, "c-client.log", "BEGIN, binkd/")`
        - Do not change the packet assertions, the exit-code assertions,
          the `TIME` frame assertion, or the two-C-binaries flow. Only the
          log helper and the config path form change.
    - **Modify `rust/crates/binkd-net/tests/interop.rs`**
        - In `c_caller_to_rust_listener`, read the C client log, normalize
          `\` to `/`, and then assert that it contains
          `sending ./outb/13b70064.cut as `. The MinGW C client writes
          `.\outb\13b70064.cut`, and the assertion is the only fail of the
          Windows network test. The normalization is a no-op on Unix.
        - Do not change the received-file, packet-byte, or frame-message
          assertions.
    - **Verify:** from the repository root:
      ```sh
      make -f mkfls/nt95-mingw/Makefile -n CC=gcc ZLIB=1 BZLIB2=1 \
        OUTDIR=bin/mingw32-zlib-bzlib2 \
        CFLAGS='-std=gnu89 -mthreads -pipe -funsigned-char -Wall -Wno-char-subscripts -Wno-comment -O2 -s' \
        LFLAGS='-static' all | rg 'bin/mingw32-zlib-bzlib2/binkd-mingw-zlib-bzlib2.exe'
      cd rust
      cargo test -p binkd-net --test interop
      cargo test -p binkd --test interop_binary
      cargo fmt --check
      ```
      The `make -n` command confirms the exact output path. The tests without
      `BINKD_C` compile and skip. The `interop-windows` job in Task 5 runs the
      real build and tests.

- [x] **Task 3: Record the FreeBSD decision and the platform matrix**
    - **Implements:** roadmap `modern-platform-ci-and-release`; the roadmap
      decision "BSD CI"; design §2.3; checklist item 1.
    - **Create `doc/decisions/platform-ci.md`** with these exact sections
      and text:
        ````markdown
        # Decision: the platform CI matrix

        Status: accepted. The base C and Rust interop matrix runs on Linux,
        macOS, and Windows. BSD is build-supported but not CI-tested.

        ## Context

        The C code is the reference and the oracle of the tests. The Rust
        workspace is the default build. The CI must run the C and Rust
        suites on the systems that the port supports, and it must state the
        systems that it does not run.

        The existing jobs `interop`, `interop-proxy`, `interop-bwlim`, and
        `interop-v10` run on Ubuntu. The `macos` and `windows` jobs run the
        Rust workspace suite, and the `windows` job runs the parity tests
        and the service smoke test. Neither of those two jobs builds the C
        reference.

        The repository holds no accepted GitHub runner action for FreeBSD.
        A third-party action would add a dependency of the CI on a project
        that the team does not control.

        ## Options

        1. Add a FreeBSD job with a third-party runner action. This option
           gives CI coverage of BSD, and it adds an external action and a
           runner that the team does not control.
        2. Build the base C and Rust matrix on macOS and Windows, and keep
           BSD build-supported and not CI-tested. This option covers the two
           systems that the team runs, and it states the BSD gap.
        3. Add no platform job and keep the C interop on Ubuntu. This
           option leaves the macOS and Windows C builds untested.

        ## Decision

        The team takes option 2. The CI runs the base C and Rust matrix on
        Linux, macOS, and Windows. BSD stays build-supported and not
        CI-tested until an accepted FreeBSD runner action is available.

        The platform matrix:

        | Platform | Jobs | C build | Rust suite |
        | --- | --- | --- | --- |
        | Linux | `rust`, `msrv`, `c`, `perl`, `interop`, `interop-proxy`, `interop-bwlim`, `interop-v10` | plain and variants | yes |
        | macOS | `macos`, `interop-macos` | UNIX plain | yes |
        | Windows | `windows`, `interop-windows` | MinGW plain | yes |
        | BSD | none | build-supported | build-supported |
        | OS/2, DOS, Amiga, Windows 9x/Me | none | C only | no |

        The interop coverage:

        | Platform | Job | Base | Binary | Config, CLI, daemon | Proxy | Rate limit | Protocol 1.0 |
        | --- | --- | --- | --- | --- | --- | --- | --- |
        | Linux | `interop` | yes | yes | yes | no | no | no |
        | Linux | `interop-proxy` | no | no | no | yes | no | no |
        | Linux | `interop-bwlim` | no | no | no | no | yes | no |
        | Linux | `interop-v10` | no | no | no | no | no | yes |
        | macOS | `interop-macos` | yes | yes | yes | no | no | no |
        | Windows | `interop-windows` | yes | yes | no | no | no | no |
        | BSD | none | no | no | no | no | no | no |

        ## The Windows C build

        The `interop-windows` job installs `make`,
        `mingw-w64-x86_64-gcc`, `mingw-w64-x86_64-binutils`,
        `mingw-w64-x86_64-zlib`, and `mingw-w64-x86_64-bzip2` in the MINGW64
        environment. It builds:

        ```sh
        make -f mkfls/nt95-mingw/Makefile \
          CC=gcc ZLIB=1 BZLIB2=1 \
          OUTDIR=bin/mingw32-zlib-bzlib2 \
          CFLAGS='-std=gnu89 -mthreads -pipe -funsigned-char -Wall -Wno-char-subscripts -Wno-comment -O2 -s' \
          LFLAGS='-static'
        ```

        The command-line `OUTDIR` pins the output path
        `bin/mingw32-zlib-bzlib2/binkd-mingw-zlib-bzlib2.exe`; the makefile
        would otherwise append `-binkd-zlib-bzlib2`. The command-line
        `CFLAGS` replaces the makefile `CFLAGS +=` line, so the obsolete
        `-mno-cygwin` flag is not passed. `LFLAGS='-static'` links `libz.a`
        and `libbz2.a` into the executable.

        `BINKD_C` is
        `${{ github.workspace }}\bin\mingw32-zlib-bzlib2\binkd-mingw-zlib-bzlib2.exe`.
        The PowerShell test step adds `C:\msys64\mingw64\bin` to `PATH` as
        a safety measure for a toolchain DLL.

        The C Windows build differs in these points:

        - It defines `AMIGADOS_4D_OUTBOUND` and accepts `aso`; the Rust
          Windows build rejects `aso`.
        - It has no syslog backend; the Rust Windows build rejects `syslog`.
        - It accepts `limit-rate` only with `BW_LIM`; the Rust Windows build
          accepts it.
        - It accepts the compression keywords only with `WITH_ZLIB` or
          `WITH_BZLIB2`; the Rust Windows build accepts them.
        - It can load zlib and bzip2 dynamically with `ZLIBDL`; the Rust
          port cannot.

        The C Windows binary is not the oracle of the config dump, the CLI,
        or the daemon. The `interop-windows` job runs only the base
        protocol, network, record, and binary tests.

        The C Windows binary and the Rust Windows binary log
        `BEGIN standalone, binkd/`, the `WIN32` branch of `binkd.c` and the
        `#[cfg(windows)]` branch of `daemon.rs`; the UNIX builds of both log
        `BEGIN, binkd/`. The binary interop test accepts both forms.

        ## The special builds

        - `interop-proxy` builds the C binary with `--with-proxy --with-ntlm`
          and runs `binkd-net/tests/proxy_interop.rs` with
          `BINKD_C_PROXY`.
        - `interop-bwlim` builds the C binary with `--with-bwlim` and runs
          `rust_client_to_c_server_rate_limited` with `BINKD_C_BWLIM`.
        - `interop-v10` builds the C binary with `-DPRTCLVER=\"1.0\"` and
          runs `c_client_claims_freq_to_rust_server` and
          `c_client_skips_the_request_of_rust_server` with
          `BINKD_C_V10`.

        The macOS and Windows jobs leave those variables unset, so the tests
        skip.

        ## Consequences

        - The C reference builds on Linux, macOS, and Windows in CI.
        - The base C↔Rust matrix runs on the three systems.
        - The proxy, rate-limit, and protocol 1.0 variants stay Ubuntu-only.
        - BSD is build-supported and not CI-tested. The README, the tests
          README, `AGENTS.md`, and the rewrite plan state the gap.
        - A FreeBSD job needs a new decision and an accepted runner action.
        ````
        - Use compact tables where a table exceeds 80 characters. The
          project Markdown rule allows the compact form with single spaces
          and `| --- |`.
    - **Modify `doc/decisions/legacy-platforms.md`**
        - Add this consequence after the last consequence:
          ```
          - BSD is not a platform of this record: it is a supported UNIX
            target of the Rust port. It is build-supported but not CI-tested,
            the record `doc/decisions/platform-ci.md`.
          ```
    - **Modify `doc/decisions/windows-support.md`**
        - Add this section after `## Status` and before
          `## Parity of the Rust build`:
          ```
          ## CI coverage

          The `interop-windows` job of `.github/workflows/ci.yml` builds the C
          Windows reference with MSYS2/MinGW64 and runs the base C↔Rust
          protocol, network, record, and binary tests. The build command, the
          output path, and the limits of the job are in
          `doc/decisions/platform-ci.md`. The job does not run the config
          corpus, the CLI, or the daemon with the C Windows binary, because
          the C Windows feature set is not the oracle of the Rust Windows
          build. The Rust Windows parity tests stay in the `windows` job. The
          C Windows and Rust Windows binaries log `BEGIN standalone, binkd/`;
          the binary interop test accepts that line and the UNIX
          `BEGIN, binkd/` line.
          ```
    - **Verify:**
      ```sh
      rg -n "build-supported but not CI-tested|interop-macos|interop-windows" \
        doc/decisions/platform-ci.md doc/decisions/legacy-platforms.md \
        doc/decisions/windows-support.md
      git diff --check
      ```

- [x] **Task 4: Align the core documents with the platform matrix**
    - **Implements:** roadmap `modern-platform-ci-and-release`; design §2.3
      and §4; AGENTS contribution rules for documentation.
    - **Modify `AGENTS.md`**
        - In Technical Context, change the Target Platform bullet to:
          ```
          - **Target Platform**: UNIX (Linux, macOS, BSD) and Windows NT and
            later. The Rust port supports the Windows service mode; the tray
            icon is not supported. The C code also targets OS/2, DOS, Amiga,
            and Windows 9x/Me. Linux, macOS, and Windows are CI-tested; BSD is
            build-supported but not CI-tested.
          ```
        - In Build And Test Commands, replace the last two lines of the
          interop bullet with:
          ```
          The CI jobs `interop`, `interop-proxy`, `interop-bwlim`, and
          `interop-v10` run the plain, proxy, rate limit, and protocol 1.0
          matrices on Ubuntu. The `interop-macos` job runs the base matrix on
          macOS, and the `interop-windows` job runs the base protocol,
          network, record, and binary matrix on Windows with the MinGW C
          reference.
          ```
        - In Testing, after the binary interop paragraph, add:
          ```
          - The `interop-macos` job builds the UNIX C reference and runs the
            base protocol, network, record, config, CLI, daemon, and binary
            tests. The `interop-windows` job builds the MinGW C reference and
            runs the base protocol, network, record, and binary tests. The
            config corpus, CLI, and daemon tests are not the Windows C oracle.
            The proxy, rate-limit, and protocol 1.0 variants stay Ubuntu-only.
            BSD is build-supported but not CI-tested, the record
            `doc/decisions/platform-ci.md`.
          ```
        - In Testing, change the CI bullet to:
          ```
          - The CI runs the workspace suite on Linux, macOS, and Windows. The
            `interop-macos` and `interop-windows` jobs run the C↔Rust matrix;
            BSD is not in CI. The Unix-only signal and shell cases carry
            `#[cfg(unix)]`.
          ```
        - In Project Structure, change the `ci.yml` comment to:
          `# CI: Rust, C, interop, MSRV, Perl, macOS, Windows`.
    - **Modify `README.md`**
        - After the C reference paragraph in `## Compiling`, add:
          ```
          CI builds and tests the C reference on Linux, macOS, and Windows; the
          Windows build uses MSYS2/MinGW64 and runs the base C↔Rust matrix.
          BSD is build-supported but not CI-tested.
          ```
        - In `## Port status`, add:
          ```
          - Linux, macOS, and Windows are CI-tested. BSD is build-supported but
            not CI-tested: the C reference and the Rust workspace build on BSD,
            and no CI runner is configured. See
            `doc/decisions/platform-ci.md`.
          ```
    - **Modify `tests/README.md`**
        - In `## The interop tests`, replace the paragraph that starts
          with `The CI jobs` and names the four interop jobs with this
          subsection:
          ````markdown
          ### The platform jobs

          The `interop` job builds the plain C reference on Ubuntu and runs
          the base matrix and the binary-level test.

          The `interop-macos` job builds the UNIX C reference on a macOS
          runner and runs the base matrix:

          ```sh
          cp mkfls/unix/* .
          chmod +x mkinstalldirs
          ./configure
          make
          cd rust
          BINKD_C=$PWD/../binkd cargo test -p binkd-proto --test interop -- --test-threads=1
          BINKD_C=$PWD/../binkd cargo test -p binkd-net --test interop -- --test-threads=1
          BINKD_C=$PWD/../binkd cargo test -p binkd-proto --test record -- --test-threads=1
          BINKD_C=$PWD/../binkd cargo test -p binkd-config --test dump_corpus
          BINKD_C=$PWD/../binkd cargo test -p binkd-config --test reload_corpus
          BINKD_C=$PWD/../binkd cargo test -p binkd --test cli
          BINKD_C=$PWD/../binkd cargo test -p binkd --test daemon -- --test-threads=1
          BINKD_C=$PWD/../binkd cargo test -p binkd --test interop_binary -- --test-threads=1
          ```

          The `interop-windows` job builds the C reference with MSYS2/MinGW64.
          From the repository root in a MINGW64 shell:

          ```sh
          make -f mkfls/nt95-mingw/Makefile \
            CC=gcc ZLIB=1 BZLIB2=1 \
            OUTDIR=bin/mingw32-zlib-bzlib2 \
            CFLAGS='-std=gnu89 -mthreads -pipe -funsigned-char -Wall -Wno-char-subscripts -Wno-comment -O2 -s' \
            LFLAGS='-static'
          ```

          The binary is
          `bin/mingw32-zlib-bzlib2/binkd-mingw-zlib-bzlib2.exe`. Set
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

          The special builds stay on Ubuntu. `interop-proxy` builds the proxy
          C binary and runs `proxy_interop`; `interop-bwlim` builds the
          rate-limit C binary and runs
          `rust_client_to_c_server_rate_limited`; `interop-v10` builds the
          protocol 1.0 C binary and runs
          `c_client_claims_freq_to_rust_server` and
          `c_client_skips_the_request_of_rust_server`. The macOS and Windows
          jobs leave `BINKD_C_PROXY`, `BINKD_C_BWLIM`, and `BINKD_C_V10`
          unset, so those tests skip.

          BSD is build-supported but not CI-tested. The C reference and the
          Rust workspace build on BSD, and no CI runner is configured. The
          record `doc/decisions/platform-ci.md` holds the matrix.
          ````
        - Add a note that the binary test accepts both `BEGIN` forms:
          ```
          The binary interop test accepts `BEGIN, binkd/` and the Windows
          `BEGIN standalone, binkd/` line. It writes config paths with `/`,
          because the config tokenizer reads `\` as an escape.
          ```
    - **Modify `doc/rust-rewrite-plan.md`**
        - In §2.3, after the paragraph on the Rust targets, add:
          ```
          The CI matrix covers Linux, macOS, and Windows. The `interop-macos`
          and `interop-windows` jobs run the C↔Rust base matrix; BSD is
          build-supported but not CI-tested, the decision of
          `doc/decisions/platform-ci.md`.
          ```
        - In §4 Level 4, replace the CI paragraph with:
          ```
          The CI jobs `interop`, `interop-proxy`, `interop-bwlim`, and
          `interop-v10` build the C binary on Ubuntu and set `BINKD_C`,
          `BINKD_C_PROXY`, `BINKD_C_BWLIM`, and `BINKD_C_V10`. The
          `interop-macos` job builds the UNIX C reference on macOS and runs the
          base matrix with `BINKD_C`. The `interop-windows` job builds the
          MinGW C reference and runs the base protocol, network, record, and
          binary matrix with `BINKD_C`; the proxy, rate-limit, and protocol
          1.0 variants stay Ubuntu-only. The `rust`, `macos`, and `windows` jobs run the
          Rust-only direction; the `windows` job also runs the parity tests and
          the service smoke test. BSD is build-supported but not CI-tested.
          ```
        - In Phase 8, replace the CI deliverable bullet with:
          ```
          - CI runs both suites. The `interop`, `interop-proxy`,
            `interop-bwlim`, and `interop-v10` jobs run the C and Rust matrix
            on Ubuntu. The `interop-macos` job runs the base matrix on macOS,
            and the `interop-windows` job runs the base protocol, network,
            record, and binary matrix on Windows. The `macos` and `windows` jobs run the workspace
            suite; the Windows job runs the parity tests and the service smoke
            test. BSD is build-supported but not CI-tested.
          ```
        - In the cutover status, replace the `ci.yml` bullet with:
          ```
          - `.github/workflows/ci.yml` runs the Rust suite, the C suite, the C
            and Rust interop jobs on Ubuntu, the `interop-macos` and
            `interop-windows` jobs, the MSRV check, the Perl hooks with the C
            oracle, the macOS workspace suite, and the Windows workspace,
            parity, and service smoke tests.
          ```
    - **Verify:**
      ```sh
      rg -n "interop-macos|interop-windows" AGENTS.md README.md tests/README.md doc/rust-rewrite-plan.md
      rg -n "build-supported but not CI-tested" AGENTS.md README.md tests/README.md doc/rust-rewrite-plan.md
      rg -n "platform-ci.md" AGENTS.md README.md tests/README.md doc/rust-rewrite-plan.md
      git diff --check
      ```

- [ ] **Task 5: Run the full verification and update the slice status**
    - **Implements:** roadmap `modern-platform-ci-and-release`; design §3
      success criteria and Phase 8; the Definition Of Done.
    - **Run the local macOS C build and interop** from the repository root:
      ```sh
      cp mkfls/unix/* .
      chmod +x mkinstalldirs
      ./configure
      make
      cd rust
      BINKD_C=$PWD/../binkd cargo test -p binkd-proto --test interop -- --test-threads=1
      BINKD_C=$PWD/../binkd cargo test -p binkd-net --test interop -- --test-threads=1
      BINKD_C=$PWD/../binkd cargo test -p binkd-proto --test record -- --test-threads=1
      BINKD_C=$PWD/../binkd cargo test -p binkd-config --test dump_corpus
      BINKD_C=$PWD/../binkd cargo test -p binkd-config --test reload_corpus
      BINKD_C=$PWD/../binkd cargo test -p binkd --test cli
      BINKD_C=$PWD/../binkd cargo test -p binkd --test daemon -- --test-threads=1
      BINKD_C=$PWD/../binkd cargo test -p binkd --test interop_binary -- --test-threads=1
      cd ..
      ```
    - **Run the full local suites:**
      ```sh
      cd rust
      cargo test --workspace
      cargo fmt --check
      cargo clippy --workspace --all-targets
      cd ..
      make -C tests
      ```
    - **Run the Windows smoke test** on a Windows host with an
      administrator PowerShell:
      ```powershell
      cd rust
      cargo build --release -p binkd
      cd ..
      pwsh -NoProfile -ExecutionPolicy Bypass -File tests/windows-smoke.ps1
      ```
    - **Confirm the CI matrix** on the branch run of the `CI` workflow.
      Every job must be green:
      `rust`, `msrv`, `c`, `perl`, `interop`, `interop-proxy`,
      `interop-bwlim`, `interop-v10`, `interop-macos`, `interop-windows`,
      `macos`, `windows`.
      The `windows` job includes the service smoke test.
    - **Modify `doc/checklist-gaps-roadmap.md` only after every command and
      CI job above passes:**
        - In the "Decisions To Make In The Slice Plans" section, change the
          BSD bullet to:
          ```
          - **BSD CI**: documented as build-supported but not CI-tested, the
            decision of `doc/decisions/platform-ci.md`. A FreeBSD job needs an
            accepted runner action and a new decision.
          ```
        - In `### Slice 6 — modern-platform-ci-and-release`, change:
          ```
          - **Status:** In Progress
          ```
          to:
          ```
          - **Status:** Implemented
          ```
        - Set the top-level roadmap `Status` to `Implemented`: this is the
          last slice. Do not change the date or the other slices. The plan
          link is already set.
    - **Verify:**
      ```sh
      rg -n "Status:\*\* Implemented|checklist-gaps-plan-modern-platform-ci-and-release|platform-ci.md" doc/checklist-gaps-roadmap.md
      git diff --check
      ```

## Deviations

- The Windows C build pins `-std=gnu89` in `CFLAGS`. The approved plan had
  no dialect flag. The MinGW GCC default mode rejects the implicit
  declaration of `Is9x()` in `run.c`; `gnu89` compiles the reference. The
  cross-build with MinGW 14 succeeded without the optional zlib and bzip2
  libraries.
- The roadmap slice was set to `In Progress` with the plan link during
  pre-flight. Task 5 changes the slice to `Implemented` after the remaining
  CI verification.
- The Windows C reference cross-build on this host did not build the
  optional zlib and bzip2 libraries; the `interop-windows` job remains the
  verification of the full Windows build and runtime.
- The test-only changes in `interop_binary.rs` and
  `binkd-net/tests/interop.rs` are the approved Windows normalizations.
  No production behavior changed.

## Verification

Run these commands from the repository root unless the command changes
directory:

```sh
# macOS C reference and the base C↔Rust matrix
cp mkfls/unix/* .
chmod +x mkinstalldirs
./configure
make
cd rust
BINKD_C=$PWD/../binkd cargo test -p binkd-proto --test interop -- --test-threads=1
BINKD_C=$PWD/../binkd cargo test -p binkd-net --test interop -- --test-threads=1
BINKD_C=$PWD/../binkd cargo test -p binkd-proto --test record -- --test-threads=1
BINKD_C=$PWD/../binkd cargo test -p binkd-config --test dump_corpus
BINKD_C=$PWD/../binkd cargo test -p binkd-config --test reload_corpus
BINKD_C=$PWD/../binkd cargo test -p binkd --test cli
BINKD_C=$PWD/../binkd cargo test -p binkd --test daemon -- --test-threads=1
BINKD_C=$PWD/../binkd cargo test -p binkd --test interop_binary -- --test-threads=1
cd ..

# Full Rust and C suites
cd rust
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets
cd ..
make -C tests
```

The Windows runner commands:

```powershell
cd rust
cargo build --release -p binkd
cd ..
pwsh -NoProfile -ExecutionPolicy Bypass -File tests/windows-smoke.ps1
```

The CI jobs that must pass are `rust`, `msrv`, `c`, `perl`, `interop`,
`interop-proxy`, `interop-bwlim`, `interop-v10`, `interop-macos`,
`interop-windows`, `macos`, and `windows`.

The macOS and Windows C build paths, the exact test lists, the special-build
skips, and the FreeBSD absence are recorded in
`doc/decisions/platform-ci.md`. The core documents,
`AGENTS.md`, `README.md`, `tests/README.md`, and
`doc/rust-rewrite-plan.md`, link that record and name the jobs.

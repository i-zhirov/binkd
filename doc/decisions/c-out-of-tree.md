# Decision: the C implementation out of the tree

Status: accepted and implemented. The Rust workspace in `rust/` is the only
implementation in this repository and the default on UNIX and Windows. The C
implementation lives upstream in `https://github.com/pgul/binkd`. The interop
tests clone it at a pinned commit and apply `ci/upstream-c.patch`.

## Context

The Rust rewrite reached its cutover. The workspace builds the binary, the
library, the server and client modes, the Windows service mode, and every
feature of the C implementation. The C tree stayed in the repository as the
reference and as the oracle of the differential tests.

The local C tree carried a cost:

- It duplicated the upstream C source and the Rust source in one repository.
- It held the C build system in `mkfls/`, the platform layers in `unix/`,
  `nt/`, `os2/`, `dos/`, and `amiga/`, the C test harness in `tests/`, the
  `Config.h` release value, and the Debian packaging in `debian/`.
- A change to the C code had to match the upstream C code, but the two copies
  could drift.

The C implementation is still necessary. It is the reference of the interop
tests, and it is the only build of OS/2, DOS, Amiga, and Windows 9x/Me, the
record `doc/decisions/legacy-platforms.md`.

## Options

1. Keep the local C tree beside the Rust workspace. This option keeps the C
   source in the repository, and it keeps the duplicate tree and the drift.
2. Remove the local C tree and clone the upstream C reference in the interop
   jobs. This option gives one implementation in the repository and keeps the
   C oracle, the choice of this record.
3. Remove the C tree and stop the interop tests. This option gives one
   implementation and one tree, and it loses the differential oracle and the
   legacy platforms.

## Decision

The repository takes option 2.

- The Rust workspace in `rust/` is the only implementation in this tree and
  the default on UNIX and Windows NT and later.
- The local C tree is removed: the C source, `mkfls/`, the platform
  directories, the C test harness, `Config.h`, and the Debian C packaging.
- The interop tests clone `https://github.com/pgul/binkd.git` at commit
  `5a84e8596a96ca8b1f4ec69d9a608cd264aa06ba`, apply `ci/upstream-c.patch`,
  and build the C reference. The `.github/actions/upstream-c` action holds the
  clone and the patch steps.
- The patch holds only the changes that the interop tests need: the
  `PRTCLVER` override of `Config.h`, the exit of `-n`, the `PROTOTYPES` define
  of `md5b.c`, and the bzip2 type of the configure test.

## Consequences

- The repository holds one implementation. A reader, a contributor, and a CI
  job find the Rust code at the root and the C code upstream.
- The Rust tests are the only test suite in the tree. The golden corpus in
  `tests/golden/` stays, and the Rust tests read it. The former C runner is
  gone.
- The upstream C implementation is the oracle only in the interop jobs. The
  jobs `interop`, `interop-proxy`, `interop-bwlim`, `interop-v10`,
  `interop-macos`, `interop-windows`, and `perl` clone and build it.
- OS/2, DOS, Amiga, and Windows 9x/Me use upstream C, not this repository,
  the record `doc/decisions/legacy-platforms.md`.
- The upstream C release process owns `HISTORY`, `PATCHLEVEL`, the spec file,
  and the Debian packaging. This repository holds no copy of them.
- The configuration contract stays: the Rust parser reads the same config
  files and matches the upstream `-d` dump.
- The documents name the upstream C source: `README.md`, `AGENTS.md`,
  `tests/README.md`, `doc/rust-rewrite-plan.md`, `binkd.8`, and the record
  `doc/decisions/platform-ci.md`.
- BSD stays build-supported and not CI-tested for the Rust workspace and for
  the upstream C reference, the record `doc/decisions/platform-ci.md`.

# Decision: the legacy platforms

Status: accepted. The Rust port does not support OS/2, DOS, and Amiga. The C
code stays in the tree as the reference and the build of the three. The team
accepted option 2.

## Context

The C code holds a platform layer for every system that it builds on:

| Directory | The layer | The files |
| --- | --- | --- |
| `unix/` | UNIX | the daemonize, the sockets, the file system |
| `nt/` | Windows | the sockets, the service mode, the tray icon |
| `os2/` | OS/2 | the directory calls, the free space, the threads |
| `dos/` | DOS | the directory calls, the free space, the sleep |
| `amiga/` | Amiga | the free space, the rename, the semaphores |

The core modules hold the platform guards: `protocol.c`, `server.c`, and
`client.c` name `OS2`, `__DOS__`, and `AMIGA` in `#ifdef` blocks, the port of
the parts of the session that the systems differ on.

The Rust workspace targets UNIX and Windows, the two systems of
`AGENTS.md`: `std` holds the sockets, the files, and the threads of the two, and
the port of the platform layer of `binkd-net` and of `binkd-log` covers them.
`std` states no directory call, no free space, and no semaphore of OS/2, DOS, or
Amiga, and the three systems hold no current toolchain of the workspace.

## Options

1. Port the three platforms. This option gives one language, and it needs three
   systems that hold no toolchain of the workspace, and the port of a layer that
   `std` cannot express.
2. Keep the C code for the three platforms and mark them out of scope for the Rust
   port, the choice of this record. This option keeps the systems of the
   operators, and it keeps one C module of the platform layer of the tree.
3. Stop the support of the three platforms. This option gives one tree, and it
   breaks the systems of the operators who run them.

## Decision

The port takes option 2. The C code holds the reference and the build of OS/2,
DOS, and Amiga, and the record of this decision states the status of the three.
The Rust port covers UNIX and Windows, the port of the systems of the test suite
and of the oracle of the port. A system of the three that a maintainer asks for
gets its own record and its own phase. Windows 9x/Me is out of scope too, the
decision of `doc/decisions/windows-support.md`.

## Consequences

- The tree holds the C code and the Rust port for the platforms that the two share,
  and the C code alone for the three.
- The platform guards of `protocol.c`, `server.c`, and `client.c` stay in the C: the
  port of a core module holds the guards of the systems of the port.
- The `README.md` and the `doc/` state the three as out of scope, so an operator of
  the three knows the status.
- A change of the core modules must not break the build of the three: the C code of
  the three builds from the same sources.
- BSD is not a platform of this record: it is a supported UNIX
  target of the Rust port. It is build-supported but not CI-tested,
  the record `doc/decisions/platform-ci.md`.

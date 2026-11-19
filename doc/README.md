# Documentation

This repository holds the documentation for rbinkd, the Rust
reimplementation of Binkd. The upstream Binkd documentation is not copied
here. This index links to it.

## Rust documentation

- [README](../README.md): what rbinkd is, build, run, and status.
- [Rust rewrite plan](rust-rewrite-plan.md): scope, architecture, and port
  status.
- [Decisions](decisions/): architecture decision records.
- [Roadmap](checklist-gaps-roadmap.md): the implementation slices.
- [Manual](../rbinkd.8): command line, options, signals, and environment.
- [Tests](../tests/README.md): test suite, corpus, and interop tests.

The implementation checklists are historical records of the Rust rewrite.
They are not user documentation.

## Configuration samples

- [binkd.conf](../binkd.conf): the UNIX sample.
- [binkd.cfg](../binkd.cfg): the DOS and Windows sample.

The Rust parser reads these files. The comments in the samples are the
configuration reference.

## Upstream documentation

The upstream project owns the original Binkd documentation. Use these
links for the C implementation and the legacy platforms:

- [Upstream repository](https://github.com/pgul/binkd)
- [Upstream documentation directory](https://github.com/pgul/binkd/tree/master/doc)
- Binkd manual: [English](https://github.com/pgul/binkd/blob/master/binkd.txt)
- Binkd FAQ: [English](https://github.com/pgul/binkd/blob/master/binkdfaq-en.txt), [Russian](https://github.com/pgul/binkd/blob/master/binkdfaq-ru.txt)
- Windows 9x note: [English](https://github.com/pgul/binkd/blob/master/binkd9x-en.txt), [Russian](https://github.com/pgul/binkd/blob/master/binkd9x-ru.txt)
- [Change history](https://github.com/pgul/binkd/blob/master/HISTORY)
- [Release notes](https://github.com/pgul/binkd/blob/master/release-notes.txt)
- [Upstream TODO](https://github.com/pgul/binkd/blob/master/todo.lst)
- Binkp 1.0: [English](https://github.com/pgul/binkd/blob/master/doc/binkp10-en.txt), [Russian](https://github.com/pgul/binkd/blob/master/doc/binkp10-ru.txt)
- Binkp 1.1: [English](https://github.com/pgul/binkd/blob/master/doc/binkp11-en.txt), [Russian](https://github.com/pgul/binkd/blob/master/doc/binkp11-ru.txt)
- Binkp license: [English](https://github.com/pgul/binkd/blob/master/doc/binkplic-en.html), [Russian](https://github.com/pgul/binkd/blob/master/doc/binkplic-ru.html)
- [SRIF](https://github.com/pgul/binkd/blob/master/doc/srif.txt)
- [ND mode](https://github.com/pgul/binkd/blob/master/doc/nd-mode-ru.txt)
- [Perl hooks](https://github.com/pgul/binkd/blob/master/doc/perlhooks.txt)
- How-to: [English](https://github.com/pgul/binkd/blob/master/doc/howto-en.texi), [Russian](https://github.com/pgul/binkd/blob/master/doc/howto-ru.texi)
- [Russian manual](https://github.com/pgul/binkd/blob/master/doc/binkd-ug-ru.htm)
- [Log level reference](https://github.com/pgul/binkd/blob/master/doc/binkd-ref-en.txt)

The upstream files that this tree once copied are now linked above. The
Rust tests and the interop jobs use the upstream source as the oracle.

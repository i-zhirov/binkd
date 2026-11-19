# Documentation

rbinkd is a Rust reimplementation of the Binkd Fidonet mailer. This index
collects the project documentation and links to the upstream Binkd
documentation for the original C daemon.

## User documentation

- [README](../README.md): overview, build, quick start, and limitations
- [Compatibility and limitations](compatibility.md): drop-in behavior,
  supported platforms, and known differences
- [Configuration](configuration.md): configuration files, validation, and
  platform feature sets
- [Operations](operations.md): daemon, polls, inetd, Windows service, logs,
  and shutdown
- [Manual page](../rbinkd.8): options, signals, and runtime files

## Configuration samples

- [binkd.conf](../binkd.conf): UNIX sample
- [binkd.cfg](../binkd.cfg): Windows sample

The Rust parser reads these files. The comments in the samples describe the
common directives.

## Developer documentation

- [Development](development.md): build, tests, workspace layout, and
  runtime design
- [Test suite](../tests/README.md): golden corpus, session replay, and C
  interop tests
- [Coding rules](../AGENTS.md): repository and code conventions

## Upstream documentation

The upstream project owns the original Binkd documentation. Use these links
for the C implementation and the legacy platforms:

- [Upstream repository](https://github.com/pgul/binkd)
- [Upstream documentation directory](https://github.com/pgul/binkd/tree/master/doc)
- [Binkd manual](https://github.com/pgul/binkd/blob/master/binkd.txt)
- [Binkd FAQ](https://github.com/pgul/binkd/blob/master/binkdfaq-en.txt)
- [Windows 9x note](https://github.com/pgul/binkd/blob/master/binkd9x-en.txt)
- [Change history](https://github.com/pgul/binkd/blob/master/HISTORY)
- [Release notes](https://github.com/pgul/binkd/blob/master/release-notes.txt)
- [Binkp 1.0](https://github.com/pgul/binkd/blob/master/doc/binkp10-en.txt)
- [Binkp 1.1](https://github.com/pgul/binkd/blob/master/doc/binkp11-en.txt)
- [SRIF](https://github.com/pgul/binkd/blob/master/doc/srif.txt)
- [ND mode](https://github.com/pgul/binkd/blob/master/doc/nd-mode-ru.txt)
- [Perl hooks](https://github.com/pgul/binkd/blob/master/doc/perlhooks.txt)
- [How-to](https://github.com/pgul/binkd/blob/master/doc/howto-en.texi)
- [Russian manual](https://github.com/pgul/binkd/blob/master/doc/binkd-ug-ru.htm)
- [Log level reference](https://github.com/pgul/binkd/blob/master/doc/binkd-ref-en.txt)

The upstream files are not copied here. Use the original project for the C
daemon, its release records, and the legacy platform documentation.

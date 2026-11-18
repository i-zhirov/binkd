rbinkd is a Rust reimplementation of the Binkd Fidonet mailer.

[![CI](https://github.com/i-zhirov/rbinkd/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/i-zhirov/rbinkd/actions/workflows/ci.yml)

It is an FTN-compatible internet daemon and makes possible efficient
utilization of TCP/IP protocol suite as a transport layer in
FTN-based (Fido Technology Network) networks.

## Compiling

The Rust workspace in this repository is the only implementation and the
default build. The C implementation lives upstream in
`https://github.com/pgul/binkd`, the decision of
`doc/decisions/c-out-of-tree.md`.

### Rust (default)

Build the binary:

`cargo build --release -p rbinkd`

The binary is `target/release/rbinkd`. The project keeps the `binkd.conf`
config name, the `BINKD_*` environment variables, the `binkd` protocol
banner, and the `binkd-service` service name for compatibility with the
upstream mailer.

The Windows port supports Windows NT and later, including the service mode,
the decision of `doc/decisions/windows-support.md`. The tray icon is not
supported, and Windows 9x/Me is out of scope. The Windows build accepts the
proxy, socks, and `limit-rate` keywords. It rejects `aso`, `syslog`,
`zlib-dll`, and `bzlib2-dll`; the C Windows build accepts `aso` and the
dynamic loader keywords.

### Upstream C reference

This tree does not hold the C implementation. The interop tests clone the
upstream C reference at a pinned commit and apply `ci/upstream-c.patch`. The
upstream C implementation is also the only build of OS/2, DOS, Amiga, and
Windows 9x/Me, the decision of `doc/decisions/legacy-platforms.md`.

CI builds and tests the upstream C reference on Linux, macOS, and Windows; the
Windows build uses MSYS2/MinGW64 and runs the base C↔Rust matrix. The
`.github/actions/upstream-c` action holds the clone and the patch steps.

The upstream C tree builds the non-UNIX systems:

1. Find in mkfls/ a subdirectory for your system/compiler, copy all files
   to the root of the sources.
2. Run make (nmake, wmake or gmake, name of make's binary is rely with C
   compiler).

UNIXes:

1. Clone the upstream repository:

`$ git clone https://github.com/pgul/binkd`

2. Change into the new binkd source directory:

`$ cd binkd`

3. Check out the pinned commit and apply the patch of this repository:

`$ git checkout 5a84e8596a96ca8b1f4ec69d9a608cd264aa06ba`
`$ git apply /path/to/rbinkd/ci/upstream-c.patch`

4. Copy all files from mkfls/unix/ to the root of binkd sources:

`cp mkfls/unix/* .`

5. Run configure and make:

`$ ./configure`
`$ make`

6. When finished, the following instructions will be displayed offering various
   options for you:

```
 Binkd is successfully compiled.

 If you want to install Binkd files into /usr/local
     1. Run `make -n install' to be sure this makefile will
        do not something criminal during the installation;
     2. `su' to root;
     3. Run `make install' to install Binkd.
     4. Edit /usr/local/etc/binkd.conf-dist and RENAME it or
        MOVE it somewhere (so another `make install' will
        not overwrite it during your next Binkd upgrade)

 If you want to put the files into some other directory just
 run `configure --prefix=/another/path' and go to step 1.
```

## Installation

1. Copy the sample config: `binkd.conf` on UNIX or `binkd.cfg` on Windows.
2. Edit the config.
3. Run the binary with the config as the last argument:
   `target/release/rbinkd binkd.conf`. An upstream C reference build runs
   as `./binkd binkd.conf`.

On Windows, install and control the service with the same binary:

```
rbinkd -t install binkd.conf
rbinkd -t start|stop|restart|status|uninstall
```

`-S name` sets the service name. The service mode holds the `nt/service.c`
behavior of the upstream C code.

## Port status

- The Rust workspace is the only implementation in this tree and the default
  on UNIX and Windows NT and later.
- The Windows service mode is implemented; the tray icon is not supported.
- The Windows build accepts the proxy, socks, and `limit-rate` keywords and
  rejects `aso`, `syslog`, `zlib-dll`, and `bzlib2-dll`.
- OS/2, DOS, Amiga, and Windows 9x/Me are upstream-C-only platforms. This tree
  does not hold the C implementation. The upstream C implementation is the
  reference and the oracle of the interop tests.
- Linux, macOS, and Windows are CI-tested. BSD is build-supported but
  not CI-tested: the Rust workspace and the upstream C reference build
  on BSD, and no CI runner is configured. See
  `doc/decisions/platform-ci.md`.

The records are in `doc/decisions/`: `c-out-of-tree.md`,
`legacy-platforms.md`, `windows-support.md`, `perl-hooks.md`, and
`platform-ci.md`.

## More info

**Echomail areas:**
* RU.BINKD (russian)
* BINKD (international)

**Web site:** http://www.corbina.net/~maloff/binkd/

**FTP:** ftp://happy.kiev.ua/pub/fidosoft/mailer/binkd/

**The mirrors:**
   * ftp://fido.thunderdome.us/pub/mirror/binkd/
   * ftp://cube.sut.ru/pub/mirror/binkd/
   * http://binkd.spb.ru

**Documentation:**

* [English manual for binkd 0.9.2](http://web.archive.org/web/20131010041927/http://www.doe.carleton.ca/~nsoveiko/fido/binkd/man/) © Nick Soveiko (<nsoveiko@doe.carleton.ca>)
* [Russian manual for binkd 0.9.9](http://binkd.grumbler.org/binkd-ug-ru.htm.win.ru) © Stas Degteff (`2:5080/102@fidonet`)
* [FAQ](http://binkd.grumbler.org/binkdfaq.shtml)

**Unit tests:** see [tests/README.md](tests/README.md) for the Rust test
suite and the golden corpus.

**Rust rewrite plan:** see [doc/rust-rewrite-plan.md](doc/rust-rewrite-plan.md).

**Authors:** Dmitry Maloff <maloff@corbina.net> and others.

**Bug reporting:** <binkd-bugs@happy.kiev.ua>, also RU.BINKD or BINKD echoconferences.

**Binkd developers mailing list:** <binkd-dev@happy.kiev.ua> (send `subscribe binkd-dev` to <majordomo@happy.kiev.ua> for subscribe).

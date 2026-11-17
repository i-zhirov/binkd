Binkd is a Fidonet mailer designed to operate via TCP/IP networks.

As a FTN-compatible internet daemon, it makes possible efficient
utilization of TCP/IP protocol suite as a transport layer in
FTN-based (Fido Technology Network) networks.

## Compiling

The Rust workspace in `rust/` is the default build. The C code stays in the
tree as the reference and the build of OS/2, DOS, Amiga, and Windows 9x/Me,
the decision of `doc/decisions/legacy-platforms.md`.

### Rust (default)

Build the binary:

`cd rust && cargo build --release -p binkd`

The binary is `rust/target/release/binkd`.

The Windows port supports Windows NT and later, including the service mode,
the decision of `doc/decisions/windows-support.md`. The tray icon is not
supported, and Windows 9x/Me is out of scope. The Windows build accepts the
proxy, socks, and `limit-rate` keywords. It rejects `aso`, `syslog`,
`zlib-dll`, and `bzlib2-dll`; the C Windows build accepts `aso` and the
dynamic loader keywords.

### C reference

The C code is the reference and the oracle of the tests. It also builds the
legacy platforms.

non-UNIX:

1. Find in mkfls/ a subdirectory for your system/compiler, copy all files
   to the root of the sources.
2. Run make (nmake, wmake or gmake, name of make's binary is rely with C
   compiler).

UNIXes:

1. Clone the repo:

`$ git clone https://github.com/pgul/binkd`

2. Change into the new binkd source directory:

`$ cd binkd`

3. Copy all files from mkfls/unix/ to the root of binkd sources:

`cp mkfls/unix/* .`

4. Run configure and make:

`$ ./configure`
`$ make`

5. When finished, the following instructions will be displayed offering various
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
   `rust/target/release/binkd binkd.conf` for the Rust default, or
   `./binkd binkd.conf` for the C reference build.

On Windows, install and control the service with the same binary:

```
binkd -t install binkd.conf
binkd -t start|stop|restart|status|uninstall
```

`-S name` sets the service name. The service mode holds the `nt/service.c`
behavior of the C code.

## Port status

- The Rust binary is the default on UNIX and Windows NT and later.
- The Windows service mode is implemented; the tray icon is not supported.
- The Windows build accepts the proxy, socks, and `limit-rate` keywords and
  rejects `aso`, `syslog`, `zlib-dll`, and `bzlib2-dll`.
- OS/2, DOS, Amiga, and Windows 9x/Me are C-only platforms. The C code is the
  reference and the oracle of the tests.

The records are in `doc/decisions/`: `legacy-platforms.md` and
`windows-support.md`.

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

**Unit tests:** see [tests/README.md](tests/README.md) for the C and Rust test suites.

**Rust rewrite plan:** see [doc/rust-rewrite-plan.md](doc/rust-rewrite-plan.md).

**Authors:** Dmitry Maloff <maloff@corbina.net> and others.

**Bug reporting:** <binkd-bugs@happy.kiev.ua>, also RU.BINKD or BINKD echoconferences.

**Binkd developers mailing list:** <binkd-dev@happy.kiev.ua> (send `subscribe binkd-dev` to <majordomo@happy.kiev.ua> for subscribe).

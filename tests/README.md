# Unit tests for the C implementation

The tests in this directory check the pure parts of the C code.
They link the real binkd modules against the stubs in `stubs.c`.
The tests pin the behaviour that the Rust port must reproduce.

## Requirements

- A C compiler (`cc`).
- A UNIX system. The tests define `HAVE_FORK` and `UNIX` and use the
  same feature defines as the autoconf build.

## Run the tests

Run all tests:

```
make -C tests
```

The command builds each test program and runs it. A test program prints
`ok` or `FAIL` for its checks and the exit code of the make command
shows the result.

Remove the build directory:

```
make -C tests clean
```

Run the tests with AddressSanitizer:

```
make -C tests clean
make -C tests CFLAGS="-O0 -g -fsanitize=address -fno-omit-frame-pointer"
```

## What the tests cover

| Test program | Modules | Checks |
| --- | --- | --- |
| `test_pmatch` | `pmatch.c` | Literal, `?`, `*`, and `[...]` patterns |
| `test_ftnaddr` | `ftnaddr.c`, `ftndom.c` | Parse, format, compare, paths |
| `test_getw` | `getw.c` | Words, quotes, escapes, `%VAR%`, `#` comments |
| `test_crypt` | `crypt.c` | CRC table, key schedule, ZipCrypto vectors |
| `test_md5` | `md5b.c` | HMAC-MD5 (RFC 2202), CRAM-MD5 parse and format |
| `test_tools` | `tools.c`, `xalloc.c` | Strings, args, paths, packets, time |
| `test_readflo` | `readflo.c` | `.flo` line parse and `ftrans` rewrite |

The tests do not cover the modules that need the network, the process
managers, or the config parser. See `doc/rust-rewrite-plan.md` for the
plan that adds those tests.

## How the tests link

The Makefile compiles each real module from the parent directory into
`build/`. `stubs.c` supplies the parts of binkd that need the full
daemon:

- `inetd_flag` and `mypid` for the logger and the CRAM generator.
- `ext_rand` for the CRAM generator.
- `mask_test()` returns `NULL`, so the logger does not filter messages.

`harness.c` calls `InitLog(0, 0, "", NULL)`, so the tests stay silent.

## Add a test

1. Write `test_<module>.c` with a `main()` that calls `test_init()`,
   the test functions, and `test_finish("<name>")`.
2. Add the name to `TESTS` in `Makefile`.
3. Add the module source file name to `MODULES` in `Makefile` when the
   test needs a module that no other test links.
4. Use `CHECK`, `CHECK_INT`, `CHECK_STR`, `CHECK_NULL`, and
   `CHECK_NOT_NULL` from `harness.h` for the checks.

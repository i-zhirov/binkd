# Tests of the C implementation

The tests in this directory check the C code.
The golden corpus in `golden/` holds the value cases.
The C runner in `test_corpus.c` and the Rust tests in `rust/` read the
same corpus, so the corpus is the equivalence oracle.

## Requirements

- A C compiler (`cc`).
- A UNIX system. The tests define `HAVE_FORK` and `UNIX` and use the
  same feature defines as the autoconf build.

## Run the C tests

```
make -C tests
```

The command builds each test program and runs it. A test program prints
`ok` or `FAIL` for its checks and the exit code of the make command shows
the result.

Remove the build directory:

```
make -C tests clean
```

Run the tests with AddressSanitizer:

```
make -C tests clean
make -C tests CFLAGS="-O0 -g -fsanitize=address -fno-omit-frame-pointer"
```

## Run the Rust tests

```
cd rust
cargo test --workspace
```

The Rust tests read the same corpus. A difference in one case fails the
test with the file name and the line number.

## The golden corpus

A corpus file is a text file with a `# crate:` header and the cases. The
header names the crate of the Rust workspace that owns the file.

A case line has these fields, separated by a tab:

```
<func> <arg>... => <want>
```

A field escapes these characters:

| Escape | Value |
| --- | --- |
| `\\` | Backslash |
| `\t` | Tab |
| `\n` | Newline |
| `\r` | Carriage return |
| `\xHH` | The byte `HH` |

The text `<null>` means a NULL value. Lines that start with `#` are
comments. Empty lines are ignored.

The runners of the C tests and the Rust tests set these environment
variables for the `%VAR%` cases of `getword`:

- `BINKD_TEST_VAR=value`
- `BINKD_TEST_LOWER=lower`

The `ftn.tsv` file uses a fixed domain table and a fixed AKA. The table is
in the header of the file and in `init_tables()` of `test_corpus.c`. The
Rust test uses the same table.

## The spool corpus

The spool cases live in `tests/golden/spool/`. A case sets up a spool, runs
the operations, and compares the output with the lines after the `---` line.
The C runner creates the case directory in a temporary directory, so the case
files hold relative paths only.

The lines before `---` are:

| Line | Meaning |
| --- | --- |
| `setup dir <path>` | Create a directory. |
| `setup file <path> <size> <mtime>` | Create a file of the size and the time. |
| `setup text <path> <content>` | Create a file with the content. |
| `config domain <name> <dir> <z0>` | Add a domain. |
| `config node <addr> <hosts> <obox>` | Add a node. `-` is no box. |
| `config aka <addr>` | Add one of our addresses. |
| `config share <sha> <addr>...` | Add a share chain. |
| `config scalar <name> <value>` | Set a config value. |
| `op scan` | Scan the outbound and the fileboxes. |
| `op sort` | Sort the queue. |
| `op select <addr>` | Select the next file for the address. |
| `op sizes` | Sum the sizes of the files. |
| `op freq` | Count the requests. |
| `op read_try <addr>` | Read the `.try` file. |
| `op write_try <addr> <nok> <nbad> <comment>` | Write the `.try` file. |
| `op bad_try <addr> <error> <where>` | Count a bad attempt. |
| `op good_try <addr> <comment>` | Count a good session. |
| `op remove_try <addr>` | Remove the `.try` file. |
| `op hold <addr> <until>` | Write the `.hld` file. |
| `op process_hld <addr>` | Read the `.hld` file. |
| `op dump` | Print the queue. |
| `op nodes` | Print the nodes. |
| `op files` | Print the files under the case root, sorted. |
| `op hex <path>` | Print the bytes of a file. |
| `op text <path>` | Print the text of a file. |
| `op exists <path>` | Print 0 or 1. |

The lines after `---` are the output of the operations:

| Line | Meaning |
| --- | --- |
| `queue <path> <flvr> <action> <type> <size> <sent>` | A queue entry. |
| `node <addr> <mail> <files> <busy> <hold>` | A node. |
| `file <path> <size>` | A file that remains. |
| `sizes <netsize> <filessize>` | The sum of the sizes. |
| `freq <n>` | The number of requests. |
| `select <path>` | The file that `select` returned. |
| `try <addr> <nok> <nbad>` | The values of the `.try` file. |
| `hex <path> <hex>` | The bytes of a file. |
| `text <path> <text>` | The text of a file. |
| `exists <path> <0\|1>` | The file exists. |

A case must not hold two queue entries with the same sort key. The C code
keeps the `readdir()` order there, and that order is not the same on every
file system. The Rust port keeps the same order as the C code.

## What the corpus covers

| File | Crate | Cases |
| --- | --- | --- |
| `pmatch.tsv` | `binkd-ftn` | `xpmatch` |
| `getword.tsv` | `binkd-ftn` | `getwordx2` |
| `crypt.tsv` | `binkd-ftn` | ZipCrypto keys and buffers |
| `cram.tsv` | `binkd-ftn` | HMAC-MD5 and CRAM-MD5 |
| `ftn.tsv` | `binkd-ftn` | FTN addresses and domains |
| `tools.tsv` | `binkd-ftn` | The pure helpers of `tools.c` |
| `pkt.tsv` | `binkd-ftn` | Packet headers |
| `flo.tsv` | `binkd-spool` | Flow file lines |
| `spool/queue_scan.tsv` | `binkd-spool` | The queue scan and the sort |
| `spool/queue_points.tsv` | `binkd-spool` | Point directories |
| `spool/queue_zone.tsv` | `binkd-spool` | Zone directories |
| `spool/filebox.tsv` | `binkd-spool` | Fileboxes |
| `spool/bsy.tsv` | `binkd-spool` | `.bsy` and `.csy` files |
| `spool/try_hld.tsv` | `binkd-spool` | `.try` and `.hld` files |
| `spool/try_remove.tsv` | `binkd-spool` | `remove-try-files` |

The corpus does not cover the modules that need the network, the process
managers, or the config parser. See `doc/rust-rewrite-plan.md` for the
plan that adds those tests.

## What the C tests cover without the corpus

| Test program | Modules | Checks |
| --- | --- | --- |
| `test_corpus` | All of the above | The corpus cases |
| `test_spool` | `ftnq.c`, `ftnnode.c` | The spool corpus cases |
| `test_crypt` | `crypt.c` | CRC table, round trip, key stream |
| `test_md5` | `md5b.c` | NULL arguments, a new challenge |

The invariant tests hold the cases that a table cannot express.

## How the tests link

The Makefile compiles each real module from the parent directory into
`build/`. `stubs.c` supplies the parts of binkd that need the full
daemon:

- `inetd_flag` and `mypid` for the logger and the CRAM generator.
- `ext_rand` for the CRAM generator.
- `mask_test()` returns `NULL`, so the logger does not filter messages.

`harness.c` calls `InitLog(0, 0, "", NULL)`, so the tests stay silent.

## Add a spool corpus case

1. Write the case in `golden/spool/`.
2. Run `make -C tests` to check the case against the C code.
3. Run `cargo test` in `rust/` to check the case against the Rust code.

## Add a corpus case

1. Add the line to the file of the function in `golden/`.
2. Run `make -C tests` to check the case against the C code.
3. Run `cargo test` in `rust/` to check the case against the Rust code.

A new corpus file needs the `# crate:` header. The C runner rejects a file
without a known crate name, so no case stays unchecked.

## Add a C test

1. Write `test_<module>.c` with a `main()` that calls `test_init()`,
   the test functions, and `test_finish("<name>")`.
2. Add the name to `TESTS` in `Makefile`.
3. Add the module source file name to `MODULES` in `Makefile` when the
   test needs a module that no other test links.
4. Use `CHECK`, `CHECK_INT`, `CHECK_STR`, `CHECK_NULL`, and
   `CHECK_NOT_NULL` from `harness.h` for the checks.

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
| `setup binary <path> <hex>` | Create a file with the bytes. |
| `setup session <state> [<dir>]` | Set the session state and the directory. |
| `setup session aka <addr>` | Add one of the session addresses. |
| `setup session nfa <n>` | Set the number of the non-busy addresses. |
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
| `op busy_add <addr> <flag>` | Create the busy file. |
| `op busy_test <addr> <flag>` | Test the busy file. |
| `op busy_remove <addr> <flag>` | Remove the busy file. |
| `op busy_remove_all` | Remove every busy file. |
| `op busy_touch` | Touch the busy files. |
| `op mtime <path>` | Print `now` or `old` for the modification time. |
| `op set_mtime <path> <time>` | Set the modification time. |
| `op setup_partial <name> <netname> <size> <time>` | Create partials. |
| `op setup_real <path> <size>` | Create the real file of a transfer. |
| `op inb_test <name> <size> <time>` | Test the inbound file. |
| `op inb_fopen <netname> <size> <time>` | Open the partial file. |
| `op inb_reject <netname> <size> <time>` | Remove the partial file. |
| `op inb_done <netname> <size> <time>` | Rename the partial file. |
| `op set_scalar <name> <value>` | Set a config value. |
| `op set_session <state>` | Set the session state. |
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

## The config corpus

The config cases live in `tests/golden/config/`. A case is a config file
`<name>.conf` and the expected `-d` dump `<name>.expect`.

The C binary makes the expectations:

```
cd tests/golden/config
binkd -d <name>.conf > <name>.expect
```

The configs use relative paths, so the C binary runs in the case directory. The
directories of the cases (`in`, `outb`, `box`, `inbox`, `bbox`) are in the
repository, and the `include` fragments live in `parts/`.

The Rust test `crates/binkd-config/tests/dump_corpus.rs` parses each case and
compares the dump with the expectation. When the `BINKD_C` variable names the C
binary, the test also runs it and checks that the expectation is current:

```
cd rust
BINKD_C=/path/to/binkd cargo test -p binkd-config --test dump_corpus
```

## The reload corpus

A reload case is a config file `<name>.conf` and a changed variant
`<name>.changed`. The Rust test copies the corpus into a temporary directory,
loads the case, writes the variant over the case file, and runs the reload. It
compares the dump of the new config with `<name>.changed.expect`. The C binary made that
expectation with `binkd -d <name>.conf` on the changed file.

The test also checks that a check without the `-C` option does nothing, that a
second check sees no change, and that a broken file keeps the old config.

To check the reload of the C daemon by hand:

```
cd tests/golden/config
binkd -c -C <name>.conf &        # client only, reload on change
# change <name>.conf, wait for the rescan delay, then stop the daemon
grep "Reloading configuration" <log>
```

## The interop tests

The Rust test `crates/binkd-proto/tests/interop.rs` runs the C binary and the
Rust session against each other over the loopback address. Set `BINKD_C` to the
path of the C binary:

```sh
cd rust
BINKD_C=/path/to/binkd cargo test -p binkd-proto --test interop
```

Without `BINKD_C` the tests report that they are skipped.

The matrix of `doc/rust-rewrite-plan.md`:

| Client | Server | State |
| --- | --- | --- |
| Rust | C | plain, zlib, and bzip2 |
| C | Rust | plain, zlib, and bzip2 |
| C | C | the C test suite of the C project |
| Rust | Rust | `crates/binkd-proto/tests/session.rs` |

The compression of a file has no corpus case: the bytes of a zlib or bzip2
stream depend on the version of the library. The unit tests of
`crates/binkd-proto/src/compress.rs` cover a round trip of both methods, and
the interop tests cover the C and the Rust streams against each other.

The C client needs the poll file in the outbound directory of the domain. `-P`
alone adds the poll and starts the daemon, which never exits; pass `-P <node>`
and `-p` in one call so the daemon creates the poll, runs the clients, and
exits.

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
| `spool/busy.tsv` | `binkd-spool` | `.bsy` and `.csy` flags |
| `spool/busy_touch.tsv` | `binkd-spool` | `bsy_touch` delay |
| `spool/inbound_test.tsv` | `binkd-spool` | `inb_test` and the rename styles |
| `spool/inbound_done.tsv` | `binkd-spool` | `inb_done` rename |
| `spool/inbound_reject.tsv` | `binkd-spool` | `inb_reject` and `inb_fopen` |
| `spool/inbound_pkthdr.tsv` | `binkd-spool` | `check_pkthdr` |
| `config/minimal.conf` | `binkd-config` | The defaults of the `-d` dump |
| `config/full.conf` | `binkd-config` | The `-d` dump of the values |
| `config/styles.conf` | `binkd-config` | The `-d` dump of the styles |
| `config/reload_basic.conf` | `binkd-config` | The reload of the values |
| `config/reload_node.conf` | `binkd-config` | The reload of the node table |

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

# Decision: the Perl hooks

Status: approved. The team takes option 1: the port links the C `perlhooks.c` with
the C ABI in one crate of the FFI, `binkd-perl`, and the crate holds the
exception of the lint `unsafe_code`. The crate is optional: a build of the port
holds it only with the feature `perl`, so the default build holds no FFI and no
`unsafe`. The exception holds the crate alone, the port of the rule of `AGENTS.md`
that no crate weakens a workspace lint.

## Context

`perlhooks.c` holds the hooks of the process and of the session, and the
`perl-hooks` and `perl-strict` directives name a script of the operator:
`perl_on_start()`, `perl_on_call()`, `perl_on_error()`, `perl_on_handshake()`,
`perl_after_handshake()`, `perl_after_session()`, `perl_before_recv()`,
`perl_after_recv()`, `perl_before_send()`, `perl_after_sent()`, `perl_on_log()`,
`perl_on_send()`, `perl_on_recv()`, `perl_setup_rlimit()`, `perl_need_reload()`,
`perl_config_loaded()`, and `perl_on_exit()`.

The C code links the hooks only with `./configure --with-perl`, whose default is
no. The oracle of the C tests holds no Perl: `make -C tests` builds without it,
no test of `tests/` names a hook, and no case of the corpus names a hook. The port
builds an oracle with the hooks: `./configure --with-perl` answers
`Compilation flags: clang, zlib, bzlib2, perl.`

## Options

1. Link the C `perlhooks.c` into the Rust binary with the C ABI. This option
   keeps the hooks and the hook API, and it keeps one C module in the tree, the
   recommendation of the plan. The Rust workspace forbids `unsafe_code`, so the
   option needs either the relaxation of the lint for one crate of the FFI or a
   change of the lint.
2. Port the hooks to a Rust binding of Perl. This option gives one language, and
   it adds a dependency and the work of the port of the hooks.
3. Drop the hooks behind a build flag. This option breaks the config
   compatibility of the operators who hold a hook.

## Recommendation

Option 1, the link of the C `perlhooks.c` with the C ABI, in one crate of the
FFI, `binkd-perl`. The crate holds `[lints.rust] unsafe_code = "allow"` of its
own and a comment that states the reason, the port of the single exception of the
rule of `AGENTS.md` that no crate weakens a workspace lint: every other crate
keeps `forbid`, and the exception holds one crate of about a hundred lines of the
declarations of the C functions and of the four types of the bridge.

The reason: the option keeps the hooks, the hook API, and the config of the
operators who hold a hook, the goal of the phase. Option 2 adds a dependency of
the port of Perl, and it must follow the API of the C hooks. Option 3 breaks the
config compatibility of the operators, and the C code itself links the hooks only
with `--with-perl`, whose default is no.

The maintainer approved the exception. `AGENTS.md` holds the record of the
approval and the port of the two rules: the crate `binkd-perl` holds
`unsafe_code = "allow"` of its own, and every other crate keeps `forbid`. The
crate is optional, and the default build of the workspace holds it not.

## The first slice

The hooks that the C oracle exercises: no test of the C tree names a hook, so the
first slice must build an oracle of `--with-perl` and a script that a config names.
The hook of the log, `perl_on_log()`, is the first target: the port holds the log
as a value, so a test may hold the message and the level of the hook without a
process of Perl, and the oracle of `--with-perl` holds the same hook over the
script.

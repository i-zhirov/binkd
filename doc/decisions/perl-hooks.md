# Decision: the Perl hooks

Status: open. The plan recommends option 1 for the first release and option 2
when a maintainer asks for it. The record waits for the decision of the team.

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

## The first slice

The hooks that the C oracle exercises: no test of the C tree names a hook, so the
first slice must build an oracle of `--with-perl` and a script that a config names.
The hook of the log, `perl_on_log()`, is the first target: the port holds the log
as a value, so a test may hold the message and the level of the hook without a
process of Perl, and the oracle of `--with-perl` holds the same hook over the
script.

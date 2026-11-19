# Configuration

rbinkd reads the Binkd configuration format. The sample files in the
repository are the local reference:

- `binkd.conf` for UNIX
- `binkd.cfg` for Windows

The upstream Binkd manual documents every original directive. See the
[documentation index](README.md) for the upstream links.

## Basic setup

The configuration file is the last argument:

```sh
rbinkd binkd.conf
```

A minimal configuration names a domain, an address, a spool directory, and
the inbound directory:

```text
domain fidonet /var/spool/ftn/outbound 2
address 2:5047/13@fidonet
sysname "Example system"
sysop "Example operator"
location "Example location"
nodeinfo "115200,TCP,BINKP"
inbound /var/spool/ftn/inbound
log /var/log/rbinkd.log
```

Add the nodes that you call or accept. Use the sample files for the full
set of common directives and comments.

## Validate a configuration

Check the configuration without starting the daemon:

```sh
rbinkd -n binkd.conf
```

Dump the parsed configuration and exit:

```sh
rbinkd -d binkd.conf
```

The `-d` output is a compatibility surface. Keep it stable when you change
the parser.

## Directives

The parser accepts the supported Binkd keyword names. The samples describe
the common directives: domains, addresses, nodes, passwords, spool paths,
logging, compression, proxy settings, and Perl hooks.

`include` loads another configuration file. `passwords` names a separate
password file.

The exact accepted set depends on the platform and the build features.

## Platform feature set

| Feature | UNIX | Windows |
| --- | --- | --- |
| BSO spool | yes | yes |
| GZ and BZ2 compression | yes | yes |
| `syslog` | yes | no |
| `proxy`, `socks` | no | yes |
| `limit-rate` | no | yes |
| `aso` | no | no |
| `zlib-dll`, `bzlib2-dll` | no | no |
| Perl hooks | optional build | optional build |
| Tray icon | no | no |
| Service mode | no | yes |

The parser reports an unknown keyword when a feature is absent. Check a
config with `-n` before you replace a running daemon.

## Perl hooks

The Perl hooks are not in the default build. Build `rbinkd` with the `perl`
feature and the Perl development files to enable the `perl-*` directives:

```sh
cargo build --release -p rbinkd --features perl
```

The hooks are optional because they add a C shim and a dependency on the
Perl interpreter.

## Upstream configuration reference

The original Binkd documentation remains the reference for directive
syntax and for the C implementation. The [documentation index](README.md)
links to the upstream manual, FAQ, and protocol notes.

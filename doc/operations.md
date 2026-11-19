# Operations

This guide describes how to run and control rbinkd. See
[configuration](configuration.md) for the configuration file and
[compatibility](compatibility.md) for the platform differences.

## Run in the foreground

```sh
rbinkd binkd.conf
```

The process runs both the server and the client. Use `-c` for client only
and `-s` for server only.

## Run as a daemon

```sh
rbinkd -D binkd.conf
```

`-D` is not available in client-only mode. The pid file and the log file
come from the configuration.

## Reload the configuration

Send `SIGHUP` to the daemon, or start it with `-C` to reload when the
configuration file changes:

```sh
rbinkd -C binkd.conf
```

A reload keeps the running sessions and applies the new configuration to
the managers.

## Poll a node

Create a poll and exit:

```sh
rbinkd -P 2:5047/13 -p binkd.conf
```

`-P` names the node. `-p` runs the client manager, processes the polls, and
exits when the queue stays empty. Use `-c` with `-p` when you only want the
client role.

## Run from inetd

```sh
rbinkd -i binkd.conf
```

`-i` runs one server session on standard input and output. Use `-a` to
supply the remote address when the transport does not provide it.

## Windows service

Install the service, then start, stop, restart, or query it:

```sh
rbinkd -t install binkd.conf
rbinkd -t start
rbinkd -t stop
rbinkd -t restart
rbinkd -t status
rbinkd -t uninstall
```

`-S name` sets the service name. The service mode uses the Windows service
control manager and the registry records of the upstream Binkd service.

The Rust build has no tray icon and no `-T` switch. A configuration or
script that uses the tray icon from the C build needs a change.

## Logs and session logs

Set the text log path with `log` in the configuration. `binlog` enables the
binary session log. `loglevel` and `conlog` control the amount of output.

The text log line format stays compatible with Binkd. The message text can
differ.

## Stop the daemon

Send `SIGTERM` or `SIGINT` to stop a UNIX daemon. The daemon removes its
busy flags and pid file on shutdown.

## Command-line reference

Use `rbinkd -h` for the short help and the [manual page](../rbinkd.8) for
the full option list.

The legacy `BINKD_LOG` environment variable is not read. Set the log path
in the configuration file.

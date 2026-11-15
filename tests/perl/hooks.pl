# The hooks of the test of the oracle of the port of the Perl hooks.
#
# The port of the hooks of `perlhooks.c`: the C code sets the values of the
# session, it calls the sub of the name, and the return value answers the C code.
# The hooks of the test write their name into the file of the environment
# `BINKD_PERL_MARK`, so the test holds the hook that ran.

sub on_log {
  s/Exit on option/Exit on option (perl)/;
  $lvl = 1;
  return 1;
}

sub on_start {
  mark("start");
  mark("config " . $config{sysname});
  return 1;
}

sub on_exit {
  mark("on_exit");
  return 1;
}

sub on_call {
  mark("call");
  return 1;
}

sub on_error {
  mark("error");
  mark("error where=$where");
  return 1;
}

sub on_handshake {
  mark("on_handshake");
  mark("on_handshake he=" . join(",", @he));
  return "";
}

sub after_handshake {
  mark("after_handshake");
  return "";
}

sub after_session {
  mark("after_session");
  return 1;
}

sub before_recv {
  mark("before_recv");
  return 1;
}

sub after_recv {
  mark("after_recv");
  mark("after_recv tmpfile=$tmpfile file=$file");
  return 1;
}

sub before_send {
  mark("before_send");
  # The C code of `perl_before_send()` of `perlhooks.c` aborts the send of a
  # file when the hook answers a value above 0, the port of the check of
  # `protocol.c`, so the hook answers 0 and the file goes.
  return 0;
}

sub after_sent {
  mark("after_sent");
  # The port of `$action` of `perl_after_sent()` of `perlhooks.c`: a nonzero
  # return reads the value back, and the code removes the file of the spool.
  $action = "d";
  return 1;
}

sub on_send {
  mark("on_send");
  mark("on_send $type|$s1|$s2");
  return 1;
}

sub on_recv {
  mark("on_recv");
  return 1;
}

sub setup_rlimit {
  mark("setup_rlimit");
  return 1;
}

sub need_reload {
  mark("need_reload");
  return 0;
}

sub config_loaded {
  mark("config_loaded");
  return 1;
}

sub mark {
  my ($name) = @_;
  my $path = $ENV{BINKD_PERL_MARK};
  return 0 unless $path;
  open(my $fh, ">>", $path) or return 0;
  print $fh "$name\n";
  close($fh);
  return 0;
}

1;

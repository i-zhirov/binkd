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
  return 1;
}

sub on_call {
  mark("call");
  return 1;
}

sub on_error {
  mark("error");
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

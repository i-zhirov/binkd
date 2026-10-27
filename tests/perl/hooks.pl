# The hooks of the test of the oracle of the port of the Perl hooks.
#
# The port of `on_log()` of `perlhooks.c`: the C code sets `$_` to the message
# and `$lvl` to the level, it calls the sub, and a true return replaces the
# two, the port of `perl_on_log()`.

sub on_log {
  s/Exit on option/Exit on option (perl)/;
  $lvl = 1;
  return 1;
}

1;

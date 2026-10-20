# Unit tests for the binkd C implementation.
#
# The tests build the real binkd modules and link them against the
# stubs in stubs.c.  They pin the behaviour that the Rust port must
# reproduce.
#
# Usage:
#   make -C tests          # build and run all tests
#   make -C tests clean    # remove the build directory

# The test directory, so that the makefile also works with
# `make -f tests/Makefile` from the source root.
TESTS_DIR := $(dir $(lastword $(MAKEFILE_LIST)))
TOP := $(TESTS_DIR)..
BUILD := $(TESTS_DIR)build

CC ?= cc
CFLAGS ?= -O2 -g
# The defines below match the autoconf build on a modern UNIX system.
# SIZEOF_* feeds the UINT2/UINT4 types of md5b.h.
CPPFLAGS += -I$(TOP) -DHAVE_FORK -DUNIX -DOS='"UNIX"' \
            -DSIZEOF_SHORT=2 -DSIZEOF_INT=4 -DSIZEOF_LONG=8 \
            -DHAVE_STDARG_H -DHAVE_SNPRINTF -DHAVE_VSNPRINTF \
            -DHAVE_SYS_TIME_H -DHAVE_LOCALTIME_R -DHAVE_UNAME \
            -DHAVE_GETTIMEOFDAY -DHAVE_WAITPID -DHAVE_STRTOUMAX \
            -DHAVE_FSEEKO -DHAVE_INTTYPES_H -DHAVE_STDINT_H \
            -DHAVE_UNISTD_H -DHAVE_ARPA_INET_H -DHAVE_NETDB_H \
            -DHAVE_NETINET_IN_H -DHAVE_SOCKLEN_T -DHAVE_MSG_NOSIGNAL \
            -DHAVE_GETADDRINFO -DHAVE_GETNAMEINFO
WARNINGS := -Wall -Wno-unused-parameter

# The binkd modules that the tests link.  Add a module here when a
# new test needs it.
MODULES := pmatch ftndom ftnaddr ftnnode getw crypt md5b readflo tools xalloc ftnq

TESTS := test_crypt test_md5 test_corpus test_spool

# The golden corpus.  The Rust port reads the same files.
CORPUS := $(wildcard $(TESTS_DIR)golden/*.tsv)
SPOOL_CORPUS := $(wildcard $(TESTS_DIR)golden/spool/*.tsv)

MODOBJS := $(addprefix $(BUILD)/,$(addsuffix .o,$(MODULES)))
TESTBINS := $(addprefix $(BUILD)/,$(TESTS))

.PHONY: all test clean
all: test

test: $(TESTBINS)
	@fail=0; \
	for t in $(filter-out $(BUILD)/test_corpus $(BUILD)/test_spool,$(TESTBINS)); do \
	  $$t || fail=1; \
	done; \
	$(BUILD)/test_corpus $(CORPUS) || fail=1; \
	$(BUILD)/test_spool $(SPOOL_CORPUS) || fail=1; \
	if [ $$fail -ne 0 ]; then \
	  echo "TESTS FAILED"; \
	  exit 1; \
	fi; \
	echo "All tests passed."

$(BUILD):
	@mkdir -p $(BUILD)

# Test programs and the harness.
$(BUILD)/test_%.o: test_%.c harness.h | $(BUILD)
	$(CC) $(CFLAGS) $(WARNINGS) $(CPPFLAGS) -c -o $@ $<

$(BUILD)/harness.o: harness.c harness.h | $(BUILD)
	$(CC) $(CFLAGS) $(WARNINGS) $(CPPFLAGS) -c -o $@ $<

$(BUILD)/stubs.o: stubs.c | $(BUILD)
	$(CC) $(CFLAGS) $(WARNINGS) $(CPPFLAGS) -c -o $@ $<

# Real binkd modules.
$(BUILD)/%.o: $(TOP)/%.c | $(BUILD)
	$(CC) $(CFLAGS) $(WARNINGS) $(CPPFLAGS) -c -o $@ $<

$(BUILD)/test_%: $(BUILD)/test_%.o $(BUILD)/harness.o $(BUILD)/stubs.o $(MODOBJS)
	$(CC) $(CFLAGS) -o $@ $(BUILD)/test_$*.o $(BUILD)/harness.o $(BUILD)/stubs.o $(MODOBJS)

clean:
	rm -rf $(BUILD)

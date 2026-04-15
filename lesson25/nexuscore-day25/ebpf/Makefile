ARCH   := $(shell uname -m | sed 's/x86_64/x86/' | sed 's/aarch64/arm64/')
CLANG  := clang
CFLAGS := -O2 -g -target bpf -D__TARGET_ARCH_$(ARCH)
INC    := -I/usr/include/$(shell uname -m)-linux-gnu

.PHONY: all clean

all: nexuscore_ts.bpf.o nexuscore_ts.skel.h

nexuscore_ts.bpf.o: src/nexuscore_ts.bpf.c
	$(CLANG) $(CFLAGS) $(INC) -c $< -o $@

nexuscore_ts.skel.h: nexuscore_ts.bpf.o
	bpftool gen skeleton $< > $@

clean:
	rm -f *.bpf.o *.skel.h

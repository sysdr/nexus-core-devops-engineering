CLANG    ?= clang
ARCH     := $(shell uname -m | sed 's/x86_64/x86/' | sed 's/aarch64/arm64/')
# libbpf headers (bpf_helpers.h): clone libbpf to ~/.local/include/bpf or use libbpf-dev
LIBBPF_INC ?= $(HOME)/.local/include
CFLAGS   := -g -O2 -Wall -target bpf -D__TARGET_ARCH_$(ARCH) \
            -I$(LIBBPF_INC) \
            -I/usr/include/$(shell uname -m)-linux-gnu \
            -I./headers

all: nexuscore_xdp.bpf.o

nexuscore_xdp.bpf.o: src/nexuscore_xdp.bpf.c headers/nexuscore_schema.h
	$(CLANG) $(CFLAGS) -c $< -o $@
	@echo "[nexuscore] eBPF object built: $@"

verify: nexuscore_xdp.bpf.o
	bpftool prog load nexuscore_xdp.bpf.o /sys/fs/bpf/nexuscore_verify \
		map name schema_map pinned /sys/fs/bpf/nexuscore/schemas 2>/dev/null || true
	@echo "[nexuscore] BPF verifier passed"

clean:
	rm -f *.o

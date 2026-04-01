# eBPF build — requires clang >= 17 and libbpf-dev
ARCH    := $(shell uname -m | sed 's/x86_64/x86/' | sed 's/aarch64/arm64/')
CLANG   ?= clang
CFLAGS  := -O2 -g -target bpf -D__TARGET_ARCH_$(ARCH) \
           -I/usr/include/$(shell uname -m)-linux-gnu \
           -I/usr/include \
           -Wno-unused-value -Wno-pointer-sign \
           -Wno-compare-distinct-pointer-types \
           -Wno-gnu-variable-sized-type-not-at-end

.PHONY: all clean check

all: tenant_latency.bpf.o

# Try vmlinux.h generation, fall back to headers
tenant_latency.bpf.o: tenant_latency.bpf.c
	@if command -v bpftool >/dev/null 2>&1; then \
		echo "  [bpf] Generating vmlinux.h via bpftool..."; \
		bpftool btf dump file /sys/kernel/btf/vmlinux format c > vmlinux.h 2>/dev/null || true; \
		if [ -s vmlinux.h ]; then \
			$(CLANG) $(CFLAGS) -D__BPF_VMLINUX__ -c $< -o $@; \
		else \
			$(CLANG) $(CFLAGS) -c $< -o $@; \
		fi; \
	else \
		echo "  [bpf] bpftool not found, compiling without vmlinux.h"; \
		$(CLANG) $(CFLAGS) -c $< -o $@; \
	fi
	@echo "  [ok ] eBPF object: $@"

check:
	@if [ -f tenant_latency.bpf.o ]; then \
		echo "  [ok ] eBPF object exists"; \
		if command -v llvm-objdump >/dev/null 2>&1; then \
			llvm-objdump -d tenant_latency.bpf.o 2>/dev/null | head -30; \
		fi; \
	else \
		echo "  [!  ] Run 'make' first"; \
	fi

clean:
	rm -f *.bpf.o vmlinux.h

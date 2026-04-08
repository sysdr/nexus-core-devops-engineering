CLANG   ?= clang-17
LLC     ?= llc
ARCH    := $(shell uname -m | sed 's/x86_64/x86/')
KERN_INC = /usr/include
LIBBPF_INC = /usr/include/bpf
VMLINUX  = vmlinux.h

CFLAGS  := -g -O2 -target bpf -D__TARGET_ARCH_$(ARCH) \
           -I$(KERN_INC) -I$(LIBBPF_INC) -I.

.PHONY: all vmlinux clean

all: vmlinux projection_xdp.bpf.o

vmlinux:
	@if [ ! -f vmlinux.h ]; then \
	  echo "Generating vmlinux.h via bpftool..."; \
	  bpftool btf dump file /sys/kernel/btf/vmlinux format c > vmlinux.h; \
	fi

projection_xdp.bpf.o: projection_xdp.bpf.c projection_xdp.h vmlinux.h
	$(CLANG) $(CFLAGS) -c $< -o $@
	@echo "✓ eBPF object: $@"
	@llvm-objdump -d $@ | head -40

clean:
	rm -f *.o vmlinux.h

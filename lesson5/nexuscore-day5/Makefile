.PHONY: all build-wasi build-rust build-bpf clean

all: build-wasi build-rust

build-wasi:
	cd orchestrator && cargo build --target wasm32-wasip2 --release

build-rust:
	cargo build --release

build-bpf:
	cd bpf && clang-16 -O2 -g -Wall -target bpf \
		-D__TARGET_ARCH_x86 \
		-I/usr/include/x86_64-linux-gnu \
		-c src/nexuscore_probe.bpf.c \
		-o nexuscore_probe.bpf.o
	cd bpf && go generate && go build -o loader .

clean:
	cargo clean
	rm -f bpf/*.o bpf/loader bpf/nexuscore_bpf*

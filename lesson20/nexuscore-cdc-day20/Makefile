.PHONY: all build-ebpf build-component build-loader start stop verify stress test clean

all: build-ebpf build-component build-loader

build-ebpf:
	clang -g -O2 -target bpf -D__TARGET_ARCH_x86 \
	  -I/usr/include/$(shell uname -m)-linux-gnu \
	  -c ebpf/src/cdc_probe.bpf.c -o ebpf/cdc_probe.bpf.o

build-component:
	cd cdc-component && cargo component build --release --target wasm32-wasip2
	cp cdc-component/target/wasm32-wasip2/release/nexuscore_cdc_component.wasm ./cdc_component.wasm

build-loader:
	export GOTOOLCHAIN=local; cd loader && go mod tidy && go build -o ../nexuscore-loader ./cmd/nexuscore-loader/

start:
	bash scripts/start.sh

stop:
	bash scripts/stop.sh

verify:
	bash scripts/verify.sh

stress:
	bash scripts/stress_test.sh $$(RATE) $$(DURATION) $$(TENANTS)

test:
	bash scripts/test.sh

clean:
	rm -f nexuscore-loader cdc_component.wasm ebpf/cdc_probe.bpf.o .loader.pid loader.log
	(cd cdc-component && cargo clean) 2>/dev/null || true
	(cd loader && go clean) 2>/dev/null || true

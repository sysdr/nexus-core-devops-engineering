// NexusCore eBPF Loader — userspace side
// Loads rag_probe.bpf.o, attaches tracepoints, polls histogram.
// Build: clang -O2 -o rag_loader rag_loader.c -lbpf

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <signal.h>
#include <time.h>
#include <bpf/libbpf.h>
#include <bpf/bpf.h>

static volatile int running = 1;
static void sig_handler(int sig) { (void)sig; running = 0; }

static void print_histogram(int hist_fd) {
    printf("\n  ┌─ Read() Latency Histogram (log2 µs buckets) ─────────────────┐\n");
    for (int bucket = 0; bucket < 20; bucket++) {
        __u64 count = 0;
        __u32 key = bucket;
        bpf_map_lookup_elem(hist_fd, &key, &count);
        if (count == 0) continue;

        long lo = 1L << bucket;
        long hi = 1L << (bucket + 1);
        printf("  │ %6ld–%6ld µs │ ", lo, hi);

        // ASCII bar (max 40 chars)
        int bar_len = (int)(count > 40 ? 40 : count);
        for (int i = 0; i < bar_len; i++) printf("█");
        printf(" %llu\n", (unsigned long long)count);
    }
    printf("  └──────────────────────────────────────────────────────────────┘\n");
}

int main(int argc, char **argv) {
    const char *obj_path = argc > 1 ? argv[1] : "rag_probe.bpf.o";

    signal(SIGINT, sig_handler);
    signal(SIGTERM, sig_handler);

    struct bpf_object *obj = bpf_object__open(obj_path);
    if (!obj) { perror("bpf_object__open"); return 1; }

    if (bpf_object__load(obj)) { perror("bpf_object__load"); return 1; }

    // Attach tracepoints
    struct bpf_program *enter_prog = bpf_object__find_program_by_name(obj, "trace_read_enter");
    struct bpf_program *exit_prog  = bpf_object__find_program_by_name(obj, "trace_read_exit");

    if (enter_prog) bpf_program__attach(enter_prog);
    if (exit_prog)  bpf_program__attach(exit_prog);

    int hist_fd = bpf_object__find_map_fd_by_name(obj, "latency_hist");

    printf("[NexusCore eBPF] Probe attached. Polling every 2s. Ctrl-C to exit.\n\n");

    while (running) {
        sleep(2);
        print_histogram(hist_fd);
    }

    printf("\n[NexusCore eBPF] Detaching probe.\n");
    bpf_object__close(obj);
    return 0;
}

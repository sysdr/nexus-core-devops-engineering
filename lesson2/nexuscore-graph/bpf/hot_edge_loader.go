//go:build linux
// +build linux

package main

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/cilium/ebpf"
	"github.com/cilium/ebpf/link"
	"github.com/cilium/ebpf/ringbuf"
	"github.com/cilium/ebpf/rlimit"
)

type HeatEvent struct {
	NodeID    uint32
	PID       uint32
	ReadCount uint64
	TsNs      uint64
}

func main() {
	if err := rlimit.RemoveMemlock(); err != nil {
		log.Fatalf("remove memlock: %v", err)
	}

	spec, err := ebpf.LoadCollectionSpec("hot_edge.bpf.o")
	if err != nil {
		log.Fatalf("load BPF spec: %v", err)
	}

	coll, err := ebpf.NewCollection(spec)
	if err != nil {
		log.Fatalf("create BPF collection: %v", err)
	}
	defer coll.Close()

	kp, err := link.Kprobe("sys_read", coll.Programs["nexuscore_trace_read"], nil)
	if err != nil {
		log.Fatalf("attach kprobe: %v", err)
	}
	defer kp.Close()
	fmt.Println("[nexuscore-ebpf] kprobe attached to sys_read")

	rd, err := ringbuf.NewReader(coll.Maps["heat_events"])
	if err != nil {
		log.Fatalf("ring buffer reader: %v", err)
	}
	defer rd.Close()

	stopc := make(chan os.Signal, 1)
	signal.Notify(stopc, syscall.SIGINT, syscall.SIGTERM)
	go func() { <-stopc; _ = rd.Close() }()

	fmt.Printf("[nexuscore-ebpf] Listening for hot nodes (threshold=%d reads)...\n", 500)

	var event HeatEvent
	for {
		record, err := rd.Read()
		if err != nil {
			return
		}

		if err := binary.Read(bytes.NewReader(record.RawSample), binary.LittleEndian, &event); err != nil {
			continue
		}

		ts := time.Duration(event.TsNs)
		fmt.Printf("node=%d pid=%d reads=%d ts=%s\n", event.NodeID, event.PID, event.ReadCount, ts.String())
	}
}

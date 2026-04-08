// NexusCore Day 10 — Stress Test
// Sends NexusCore query packets (UDP) to the XDP-attached interface.
// Reports throughput, hit rate, and rebuild latency from the metrics endpoint.
package main

import (
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"math/rand"
	"net"
	"net/http"
	"os"
	"os/signal"
	"sync"
	"sync/atomic"
	"syscall"
	"time"
)

var (
	workers     = flag.Int("workers", 32, "number of concurrent senders")
	tenants     = flag.Int("tenants", 10000, "number of distinct tenant IDs")
	target      = flag.String("target", "127.0.0.1:9000", "target address")
	metricsURL  = flag.String("metrics", "http://127.0.0.1:8080/metrics", "loader metrics endpoint")
	durationSec = flag.Int("duration", 30, "test duration in seconds")
)

// nexusMagic is the query packet magic bytes
const nexusMagic = 0x4E435050

type queryPkt struct {
	Magic        uint32
	TenantID     uint32
	ProjectionID uint32
	Flags        uint32
}

func buildPacket(tenantID, projID uint32) []byte {
	pkt := make([]byte, 16)
	binary.BigEndian.PutUint32(pkt[0:], nexusMagic)
	binary.BigEndian.PutUint32(pkt[4:], tenantID)
	binary.BigEndian.PutUint32(pkt[8:], projID)
	binary.BigEndian.PutUint32(pkt[12:], 0)
	return pkt
}

type stats struct {
	sent    atomic.Int64
	recv    atomic.Int64
	errors  atomic.Int64
}

func sender(wg *sync.WaitGroup, s *stats, rng *rand.Rand, done <-chan struct{}) {
	defer wg.Done()
	conn, err := net.Dial("udp", *target)
	if err != nil {
		s.errors.Add(1)
		return
	}
	defer conn.Close()

	buf := make([]byte, 4096)
	conn.(*net.UDPConn).SetReadDeadline(time.Now().Add(10 * time.Millisecond))

	for {
		select {
		case <-done:
			return
		default:
		}
		tenantID := uint32(rng.Intn(*tenants))
		pkt := buildPacket(tenantID, 0)
		conn.SetWriteDeadline(time.Now().Add(5 * time.Millisecond))
		_, err := conn.Write(pkt)
		if err != nil {
			s.errors.Add(1)
			continue
		}
		s.sent.Add(1)

		conn.SetReadDeadline(time.Now().Add(5 * time.Millisecond))
		n, rerr := conn.Read(buf)
		if rerr == nil && n >= 16 {
			s.recv.Add(1)
		}
	}
}

type loaderMetrics struct {
	CacheHits    int64   `json:"cache_hits"`
	CacheMisses  int64   `json:"cache_misses"`
	HitRatePct   int64   `json:"hit_rate_pct"`
	RebuildsOk   int64   `json:"rebuilds_ok"`
	RebuildErrors int64  `json:"rebuild_errors"`
	AvgRebuildNs int64   `json:"avg_rebuild_ns"`
}

func fetchMetrics() (*loaderMetrics, error) {
	resp, err := http.Get(*metricsURL)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	var m loaderMetrics
	if err := json.Unmarshal(body, &m); err != nil {
		return nil, err
	}
	return &m, nil
}

func main() {
	flag.Parse()
	fmt.Printf("\n  NexusCore Stress Test — %d workers, %d tenants, %ds\n",
		*workers, *tenants, *durationSec)
	fmt.Println("  ──────────────────────────────────────────────────────")

	s := &stats{}
	done := make(chan struct{})
	var wg sync.WaitGroup

	for i := 0; i < *workers; i++ {
		wg.Add(1)
		go sender(&wg, s, rand.New(rand.NewSource(int64(i)*1337)), done)
	}

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)

	deadline := time.After(time.Duration(*durationSec) * time.Second)
	ticker := time.NewTicker(time.Second)
	defer ticker.Stop()

	var prevSent int64
	elapsed := 0

	for {
		select {
		case <-sig:
			fmt.Println("\n  Interrupted.")
			close(done)
			wg.Wait()
			return
		case <-deadline:
			close(done)
			wg.Wait()
			printSummary(s, elapsed)
			return
		case <-ticker.C:
			elapsed++
			cur := s.sent.Load()
			qps := cur - prevSent
			prevSent = cur
			recv := s.recv.Load()
			errs := s.errors.Load()

			m, merr := fetchMetrics()
			if merr != nil {
				fmt.Printf("\r  t=%02ds  sent/s=%-8d recv=%-8d err=%-4d  [metrics unavailable]  ",
					elapsed, qps, recv, errs)
			} else {
				fmt.Printf("\r  t=%02ds  sent/s=%-8d recv=%-8d err=%-4d  hit%%=%-3d  rebuilds=%-8d  avg_rebuild=%dns  ",
					elapsed, qps, recv, errs,
					m.HitRatePct, m.RebuildsOk, m.AvgRebuildNs)
			}
		}
	}
}

func printSummary(s *stats, elapsed int) {
	sent := s.sent.Load()
	recv := s.recv.Load()
	errs := s.errors.Load()
	avgQPS := sent / int64(elapsed+1)
	fmt.Printf("\n\n  ── Final Summary ───────────────────────────────────\n")
	fmt.Printf("  Duration:    %ds\n", elapsed)
	fmt.Printf("  Total sent:  %d\n", sent)
	fmt.Printf("  Total recv:  %d\n", recv)
	fmt.Printf("  Errors:      %d\n", errs)
	fmt.Printf("  Avg QPS:     %d\n", avgQPS)
	fmt.Println("  ────────────────────────────────────────────────────\n")
}

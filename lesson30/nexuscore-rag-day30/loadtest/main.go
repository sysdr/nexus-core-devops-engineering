// NexusCore RAG Load Test — Go 1.23
// Swarms the host runtime with concurrent tenant requests.
// Measures: throughput, P50/P99 latency, error rate.

package main

import (
	"flag"
	"fmt"
	"math"
	"math/rand/v2"
	"net/http"
	"sort"
	"sync"
	"sync/atomic"
	"time"
)

var (
	concurrency = flag.Int("concurrency", 100, "Number of concurrent virtual tenants")
	duration    = flag.Duration("duration", 30*time.Second, "Test duration")
	targetURL   = flag.String("url", "", "Target URL (empty = local simulation)")
	rampUp      = flag.Duration("ramp-up", 5*time.Second, "Ramp-up period")
)

type Stats struct {
	latencies []int64 // microseconds
	mu        sync.Mutex
	errors    atomic.Int64
	success   atomic.Int64
}

func (s *Stats) Record(latency time.Duration, err error) {
	if err != nil {
		s.errors.Add(1)
		return
	}
	s.success.Add(1)
	us := latency.Microseconds()
	s.mu.Lock()
	s.latencies = append(s.latencies, us)
	s.mu.Unlock()
}

func (s *Stats) Percentile(p float64) int64 {
	s.mu.Lock()
	defer s.mu.Unlock()
	if len(s.latencies) == 0 {
		return 0
	}
	sorted := make([]int64, len(s.latencies))
	copy(sorted, s.latencies)
	sort.Slice(sorted, func(i, j int) bool { return sorted[i] < sorted[j] })
	idx := int(math.Ceil(p/100.0*float64(len(sorted)))) - 1
	if idx < 0 {
		idx = 0
	}
	return sorted[idx]
}

func (s *Stats) Report(elapsed time.Duration) {
	n := s.success.Load()
	errs := s.errors.Load()
	rps := float64(n) / elapsed.Seconds()

	fmt.Println("\n╔══════════════════════════════════════════════════════╗")
	fmt.Println("║         NexusCore Load Test — Results                ║")
	fmt.Println("╠══════════════════════════════════════════════════════╣")
	fmt.Printf("║  Duration            : %-30s║\n", elapsed.Round(time.Millisecond))
	fmt.Printf("║  Concurrency         : %-30d║\n", *concurrency)
	fmt.Printf("║  Total requests      : %-30d║\n", n+errs)
	fmt.Printf("║  Successful          : %-30d║\n", n)
	fmt.Printf("║  Errors              : %-30d║\n", errs)
	fmt.Printf("║  Throughput          : %-30.1f║\n", rps)
	fmt.Printf("║  P50 latency         : %-28dµs║\n", s.Percentile(50))
	fmt.Printf("║  P95 latency         : %-28dµs║\n", s.Percentile(95))
	fmt.Printf("║  P99 latency         : %-28dµs║\n", s.Percentile(99))
	fmt.Println("╚══════════════════════════════════════════════════════╝")
}

func simulateRAGQuery(tenantID int, stats *Stats) {
	start := time.Now()

	// Simulate realistic RAG pipeline latency distribution:
	// - Embedding: ~50-200µs (Wasm component)
	// - Retrieval: ~200-800µs (cosine search, 1024 chunks)
	// - Synthesis: ~500-2000µs (grounded generation)
	baseLatency := time.Duration(750+rand.IntN(1250)) * time.Microsecond
	// Inject occasional slow tenants (P99 scenario)
	if rand.IntN(100) == 0 {
		baseLatency += time.Duration(rand.IntN(20)) * time.Millisecond
	}

	time.Sleep(baseLatency)

	// Simulate 0.3% error rate
	var err error
	if rand.IntN(333) == 0 {
		err = fmt.Errorf("synthesis timeout")
	}

	stats.Record(time.Since(start), err)
}

func httpRAGQuery(tenantID int, url string, stats *Stats) {
	start := time.Now()
	query := fmt.Sprintf("%s/rag?tenant=%d&q=test+query+%d", url, tenantID, rand.IntN(1000))
	resp, err := http.Get(query)
	if err == nil {
		resp.Body.Close()
		if resp.StatusCode != 200 {
			err = fmt.Errorf("HTTP %d", resp.StatusCode)
		}
	}
	stats.Record(time.Since(start), err)
}

func main() {
	flag.Parse()

	fmt.Println("\n╔══════════════════════════════════════════════════════╗")
	fmt.Println("║   NexusCore RAG — Load Test (Go 1.23 Goroutine Swarm)║")
	fmt.Println("╚══════════════════════════════════════════════════════╝")
	fmt.Printf("  Concurrency: %d | Duration: %s | Ramp-up: %s\n\n",
		*concurrency, *duration, *rampUp)

	stats := &Stats{latencies: make([]int64, 0, *concurrency*100)}
	deadline := time.After(*duration)
	start := time.Now()

	var wg sync.WaitGroup
	ticker := time.NewTicker(time.Duration(rampUp.Nanoseconds() / int64(*concurrency)))
	defer ticker.Stop()

	active := 0
	done := make(chan struct{})

	go func() {
		<-deadline
		close(done)
	}()

	// Ramp up goroutines gradually
	for active < *concurrency {
		select {
		case <-done:
			goto report
		case <-ticker.C:
			active++
			wg.Add(1)
			tenantID := active
			go func(id int) {
				defer wg.Done()
				for {
					select {
					case <-done:
						return
					default:
					}
					if *targetURL != "" {
						httpRAGQuery(id, *targetURL, stats)
					} else {
						simulateRAGQuery(id, stats)
					}
				}
			}(tenantID)
		}
	}

	<-done

report:
	wg.Wait()
	stats.Report(time.Since(start))
}

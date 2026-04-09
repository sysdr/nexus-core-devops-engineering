// NexusCore Day 15 - Ingestion bridge + metrics
//
// In constrained environments (WSL/containers) XDP/eBPF may be unavailable.
// This implementation provides a TCP ingest path on 127.0.0.1:9090 that the
// demo replayer can target, while still exposing the dashboard metrics on :9091.

package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"sync/atomic"
	"syscall"
	"time"
)

var (
	totalReceived  atomic.Uint64
	totalDropped   atomic.Uint64
	totalForwarded atomic.Uint64
)

func forwardToHost(hostSock string, tenantID uint32, text string) error {
	conn, err := net.DialTimeout("unix", hostSock, 2*time.Second)
	if err != nil {
		return fmt.Errorf("dial host: %w", err)
	}
	defer conn.Close()
	_ = conn.SetWriteDeadline(time.Now().Add(2 * time.Second))
	return json.NewEncoder(conn).Encode(map[string]any{
		"op": "ingest", "tenant_id": tenantID, "text": text,
	})
}

func startMetricsServer() {
	go func() {
		http.HandleFunc("/metrics", func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Access-Control-Allow-Origin", "*")
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]uint64{
				"received": totalReceived.Load(),
				"forwarded": totalForwarded.Load(),
				"dropped": totalDropped.Load(),
				"rb_drops": 0,
			})
		})
		log.Fatal(http.ListenAndServe(":9091", nil))
	}()
}

func runTCPIngest(hostSock string, stop <-chan os.Signal) error {
	ln, err := net.Listen("tcp", "127.0.0.1:9090")
	if err != nil {
		return fmt.Errorf("listen tcp :9090: %w", err)
	}
	defer ln.Close()
	log.Printf("TCP ingest listening on 127.0.0.1:9090 (demo source)")

	type msg struct {
		Text string `json:"text"`
	}

	for {
		_ = ln.(*net.TCPListener).SetDeadline(time.Now().Add(200 * time.Millisecond))
		c, err := ln.Accept()
		if err != nil {
			if ne, ok := err.(net.Error); ok && ne.Timeout() {
				select {
				case <-stop:
					return nil
				default:
					continue
				}
			}
			return fmt.Errorf("accept: %w", err)
		}
		go func(conn net.Conn) {
			defer conn.Close()
			br := bufio.NewReader(conn)
			for {
				line, err := br.ReadString('\n')
				if err != nil {
					if err == io.EOF {
						return
					}
					return
				}
				line = strings.TrimSpace(line)
				if line == "" {
					continue
				}
				var m msg
				if jerr := json.Unmarshal([]byte(line), &m); jerr != nil || m.Text == "" {
					totalDropped.Add(1)
					continue
				}
				totalReceived.Add(1)
				if ferr := forwardToHost(hostSock, 0, m.Text); ferr != nil {
					// Host may not be up / model may fail; still count drop to surface it.
					totalDropped.Add(1)
					continue
				}
				totalForwarded.Add(1)
			}
		}(c)
	}
}

func main() {
	hostSock := os.Getenv("NEXUS_HOST_SOCK")
	if hostSock == "" {
		hostSock = "/tmp/nexuscore-host.sock"
	}

	startMetricsServer()

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)

	if err := runTCPIngest(hostSock, sig); err != nil {
		log.Fatalf("tcp ingest: %v", err)
	}
	log.Printf("Shutdown. received=%d forwarded=%d dropped=%d",
		totalReceived.Load(), totalForwarded.Load(), totalDropped.Load())
}

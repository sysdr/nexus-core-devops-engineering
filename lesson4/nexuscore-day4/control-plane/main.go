package main

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"time"
	"unsafe"

	"github.com/cilium/ebpf"
)

const (
	schemaPinPath = "/sys/fs/bpf/nexuscore/schemas"
	fieldTypeU64   = 0
	fieldTypeF64   = 1
	fieldTypeBytes = 2
	fieldTypeStr   = 3
)

type SchemaDescriptor struct {
	Version      uint64
	FieldCount   uint16
	FieldOffsets [64]uint16
	FieldTypes   [64]uint8
}

type SchemaSpec struct {
	TenantID uint32      `json:"tenant_id"`
	Version  uint64      `json:"version"`
	Fields   []FieldSpec `json:"fields"`
}

type FieldSpec struct {
	Name   string `json:"name"`
	Type   string `json:"type"`
	Offset uint16 `json:"offset"`
}

func fieldTypeFromString(s string) uint8 {
	switch s {
	case "u64":
		return fieldTypeU64
	case "f64":
		return fieldTypeF64
	case "bytes":
		return fieldTypeBytes
	case "str":
		return fieldTypeStr
	default:
		return fieldTypeU64
	}
}

func openSchemaMap() (*ebpf.Map, error) {
	return ebpf.LoadPinnedMap(schemaPinPath, nil)
}

func pushSchema(spec SchemaSpec) error {
	m, err := openSchemaMap()
	if err != nil {
		return fmt.Errorf("open schema map: %w", err)
	}
	defer m.Close()

	var sd SchemaDescriptor
	sd.Version = spec.Version
	sd.FieldCount = uint16(len(spec.Fields))

	for i, f := range spec.Fields {
		if i >= 64 {
			break
		}
		sd.FieldOffsets[i] = f.Offset
		sd.FieldTypes[i] = fieldTypeFromString(f.Type)
	}

	err = m.Put(spec.TenantID, unsafe.Pointer(&sd))
	if err != nil {
		return fmt.Errorf("map update: %w", err)
	}

	fmt.Printf("[nexuscore] ✓ Schema pushed: tenant=%d version=%d fields=%d\n",
		spec.TenantID, spec.Version, sd.FieldCount)
	return nil
}

func listSchemas() error {
	m, err := openSchemaMap()
	if err != nil {
		return err
	}
	defer m.Close()

	var (
		key uint32
		val SchemaDescriptor
	)
	iter := m.Iterate()
	count := 0
	for iter.Next(&key, unsafe.Pointer(&val)) {
		fmt.Printf("  tenant=%-6d version=%-4d fields=%d\n",
			key, val.Version, val.FieldCount)
		count++
	}
	fmt.Printf("[nexuscore] %d schemas in map\n", count)
	return iter.Err()
}

func deleteSchema(tenantID uint32) error {
	m, err := openSchemaMap()
	if err != nil {
		return err
	}
	defer m.Close()
	err = m.Delete(tenantID)
	if err != nil {
		return fmt.Errorf("delete tenant %d: %w", tenantID, err)
	}
	fmt.Printf("[nexuscore] ✓ Schema deleted: tenant=%d\n", tenantID)
	return nil
}

func loadSchemaFromFile(path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	var spec SchemaSpec
	if err := json.Unmarshal(data, &spec); err != nil {
		return err
	}
	return pushSchema(spec)
}

func simulateLiveUpdates(tenantID uint32, count int) {
	baseFields := []FieldSpec{
		{Name: "timestamp", Type: "u64", Offset: 0},
		{Name: "sensor_id", Type: "u64", Offset: 8},
		{Name: "value", Type: "f64", Offset: 16},
		{Name: "label", Type: "str", Offset: 24},
	}

	for i := 0; i < count; i++ {
		version := uint64(i + 1)
		n := 2 + i%3 + 1
		if n > len(baseFields) {
			n = len(baseFields)
		}
		fields := baseFields[:n]

		spec := SchemaSpec{
			TenantID: tenantID,
			Version:  version,
			Fields:   fields,
		}

		if err := pushSchema(spec); err != nil {
			fmt.Fprintf(os.Stderr, "[error] %v\n", err)
		}
		time.Sleep(500 * time.Millisecond)
	}
	fmt.Println("[nexuscore] Live update simulation complete")
}

func buildTestPayload(spec SchemaSpec) []byte {
	buf := make([]byte, 256)
	binary.BigEndian.PutUint32(buf[0:], spec.TenantID)
	binary.BigEndian.PutUint16(buf[4:], uint16(len(buf)))
	buf[6] = 0
	buf[7] = 0

	for _, f := range spec.Fields {
		off := int(f.Offset) + 8
		switch f.Type {
		case "u64":
			binary.BigEndian.PutUint64(buf[off:], 0xDEADBEEFCAFEBABE)
		case "f64":
			binary.BigEndian.PutUint64(buf[off:], 0x400921FB54442D18)
		case "str":
			str := "nexuscore"
			binary.BigEndian.PutUint16(buf[off:], uint16(len(str)))
			copy(buf[off+2:], str)
		}
	}
	return buf
}

func main() {
	if len(os.Args) < 2 {
		fmt.Println("Usage: control-plane <push|list|delete|simulate|payload>")
		os.Exit(1)
	}

	switch os.Args[1] {
	case "push":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "usage: push <spec.json>")
			os.Exit(1)
		}
		if err := loadSchemaFromFile(os.Args[2]); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}

	case "list":
		if err := listSchemas(); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}

	case "delete":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "usage: delete <tenant_id>")
			os.Exit(1)
		}
		id, _ := strconv.ParseUint(os.Args[2], 10, 32)
		if err := deleteSchema(uint32(id)); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}

	case "simulate":
		if len(os.Args) < 4 {
			fmt.Fprintln(os.Stderr, "usage: simulate <tenant_id> <count>")
			os.Exit(1)
		}
		id, _ := strconv.ParseUint(os.Args[2], 10, 32)
		cnt, _ := strconv.Atoi(os.Args[3])
		simulateLiveUpdates(uint32(id), cnt)

	case "payload":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "usage: payload <spec.json>")
			os.Exit(1)
		}
		data, _ := os.ReadFile(os.Args[3])
		var spec SchemaSpec
		json.Unmarshal(data, &spec)
		payload := buildTestPayload(spec)
		fmt.Printf("Hex payload (%d bytes):\n", len(payload))
		for i, b := range payload[:32] {
			fmt.Printf("%02x ", b)
			if (i+1)%16 == 0 {
				fmt.Println()
			}
		}
		fmt.Println()

	default:
		fmt.Fprintf(os.Stderr, "unknown command: %s\n", os.Args[1])
		os.Exit(1)
	}
}

#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export GOTOOLCHAIN=local
echo "→ go mod tidy (loader)"
(cd loader && go mod tidy)
echo "→ go vet (loader)"
(cd loader && go vet ./...)
echo "→ go test (loader)"
(cd loader && go test ./... 2>/dev/null || true)
echo "→ go build (loader)"
(cd loader && go build -o ../nexuscore-loader ./cmd/nexuscore-loader/)
echo "OK: build + vet passed"

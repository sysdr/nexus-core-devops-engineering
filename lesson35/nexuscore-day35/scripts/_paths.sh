#!/usr/bin/env bash
# Shared path helpers for demo/loadtest (source from scripts/*.sh).
# shellcheck shell=bash

# Emit absolute path to built component Wasm (cargo-component may use wasip1 or wasip2).
nexuscore_resolve_wasm() {
  local root="$1"
  local name="nexuscore_agent.wasm"
  local triple
  for triple in wasm32-wasip2 wasm32-wasip1; do
    local p="$root/agent-component/target/$triple/release/$name"
    if [[ -f "$p" ]]; then
      echo "$p"
      return 0
    fi
  done
  return 1
}

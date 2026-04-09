#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
echo "[demo] Replaying 100K tweets to localhost:9090 -> XDP..."
python3 - "$ROOT/data/tweets.jsonl" << 'PYEOF'
import socket,json,sys,time
f=open(sys.argv[1])
s=socket.create_connection(("127.0.0.1",9090))
for i,line in enumerate(f):
    t=json.loads(line)
    s.sendall((json.dumps({"text":t["text"]})+"\n").encode())
    if i%5000==0: print(f"  sent {i}...",flush=True); time.sleep(0.01)
s.close(); print("[demo] Done.")
PYEOF

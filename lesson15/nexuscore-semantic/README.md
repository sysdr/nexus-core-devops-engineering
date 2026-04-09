# Lesson 15 — Semantic Indexing (Dashboard + Demo)

This lesson generates a workspace under `lesson15/nexuscore-semantic/` and provides:

- A **metrics-producing ingester** on `:9091`
- A **dashboard** served on `:8080`
- A **demo replay** that drives the metrics off zero

## Run

```bash
cd /home/systemdr/git/nexus-core-devops-engineering/lesson15/nexuscore-semantic

# Stop anything already running
bash ./scripts/cleanup.sh

# Start services (skip heavy Rust host build if not available)
NEXUS_SKIP_HOST_BUILD=1 bash ./scripts/start.sh

# Serve dashboard and open it
bash ./scripts/dashboard.sh   # open http://localhost:8080/

# Run demo traffic
bash ./scripts/demo.sh
```

## Stop + cleanup

```bash
cd /home/systemdr/git/nexus-core-devops-engineering/lesson15
bash ./cleanup.sh
```


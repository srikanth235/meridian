# Meridian

A long-running daemon that orchestrates coding agents (Codex app-server) against
issues in an issue tracker (Linear). It implements the spec in
[`SPEC.md`](./SPEC.md): polls candidate issues, dispatches per-issue worker
sessions inside isolated workspaces, retries on failure with exponential
backoff, and exposes a live status surface over HTTP/WebSocket.

```
┌──────────────┐    poll     ┌──────────────┐
│ Linear / API │ ◀─────────  │ Orchestrator │
└──────────────┘             └──────┬───────┘
                                    │ dispatch (bounded concurrency)
                                    ▼
                          ┌────────────────────┐
                          │ Per-issue worker:  │
                          │  - workspace dir   │
                          │  - hooks           │
                          │  - codex app-server│
                          └─────────┬──────────┘
                                    │ events
                                    ▼
                          ┌────────────────────┐
                          │ axum HTTP / WS API │ ◀── React UI (Vite + Tailwind)
                          └────────────────────┘
```

## Repo layout

```
crates/
  meridian-core         # Domain types (no IO)
  meridian-config       # WORKFLOW.md loader, typed config, live reload, prompt rendering
  meridian-workspace    # Per-issue workspace dir lifecycle + hooks
  meridian-tracker      # Tracker abstraction + Linear GraphQL adapter
  meridian-agent        # Codex app-server JSON-RPC client (handshake, turns, tools)
  meridian-orchestrator # Poll loop, dispatch, retry, reconciliation, snapshot
  meridian-server       # axum HTTP/WS API + static asset serving
  meridian              # Binary
frontend/               # React + Vite + Tailwind status UI
WORKFLOW.md             # Repository-owned workflow contract (example)
SPEC.md                 # Long-form specification
```

## Quick start

### 1. Configure

Copy `WORKFLOW.md` and fill in `tracker.project_slug`, then export your Linear
API key:

```bash
export LINEAR_API_KEY=lin_api_…
```

### 2. Build

Backend:

```bash
cargo build --release
```

Frontend (one-time):

```bash
cd frontend && npm install && npm run build
```

The release binary serves `frontend/dist/` as the UI when present.

### 3. Run

```bash
./target/release/meridian --workflow ./WORKFLOW.md
```

Open http://127.0.0.1:7878 to see the live status surface.

### Dev workflow

Run backend and frontend separately for hot-reload UI work:

```bash
# terminal 1
cargo run -p meridian

# terminal 2 — Vite dev server proxies /api → 127.0.0.1:7878
cd frontend && npm run dev
```

## Configuration

All runtime config lives in `WORKFLOW.md` front matter. The file is watched and
re-applied live (spec §6.2). The full schema is documented in
[`SPEC.md` §5.3 / §6.4](./SPEC.md). Highlights:

| key                            | default                            |
| ------------------------------ | ---------------------------------- |
| `tracker.kind`                 | required (`linear`)                |
| `tracker.api_key`              | `$LINEAR_API_KEY` recommended      |
| `polling.interval_ms`          | `30000`                            |
| `agent.max_concurrent_agents`  | `10`                               |
| `agent.max_turns`              | `20`                               |
| `codex.command`                | `codex app-server`                 |
| `codex.approval_policy`        | `never` (high-trust default)       |
| `workspace.root`               | `<temp>/meridian_workspaces`       |
| `server.port`                  | `7878`                             |

CLI flags override: `--port`, `--host`, `--workflow`, `--static-dir`.

## API

| method | path           | purpose                                     |
| ------ | -------------- | ------------------------------------------- |
| GET    | `/api/health`  | liveness probe                              |
| GET    | `/api/snapshot`| current orchestrator snapshot               |
| GET    | `/api/workflow`| parsed workflow definition                  |
| WS     | `/api/ws`      | streams snapshots on every state change     |

## Tests

```bash
cargo test --workspace
```

## Implementation notes

- **Trust posture.** Defaults match the high-trust example in spec §10.5
  (auto-approve everything, full-access sandbox). Tighten via the `codex.*`
  keys in `WORKFLOW.md` if you need stricter isolation.
- **Worker model.** Local subprocesses only in v1. The orchestrator dispatch
  path is structured to slot in an SSH worker (spec §6.4 cheat sheet) without
  changing the rest of the system.
- **Tracker writes.** Meridian only reads from Linear. Issue mutations (state
  transitions, PR comments) are performed by the agent itself with `curl`
  against the Linear GraphQL API — `LINEAR_API_KEY` is inherited by the Codex
  subprocess, and the `WORKFLOW.md` prompt instructs the agent how to call it
  (spec §11.5).
- **Restart recovery.** No durable orchestrator DB; restart re-discovers state
  from the tracker + filesystem (spec §7.4).

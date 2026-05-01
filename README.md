# Meridian

A long-running daemon that orchestrates coding agents (Codex app-server)
against issues in a GitHub repository. Polls candidate issues, dispatches
per-issue worker sessions inside isolated workspaces, retries on failure with
exponential backoff, and surfaces a live kanban board over HTTP/WebSocket
(or wrapped as a desktop app).

```
┌──────────────┐    poll     ┌──────────────┐
│  GitHub API  │ ◀───────── │ Orchestrator │
│  (gh CLI)    │             └──────┬───────┘
└──────────────┘                    │ dispatch (bounded concurrency)
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
                          │ axum HTTP / WS API │ ◀── React kanban UI
                          │                    │     (browser or Electron app)
                          └────────────────────┘
```

## Repo layout

```
crates/
  meridian-core         # Domain types (no IO)
  meridian-config       # WORKFLOW.md loader, typed config, live reload, prompt rendering
  meridian-workspace    # Per-issue workspace dir lifecycle + hooks
  meridian-tracker      # Tracker abstraction + GitHub adapter (`gh` CLI)
  meridian-agent        # Codex app-server JSON-RPC client (handshake, turns, tools)
  meridian-orchestrator # Poll loop, dispatch, retry, reconciliation, snapshot
  meridian-server       # axum HTTP/WS API + static asset serving
  meridian              # Binary
frontend/               # React + Vite + Tailwind kanban UI
desktop/                # Electron shell (spawns daemon, hosts UI in a window)
WORKFLOW.md             # Repository-owned workflow contract (example)
```

## Prerequisites

- Rust (stable)
- Node 20+ (for the frontend and the Electron shell)
- [`gh` CLI](https://cli.github.com) authenticated (`gh auth login`) with `repo` scope
- `codex` (Codex CLI with `app-server` subcommand) on PATH, signed in to ChatGPT

## Quick start (CLI / browser)

### 1. Configure

Edit `WORKFLOW.md` and set `tracker.repo: "owner/name"` to point at your repo.
Make sure the `status:todo` / `status:in-progress` / `status:in-review` labels
exist (or whatever you list under `tracker.active_states`):

```bash
gh label create "status:todo" --color fbca04
gh label create "status:in-progress" --color 1d76db
gh label create "status:in-review" --color 0e8a16
```

### 2. Build

```bash
cargo build --release
cd frontend && npm install && npm run build
```

The release binary serves `frontend/dist/` as the kanban UI when present.

### 3. Run

```bash
./target/release/meridian --workflow ./WORKFLOW.md
```

Open <http://127.0.0.1:7878>.

## Desktop app (Electron)

The `desktop/` crate wraps the daemon + UI in a native Mac window. It spawns
the `meridian` binary on a random localhost port and points an Electron
BrowserWindow at it.

```bash
cargo build --release
cd frontend && npm install && npm run build
cd ../desktop && npm install
npm start                 # dev — uses ./target/release/meridian + ./WORKFLOW.md
npm run build:mac         # produces a .app under desktop/dist/
```

The packaged `.app` keeps its workflow at
`~/Library/Application Support/Meridian/WORKFLOW.md` (scaffolded on first run
from the bundled template). Override with `MERIDIAN_WORKFLOW=/path/to/file`.

Remote debugging is enabled on `127.0.0.1:9444` so Chrome DevTools (or any
CDP client) can attach to the renderer; override with
`MERIDIAN_REMOTE_DEBUG_PORT`.

## Dev workflow (UI)

For hot-reload UI work, run backend and Vite separately:

```bash
# terminal 1
cargo run -p meridian

# terminal 2 — Vite dev server proxies /api → 127.0.0.1:7878
cd frontend && npm run dev
```

## Configuration

All runtime config lives in `WORKFLOW.md` front matter. The file is watched
and re-applied live. Highlights:

| key                            | default                                       |
| ------------------------------ | --------------------------------------------- |
| `tracker.kind`                 | `github`                                      |
| `tracker.repo`                 | required (`"owner/name"`)                     |
| `tracker.active_states`        | `[status:todo, status:in-progress]`           |
| `tracker.terminal_states`      | `[closed]`                                    |
| `tracker.columns`              | optional explicit kanban ordering             |
| `polling.interval_ms`          | `30000`                                       |
| `agent.max_concurrent_agents`  | `10`                                          |
| `agent.max_turns`              | `20`                                          |
| `codex.command`                | `codex app-server`                            |
| `codex.approval_policy`        | `never` (high-trust default)                  |
| `workspace.root`               | `<temp>/meridian_workspaces`                  |
| `server.port`                  | `7878`                                        |

CLI flags override: `--port`, `--host`, `--workflow`, `--static-dir`.

State semantics: an open GitHub issue's "state" is the first matching
`status:*` label (case-insensitive); a closed issue's state is `closed`. Open
issues with no status label show up in an "unsorted" bucket on the kanban
but are not dispatched.

## API

| method | path           | purpose                                     |
| ------ | -------------- | ------------------------------------------- |
| GET    | `/api/health`  | liveness probe                              |
| GET    | `/api/snapshot`| current orchestrator snapshot (incl. kanban)|
| GET    | `/api/workflow`| parsed workflow definition                  |
| WS     | `/api/ws`      | streams snapshots on every state change     |

## Tests

```bash
cargo test --workspace
```

## Implementation notes

- **Trust posture.** Defaults auto-approve everything and use the
  full-access sandbox. Tighten via the `codex.*` keys in `WORKFLOW.md` if you
  need stricter isolation.
- **Tracker writes.** Meridian only reads from GitHub. Issue mutations (label
  changes, closing, comments) are performed by the agent itself with `gh`
  inside its workspace — `gh` is already authenticated for the user that
  launched the daemon. The `WORKFLOW.md` prompt instructs the agent how to
  call it.
- **Restart recovery.** No durable orchestrator DB; restart re-discovers
  state from GitHub + filesystem.

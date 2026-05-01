import { useEffect, useMemo, useState } from "react";
import { useSnapshot } from "./useSnapshot";
import { useSessionLog } from "./useSessionLog";
import { formatDuration, formatNumber, relTime } from "./format";
import type { Issue, RetryRow, RunningRow, SessionLogEntry, Snapshot } from "./types";

const RUNNING_GROUP = "__running__";

export function App() {
  const { snapshot, conn } = useSnapshot();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // Re-render every second so relative timestamps tick.
  const [, force] = useState(0);
  useEffect(() => {
    const id = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  // Resolve the selected issue from the live snapshot every render so it
  // stays fresh as state ticks.
  const selectedIssue = useMemo<Issue | null>(() => {
    if (!selectedId || !snapshot) return null;
    for (const col of snapshot.kanban?.columns ?? []) {
      const f = col.issues.find((i) => i.id === selectedId);
      if (f) return f;
    }
    for (const i of snapshot.kanban?.unsorted ?? []) if (i.id === selectedId) return i;
    return null;
  }, [selectedId, snapshot]);

  return (
    <div className="h-full flex flex-col bg-bg">
      <TopBar conn={conn} snapshot={snapshot} />
      <div className="flex-1 flex min-h-0">
        <Sidebar
          snapshot={snapshot}
          selectedId={selectedId}
          onSelect={setSelectedId}
        />
        <Main snapshot={snapshot} issue={selectedIssue} />
      </div>
    </div>
  );
}

/* ---------- top bar ---------- */

function TopBar({ conn, snapshot }: { conn: string; snapshot: Snapshot | null }) {
  // Left side intentionally empty: macOS hidden-inset titlebar shows the
  // traffic-light buttons there. Pad to keep controls clear of them.
  return (
    <header className="shrink-0 border-b border-border bg-panel app-drag">
      <div className="pl-24 pr-4 h-11 flex items-center gap-3">
        <div className="flex-1" />
        <PauseToggle snapshot={snapshot} />
        <ConnPill state={conn} />
      </div>
      {snapshot?.paused && (
        <div className="bg-warn/10 border-t border-warn/20 text-warn/90 text-[11px] px-4 py-1 text-center">
          paused — no new agents will dispatch. in-flight workers continue.
        </div>
      )}
    </header>
  );
}

function PauseToggle({ snapshot }: { snapshot: Snapshot | null }) {
  const [busy, setBusy] = useState(false);
  const paused = snapshot?.paused ?? true;
  if (!snapshot) return null;
  return (
    <button
      disabled={busy}
      onClick={async () => {
        setBusy(true);
        try {
          await fetch(`/api/control/${paused ? "resume" : "pause"}`, { method: "POST" });
        } finally {
          setBusy(false);
        }
      }}
      className={`pill text-[10px] ${paused ? "pill-warn" : "pill-ok"} hover:opacity-80`}
    >
      {busy ? "…" : paused ? "▶ resume" : "⏸ pause"}
    </button>
  );
}

function ConnPill({ state }: { state: string }) {
  const cls =
    state === "open" ? "pill-ok" : state === "connecting" ? "pill-warn" : "pill-err";
  return <span className={`pill ${cls} text-[10px]`}>{state}</span>;
}

/* ---------- sidebar ---------- */

function Sidebar({
  snapshot,
  selectedId,
  onSelect,
}: {
  snapshot: Snapshot | null;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
}) {
  const runningById = useMemo(() => {
    const m = new Map<string, RunningRow>();
    for (const r of snapshot?.running ?? []) m.set(r.issue.id, r);
    return m;
  }, [snapshot]);
  const retryById = useMemo(() => {
    const m = new Map<string, RetryRow>();
    for (const r of snapshot?.retrying ?? []) m.set(r.issue_id, r);
    return m;
  }, [snapshot]);

  const groups = useMemo(() => {
    const out: Array<{ key: string; label: string; issues: Issue[] }> = [];
    if (!snapshot?.kanban?.loaded) return out;

    // Pin running at the top.
    const runningIssues = (snapshot.running ?? []).map((r) => r.issue);
    if (runningIssues.length > 0) {
      out.push({ key: RUNNING_GROUP, label: "Running", issues: runningIssues });
    }
    const runningIds = new Set(runningIssues.map((i) => i.id));
    for (const col of snapshot.kanban.columns ?? []) {
      const issues = col.issues.filter((i) => !runningIds.has(i.id));
      if (issues.length > 0) {
        out.push({ key: col.state, label: prettyState(col.state), issues });
      }
    }
    if ((snapshot.kanban.unsorted ?? []).length > 0) {
      out.push({
        key: "__unsorted__",
        label: "Unsorted",
        issues: snapshot.kanban.unsorted.filter((i) => !runningIds.has(i.id)),
      });
    }
    return out;
  }, [snapshot]);

  return (
    <aside className="shrink-0 w-72 border-r border-border bg-panel/40 overflow-y-auto">
      {!snapshot?.kanban?.loaded ? (
        <div className="text-xs text-slate-500 p-4">loading…</div>
      ) : groups.length === 0 ? (
        <div className="text-xs text-slate-500 p-4">no issues</div>
      ) : (
        <nav className="py-2">
          {groups.map((g) => (
            <SidebarGroup
              key={g.key}
              label={g.label}
              issues={g.issues}
              isRunning={g.key === RUNNING_GROUP}
              runningById={runningById}
              retryById={retryById}
              selectedId={selectedId}
              onSelect={onSelect}
            />
          ))}
        </nav>
      )}
    </aside>
  );
}

function SidebarGroup({
  label,
  issues,
  isRunning,
  runningById,
  retryById,
  selectedId,
  onSelect,
}: {
  label: string;
  issues: Issue[];
  isRunning: boolean;
  runningById: Map<string, RunningRow>;
  retryById: Map<string, RetryRow>;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const [open, setOpen] = useState(true);
  return (
    <div className="mb-1">
      <button
        onClick={() => setOpen((v) => !v)}
        className="w-full px-4 py-1 flex items-center gap-2 text-[10px] uppercase tracking-wider text-slate-500 hover:text-slate-300"
      >
        <span className="opacity-60">{open ? "▾" : "▸"}</span>
        <span>{label}</span>
        <span className="text-slate-600 tabular-nums">{issues.length}</span>
      </button>
      {open && (
        <ul className="mb-1">
          {issues.map((issue) => (
            <SidebarItem
              key={issue.id}
              issue={issue}
              live={isRunning ? runningById.get(issue.id) : undefined}
              retry={retryById.get(issue.id)}
              selected={selectedId === issue.id}
              onSelect={() => onSelect(issue.id)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function SidebarItem({
  issue,
  live,
  retry,
  selected,
  onSelect,
}: {
  issue: Issue;
  live?: RunningRow;
  retry?: RetryRow;
  selected: boolean;
  onSelect: () => void;
}) {
  const dotCls = live
    ? "bg-ok animate-pulse"
    : retry
    ? "bg-warn"
    : issue.state === "closed"
    ? "bg-slate-600"
    : "bg-slate-500";
  return (
    <li>
      <button
        onClick={onSelect}
        className={`w-full text-left px-4 py-1.5 flex items-center gap-2 hover:bg-slate-800/40 ${
          selected ? "bg-slate-700/40 border-l-2 border-accent -ml-px" : ""
        }`}
      >
        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${dotCls}`} />
        <span className="text-[11px] text-slate-500 tabular-nums shrink-0">
          {issue.identifier}
        </span>
        <span className="text-[12px] text-slate-200 truncate flex-1">{issue.title}</span>
      </button>
    </li>
  );
}

/* ---------- main pane ---------- */

function Main({
  snapshot,
  issue,
}: {
  snapshot: Snapshot | null;
  issue: Issue | null;
}) {
  if (!snapshot) {
    return <main className="flex-1 flex items-center justify-center text-slate-500 text-sm">connecting…</main>;
  }
  if (!issue) {
    return <Overview snapshot={snapshot} />;
  }
  return <IssueDetail snapshot={snapshot} issue={issue} />;
}

function Overview({ snapshot }: { snapshot: Snapshot }) {
  const totals = snapshot.codex_totals;
  const counts = snapshot.kanban?.columns ?? [];
  const totalIssues =
    counts.reduce((a, c) => a + c.issues.length, 0) +
    (snapshot.kanban?.unsorted?.length ?? 0);
  return (
    <main className="flex-1 overflow-y-auto">
      <div className="max-w-2xl mx-auto px-8 py-12">
        <h1 className="text-xl font-medium text-slate-100">Overview</h1>
        <p className="mt-1 text-sm text-slate-500">
          {snapshot.paused
            ? "Orchestrator is paused. Click an issue in the sidebar to inspect it, or resume to start dispatching."
            : "Orchestrator is running. Pick an issue in the sidebar to see its live session."}
        </p>

        <div className="mt-8 grid grid-cols-3 gap-3">
          <Tile
            label="running"
            value={`${snapshot.running.length} / ${snapshot.max_concurrent_agents}`}
          />
          <Tile label="retrying" value={`${snapshot.retrying.length}`} />
          <Tile label="issues" value={`${totalIssues}`} />
          <Tile label="tokens" value={formatNumber(totals.total_tokens)} />
          <Tile label="runtime" value={formatDuration(totals.seconds_running)} />
          <Tile
            label="poll"
            value={`${Math.round(snapshot.poll_interval_ms / 1000)}s`}
          />
        </div>

        {counts.length > 0 && (
          <div className="mt-8">
            <h2 className="text-[10px] uppercase tracking-wider text-slate-500 mb-2">
              by state
            </h2>
            <ul className="border border-border rounded divide-y divide-border/60 bg-panel/40">
              {counts.map((c) => (
                <li
                  key={c.state}
                  className="px-3 py-2 flex justify-between text-sm"
                >
                  <span className="text-slate-300">{prettyState(c.state)}</span>
                  <span className="text-slate-500 tabular-nums">
                    {c.issues.length}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </main>
  );
}

function Tile({ label, value }: { label: string; value: string }) {
  return (
    <div className="border border-border rounded bg-panel/40 px-3 py-2">
      <div className="text-[10px] uppercase tracking-wider text-slate-500">
        {label}
      </div>
      <div className="text-base text-slate-100 tabular-nums mt-0.5">{value}</div>
    </div>
  );
}

function IssueDetail({ snapshot, issue }: { snapshot: Snapshot; issue: Issue }) {
  const log = useSessionLog(issue.id);
  const live = snapshot.running.find((r) => r.issue.id === issue.id);
  const retry = snapshot.retrying.find((r) => r.issue_id === issue.id);

  return (
    <main className="flex-1 flex flex-col min-h-0">
      <div className="shrink-0 border-b border-border px-6 py-4">
        <div className="flex items-baseline gap-3">
          <a
            href={issue.url ?? "#"}
            target="_blank"
            rel="noreferrer"
            className="text-accent text-xs tabular-nums hover:underline"
          >
            {issue.identifier}
          </a>
          <span className="text-[10px] text-slate-500">
            {issue.state}
            {issue.updated_at && <> · updated {relTime(issue.updated_at)}</>}
          </span>
        </div>
        <h2 className="text-base text-slate-100 mt-1 leading-snug">{issue.title}</h2>
        {issue.labels.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1">
            {issue.labels.map((l) => (
              <span
                key={l}
                className="pill pill-muted text-[9px] normal-case tracking-normal"
              >
                {l}
              </span>
            ))}
          </div>
        )}
      </div>

      {(live || retry) && (
        <div className="shrink-0 border-b border-border/60 bg-bg/40 px-6 py-2 grid grid-cols-4 gap-4 text-[11px] tabular-nums">
          {live ? (
            <>
              <Stat label="turn" value={`${live.turn_count}`} />
              <Stat label="tokens" value={formatNumber(live.tokens_total)} />
              <Stat label="last event" value={live.last_event ?? "—"} truncate />
              <Stat label="started" value={relTime(live.started_at)} />
            </>
          ) : retry ? (
            <>
              <Stat label="retry" value={`#${retry.attempt}`} />
              <Stat label="due" value={relTime(retry.due_at)} />
              <Stat
                label="error"
                value={retry.error ?? "—"}
                truncate
                tone="text-err/80"
              />
            </>
          ) : null}
        </div>
      )}

      {issue.description && (
        <details className="shrink-0 border-b border-border/40 px-6 py-2">
          <summary className="text-[10px] uppercase tracking-wider text-slate-500 cursor-pointer hover:text-slate-300">
            description
          </summary>
          <pre className="mt-2 text-xs text-slate-400 whitespace-pre-wrap leading-relaxed">
            {issue.description}
          </pre>
        </details>
      )}

      <div className="flex-1 overflow-y-auto px-6 py-4">
        <h3 className="text-[10px] uppercase tracking-wider text-slate-500 mb-3">
          session log
        </h3>
        <LogList log={log} />
      </div>
    </main>
  );
}

function Stat({
  label,
  value,
  truncate = false,
  tone,
}: {
  label: string;
  value: string;
  truncate?: boolean;
  tone?: string;
}) {
  return (
    <div className="min-w-0">
      <div className="text-[9px] uppercase tracking-wider text-slate-500">
        {label}
      </div>
      <div
        className={`text-slate-300 ${truncate ? "truncate" : ""} ${tone ?? ""}`}
        title={truncate ? value : undefined}
      >
        {value}
      </div>
    </div>
  );
}

/* ---------- log ---------- */

function LogList({ log }: { log: ReturnType<typeof useSessionLog> }) {
  if (log === undefined) {
    return <div className="text-xs text-slate-500 text-center py-8">loading log…</div>;
  }
  if (log === null) {
    return (
      <div className="text-xs text-slate-500 text-center py-8">
        no session log (issue hasn't run, or log was evicted)
      </div>
    );
  }
  if (log.entries.length === 0) {
    return <div className="text-xs text-slate-500 text-center py-8">no events yet</div>;
  }
  return (
    <ol className="space-y-1.5">
      {log.entries.map((e, i) => (
        <LogRow key={i} entry={e} />
      ))}
    </ol>
  );
}

function LogRow({ entry }: { entry: SessionLogEntry }) {
  const [open, setOpen] = useState(false);
  const tone = toneFor(entry.kind);
  return (
    <li className="text-[11px] leading-snug">
      <div className="flex items-baseline gap-2">
        <span className="text-slate-600 tabular-nums shrink-0 w-12">
          {relTime(entry.at)}
        </span>
        <span className={`shrink-0 ${tone}`}>{entry.kind}</span>
      </div>
      <div className="ml-14 text-slate-300 break-words">{entry.summary}</div>
      {entry.detail !== null && entry.detail !== undefined && (
        <div className="ml-14 mt-1">
          <button
            onClick={() => setOpen((v) => !v)}
            className="text-[10px] text-slate-600 hover:text-slate-400"
          >
            {open ? "hide raw" : "raw"}
          </button>
          {open && (
            <pre className="mt-1 bg-black/40 p-2 rounded overflow-x-auto text-[10px] text-slate-400">
              {JSON.stringify(entry.detail, null, 2)}
            </pre>
          )}
        </div>
      )}
    </li>
  );
}

function toneFor(kind: string): string {
  if (kind.includes("failed") || kind.includes("error") || kind === "malformed")
    return "text-err";
  if (kind === "turn_completed" || kind === "session_started") return "text-ok";
  if (kind === "agent_message_delta") return "text-accent/80";
  if (kind.startsWith("approval")) return "text-warn";
  return "text-slate-500";
}

function prettyState(s: string): string {
  if (s === "closed") return "Closed";
  if (s === "open") return "Open";
  if (s.startsWith("status:")) return s.slice(7).replace(/-/g, " ");
  return s;
}

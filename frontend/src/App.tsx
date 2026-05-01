import { useEffect, useMemo, useState } from "react";
import { useSnapshot } from "./useSnapshot";
import { formatDuration, formatNumber, relTime } from "./format";
import type { Issue, KanbanBoard, RetryRow, RunningRow, Snapshot } from "./types";

export function App() {
  const { snapshot, conn } = useSnapshot();
  // Re-render every second so relative timestamps tick.
  const [, force] = useState(0);
  useEffect(() => {
    const id = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <div className="min-h-full flex flex-col">
      <Header conn={conn} snapshot={snapshot} />
      <Board snapshot={snapshot} />
      <Footer />
    </div>
  );
}

function Header({ conn, snapshot }: { conn: string; snapshot: Snapshot | null }) {
  const totals = snapshot?.codex_totals;
  return (
    <header className="border-b border-border bg-panel">
      <div className="max-w-[1600px] w-full mx-auto px-6 py-4 flex items-center gap-6">
        <div className="flex items-center gap-3">
          <span className="text-accent text-lg">▲</span>
          <h1 className="text-lg font-semibold tracking-wide">meridian</h1>
          {snapshot?.repo && (
            <a
              href={`https://github.com/${snapshot.repo}`}
              target="_blank"
              rel="noreferrer"
              className="text-xs text-slate-500 hover:text-slate-300"
            >
              {snapshot.repo}
            </a>
          )}
        </div>
        <div className="flex-1" />
        <Stat label="running" value={`${snapshot?.running.length ?? 0} / ${snapshot?.max_concurrent_agents ?? "–"}`} />
        <Stat label="retrying" value={`${snapshot?.retrying.length ?? 0}`} />
        <Stat label="tokens" value={totals ? formatNumber(totals.total_tokens) : "–"} />
        <Stat label="runtime" value={totals ? formatDuration(totals.seconds_running) : "–"} />
        <Stat label="poll" value={snapshot ? `${Math.round(snapshot.poll_interval_ms / 1000)}s` : "–"} />
        <ConnPill state={conn} />
      </div>
    </header>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="text-right">
      <div className="text-[10px] uppercase tracking-wider text-slate-500">{label}</div>
      <div className="text-sm tabular-nums">{value}</div>
    </div>
  );
}

function ConnPill({ state }: { state: string }) {
  const cls =
    state === "open" ? "pill-ok" : state === "connecting" ? "pill-warn" : "pill-err";
  return <span className={`pill ${cls}`}>{state}</span>;
}

function Board({ snapshot }: { snapshot: Snapshot | null }) {
  const runningByIssueId = useMemo(() => {
    const m = new Map<string, RunningRow>();
    for (const r of snapshot?.running ?? []) m.set(r.issue.id, r);
    return m;
  }, [snapshot]);
  const retryByIssueId = useMemo(() => {
    const m = new Map<string, RetryRow>();
    for (const r of snapshot?.retrying ?? []) m.set(r.issue_id, r);
    return m;
  }, [snapshot]);

  const board: KanbanBoard | undefined = snapshot?.kanban;

  if (!snapshot) {
    return (
      <main className="flex-1 flex items-center justify-center text-slate-500 text-sm">
        connecting…
      </main>
    );
  }
  if (!board?.loaded) {
    return (
      <main className="flex-1 flex items-center justify-center text-slate-500 text-sm">
        waiting for first tracker fetch…
      </main>
    );
  }
  if (board.columns.length === 0) {
    return (
      <main className="flex-1 flex items-center justify-center text-slate-500 text-sm">
        no columns configured (set tracker.active_states / terminal_states)
      </main>
    );
  }

  return (
    <main className="flex-1 overflow-x-auto">
      <div className="px-6 py-6 flex gap-4 min-w-min">
        {board.columns.map((col) => (
          <Column
            key={col.state}
            state={col.state}
            issues={col.issues}
            running={runningByIssueId}
            retrying={retryByIssueId}
          />
        ))}
        {board.unsorted.length > 0 && (
          <Column
            state="(unsorted)"
            issues={board.unsorted}
            running={runningByIssueId}
            retrying={retryByIssueId}
            muted
          />
        )}
      </div>
    </main>
  );
}

function Column({
  state,
  issues,
  running,
  retrying,
  muted = false,
}: {
  state: string;
  issues: Issue[];
  running: Map<string, RunningRow>;
  retrying: Map<string, RetryRow>;
  muted?: boolean;
}) {
  return (
    <section
      className={`panel flex-shrink-0 w-80 flex flex-col ${muted ? "opacity-60" : ""}`}
    >
      <header className="px-3 py-2 border-b border-border flex items-center justify-between">
        <h2 className="text-xs uppercase tracking-wider text-slate-300 font-medium">
          {prettyState(state)}
        </h2>
        <span className="text-xs text-slate-500 tabular-nums">{issues.length}</span>
      </header>
      <ol className="flex-1 overflow-y-auto p-2 space-y-2 max-h-[calc(100vh-220px)]">
        {issues.length === 0 ? (
          <li className="text-xs text-slate-600 px-2 py-6 text-center">empty</li>
        ) : (
          issues.map((issue) => (
            <Card
              key={issue.id}
              issue={issue}
              running={running.get(issue.id)}
              retry={retrying.get(issue.id)}
            />
          ))
        )}
      </ol>
    </section>
  );
}

function Card({
  issue,
  running,
  retry,
}: {
  issue: Issue;
  running?: RunningRow;
  retry?: RetryRow;
}) {
  const live = !!running;
  const queued = !!retry;
  const ringCls = live
    ? "ring-1 ring-accent/60"
    : queued
    ? "ring-1 ring-warn/40"
    : "";
  return (
    <li className={`bg-bg/60 border border-border rounded p-2.5 hover:border-slate-500 transition-colors ${ringCls}`}>
      <div className="flex items-start justify-between gap-2">
        <a
          href={issue.url ?? "#"}
          target="_blank"
          rel="noreferrer"
          className="text-accent text-xs tabular-nums hover:underline"
        >
          {issue.identifier}
        </a>
        {live && <span className="pill pill-ok text-[9px]">running</span>}
        {!live && queued && <span className="pill pill-warn text-[9px]">retry</span>}
      </div>
      <div className="text-sm text-slate-200 mt-1 leading-snug line-clamp-3">
        {issue.title}
      </div>

      {issue.labels.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1">
          {issue.labels
            .filter((l) => !l.startsWith("status:"))
            .slice(0, 4)
            .map((l) => (
              <span key={l} className="pill pill-muted text-[9px] normal-case tracking-normal">
                {l}
              </span>
            ))}
        </div>
      )}

      {live && <RunningOverlay row={running!} />}
      {!live && queued && <RetryOverlay row={retry!} />}

      {!live && !queued && issue.updated_at && (
        <div className="mt-2 text-[10px] text-slate-600">
          updated {relTime(issue.updated_at)}
        </div>
      )}
    </li>
  );
}

function RunningOverlay({ row }: { row: RunningRow }) {
  return (
    <div className="mt-2 pt-2 border-t border-border/60 text-[10px] text-slate-400 space-y-0.5">
      <div className="flex justify-between tabular-nums">
        <span>turn {row.turn_count}</span>
        <span>{formatNumber(row.tokens_total)} tok</span>
      </div>
      <div className="text-slate-500 truncate">
        {row.last_event ?? "—"} · {relTime(row.last_event_at)}
      </div>
      {row.last_message_tail && (
        <div className="text-slate-500 truncate italic">"{row.last_message_tail}"</div>
      )}
    </div>
  );
}

function RetryOverlay({ row }: { row: RetryRow }) {
  return (
    <div className="mt-2 pt-2 border-t border-border/60 text-[10px] text-slate-500 space-y-0.5">
      <div>attempt {row.attempt}</div>
      {row.error && <div className="truncate text-err/80">{row.error}</div>}
    </div>
  );
}

function prettyState(s: string): string {
  if (s === "closed") return "Closed";
  if (s === "open") return "Open";
  if (s.startsWith("status:")) return s.slice(7).replace(/-/g, " ");
  return s;
}

function Footer() {
  return (
    <footer className="border-t border-border bg-panel/50">
      <div className="max-w-[1600px] w-full mx-auto px-6 py-2 text-xs text-slate-500 flex justify-between">
        <span>meridian · coding-agent orchestrator</span>
        <a className="hover:text-slate-300" href="/api/workflow" target="_blank" rel="noreferrer">
          /api/workflow
        </a>
      </div>
    </footer>
  );
}

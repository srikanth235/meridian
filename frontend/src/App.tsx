import { useEffect, useState } from "react";
import { useSnapshot } from "./useSnapshot";
import { dueIn, formatDuration, formatNumber, relTime } from "./format";
import type { RetryRow, RunningRow } from "./types";

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
      <main className="flex-1 max-w-7xl w-full mx-auto px-6 py-6 grid grid-cols-1 lg:grid-cols-3 gap-6">
        <section className="lg:col-span-2 panel p-4">
          <div className="flex items-baseline justify-between mb-3">
            <h2 className="text-sm uppercase tracking-wider text-slate-400">
              Running ({snapshot?.running.length ?? 0})
            </h2>
            <span className="text-xs text-slate-500">
              cap {snapshot?.max_concurrent_agents ?? "–"}
            </span>
          </div>
          {snapshot && snapshot.running.length > 0 ? (
            <RunningTable rows={snapshot.running} />
          ) : (
            <Empty msg="No active sessions." />
          )}
        </section>
        <section className="panel p-4">
          <h2 className="text-sm uppercase tracking-wider text-slate-400 mb-3">
            Retry queue ({snapshot?.retrying.length ?? 0})
          </h2>
          {snapshot && snapshot.retrying.length > 0 ? (
            <RetryTable rows={snapshot.retrying} />
          ) : (
            <Empty msg="Nothing waiting." />
          )}
          <RateLimits payload={snapshot?.rate_limits} />
        </section>
      </main>
      <Footer />
    </div>
  );
}

function Header({ conn, snapshot }: { conn: string; snapshot: ReturnType<typeof useSnapshot>["snapshot"] }) {
  const totals = snapshot?.codex_totals;
  return (
    <header className="border-b border-border bg-panel">
      <div className="max-w-7xl w-full mx-auto px-6 py-4 flex items-center gap-6">
        <div className="flex items-center gap-3">
          <span className="text-accent text-lg">▲</span>
          <h1 className="text-lg font-semibold tracking-wide">meridian</h1>
        </div>
        <div className="flex-1" />
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
    state === "open"
      ? "pill-ok"
      : state === "connecting"
      ? "pill-warn"
      : "pill-err";
  return <span className={`pill ${cls}`}>{state}</span>;
}

function RunningTable({ rows }: { rows: RunningRow[] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead className="text-xs uppercase tracking-wider text-slate-500">
          <tr className="text-left">
            <th className="py-2 pr-3">issue</th>
            <th className="py-2 pr-3">state</th>
            <th className="py-2 pr-3">turns</th>
            <th className="py-2 pr-3">tokens</th>
            <th className="py-2 pr-3">last event</th>
            <th className="py-2 pr-3">started</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border/60">
          {rows.map((r) => (
            <tr key={r.issue.id} className="hover:bg-slate-800/30">
              <td className="py-2 pr-3">
                <div className="flex flex-col">
                  <a
                    href={r.issue.url ?? "#"}
                    target="_blank"
                    rel="noreferrer"
                    className="text-accent hover:underline"
                  >
                    {r.issue.identifier}
                  </a>
                  <span className="text-slate-300 truncate max-w-md">{r.issue.title}</span>
                </div>
              </td>
              <td className="py-2 pr-3">
                <span className="pill pill-muted">{r.issue.state}</span>
              </td>
              <td className="py-2 pr-3 tabular-nums">{r.turn_count}</td>
              <td className="py-2 pr-3 tabular-nums">
                <div>{formatNumber(r.tokens_total)}</div>
                <div className="text-[10px] text-slate-500">
                  {formatNumber(r.tokens_input)} ⇢ {formatNumber(r.tokens_output)}
                </div>
              </td>
              <td className="py-2 pr-3">
                <div className="text-slate-300">{r.last_event ?? "—"}</div>
                <div className="text-[10px] text-slate-500">{relTime(r.last_event_at)}</div>
              </td>
              <td className="py-2 pr-3 text-slate-400">{relTime(r.started_at)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {rows.some((r) => r.last_message_tail) && (
        <div className="mt-3 space-y-2">
          {rows
            .filter((r) => r.last_message_tail)
            .map((r) => (
              <details key={r.issue.id + ":msg"} className="text-xs">
                <summary className="cursor-pointer text-slate-500">
                  <span className="text-accent">{r.issue.identifier}</span> · last reply
                </summary>
                <pre className="mt-1 p-2 bg-black/40 rounded whitespace-pre-wrap text-slate-300">
                  {r.last_message_tail}
                </pre>
              </details>
            ))}
        </div>
      )}
    </div>
  );
}

function RetryTable({ rows }: { rows: RetryRow[] }) {
  return (
    <ul className="text-sm divide-y divide-border/60">
      {rows.map((r) => (
        <li key={r.issue_id} className="py-2 flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="text-accent">{r.identifier}</div>
            <div className="text-xs text-slate-500 truncate">
              attempt {r.attempt}{r.error ? ` · ${r.error}` : ""}
            </div>
          </div>
          <span className="pill pill-warn shrink-0">in {dueIn(r.due_at)}</span>
        </li>
      ))}
    </ul>
  );
}

function RateLimits({ payload }: { payload: unknown | null | undefined }) {
  if (!payload) return null;
  return (
    <details className="mt-4 text-xs">
      <summary className="cursor-pointer text-slate-400">rate limits (raw)</summary>
      <pre className="mt-2 p-2 bg-black/40 rounded overflow-x-auto">
        {JSON.stringify(payload, null, 2)}
      </pre>
    </details>
  );
}

function Empty({ msg }: { msg: string }) {
  return <div className="text-slate-500 text-sm py-8 text-center">{msg}</div>;
}

function Footer() {
  return (
    <footer className="border-t border-border bg-panel/50">
      <div className="max-w-7xl w-full mx-auto px-6 py-3 text-xs text-slate-500 flex justify-between">
        <span>meridian · coding-agent orchestrator</span>
        <a className="hover:text-slate-300" href="/api/workflow" target="_blank" rel="noreferrer">
          /api/workflow
        </a>
      </div>
    </footer>
  );
}

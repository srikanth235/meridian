import { useState } from "react";
import type { Issue, RetryRow, RunningRow, SessionLogEntry, Snapshot } from "../types";
import { useSessionLog } from "../useSessionLog";
import { formatNumber, relTime } from "../format";
import { Card, CardHead, Pill, Progress, StatusDot, fmtElapsed, STATUS_META } from "../atoms";
import { IconBranch, IconChevronLeft } from "../icons";
import { runningAgents, type SymAgent } from "../symphonyMap";

export function IssueDetail({
  snapshot,
  issue,
  onBack,
}: {
  snapshot: Snapshot;
  issue: Issue;
  onBack: () => void;
}) {
  const log = useSessionLog(issue.id);
  const live = snapshot.running.find((r) => r.issue.id === issue.id);
  const retry = snapshot.retrying.find((r) => r.issue_id === issue.id);
  const sym = deriveSym(issue, snapshot, live, retry);

  return (
    <div style={{ padding: 20, display: "flex", flexDirection: "column", gap: 20 }}>
      <button
        onClick={onBack}
        className="flex items-center gap-1.5 text-[12px] text-textDim hover:text-text bg-transparent border-0 cursor-pointer p-0 self-start"
      >
        <IconChevronLeft size={14} />
        Back to agents
      </button>

      <div>
        <div className="flex items-center gap-2 mb-1.5">
          <StatusDot status={sym.status} />
          <span className="font-mono text-[11.5px] text-textDim font-medium">{sym.id}</span>
          <Pill tone={pillTone(sym.status)}>{STATUS_META[sym.status].label}</Pill>
          {issue.url && (
            <a
              href={issue.url}
              target="_blank"
              rel="noreferrer"
              className="font-mono text-[11px] text-textMute hover:text-textDim ml-auto"
            >
              github ↗
            </a>
          )}
        </div>
        <h1 className="text-[20px] font-semibold text-text leading-snug m-0" style={{ letterSpacing: -0.2 }}>
          {issue.title}
        </h1>
        <div className="flex items-center gap-2 mt-2 text-[11.5px] text-textMute">
          {sym.branch && (
            <>
              <IconBranch size={11} />
              <span className="font-mono">{sym.branch}</span>
              <span>·</span>
            </>
          )}
          {sym.workflow && <span>{sym.workflow}</span>}
          {sym.worker && (
            <>
              <span>·</span>
              <span className="font-mono">{sym.worker}</span>
            </>
          )}
        </div>

        {issue.labels.length > 0 && (
          <div className="flex flex-wrap gap-1.5 mt-3">
            {issue.labels.map((l) => (
              <Pill key={l} tone="muted">{l}</Pill>
            ))}
          </div>
        )}
      </div>

      {(live || retry) && (
        <div
          className="grid gap-1"
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
            background: "var(--border)",
            border: "1px solid var(--border)",
            borderRadius: 8,
            overflow: "hidden",
          }}
        >
          {live ? (
            <>
              <Stat label="Step" value={`${live.last_event ?? "—"}`} />
              <Stat label="Turn" value={`${live.turn_count}`} />
              <Stat label="Tokens" value={formatNumber(live.tokens_total)} />
              <Stat label="Started" value={relTime(live.started_at)} />
            </>
          ) : retry ? (
            <>
              <Stat label="Retry" value={`#${retry.attempt}`} />
              <Stat label="Due" value={relTime(retry.due_at)} />
              <Stat label="Error" value={retry.error ?? "—"} />
              <Stat label="Elapsed" value={fmtElapsed(sym.elapsedSec)} />
            </>
          ) : null}
        </div>
      )}

      {sym.status === "running" && live && (
        <Card>
          <CardHead title="Progress" />
          <div className="px-4 py-4 flex flex-col gap-2">
            <div className="flex justify-between text-[11px] font-mono text-textMute">
              <span>{live.last_event ?? "—"}</span>
              <span>{Math.round(sym.progress * 100)}%</span>
            </div>
            <Progress value={sym.progress} />
          </div>
        </Card>
      )}

      {issue.description && (
        <Card>
          <CardHead title="Description" />
          <pre className="p-4 m-0 text-[12px] text-textDim whitespace-pre-wrap leading-relaxed font-sans">
            {issue.description}
          </pre>
        </Card>
      )}

      <Card>
        <CardHead
          title="Session log"
          right={
            <span className="text-[11px] text-textMute font-mono">
              {log?.entries?.length ?? 0} events
            </span>
          }
        />
        <div className="p-4">
          <LogList log={log} />
        </div>
      </Card>
    </div>
  );
}

function deriveSym(
  issue: Issue,
  snapshot: Snapshot,
  live: RunningRow | undefined,
  retry: RetryRow | undefined
): SymAgent {
  if (live) {
    return runningAgents(snapshot).find((a) => a.internalId === issue.id)!;
  }
  return {
    id: issue.identifier || issue.id,
    internalId: issue.id,
    title: issue.title,
    branch: issue.branch_name ?? null,
    workflow: null,
    worker: null,
    status: retry ? "failed" : issue.state === "closed" ? "merged" : "queued",
    step: issue.state,
    progress: 0,
    elapsedSec: 0,
    url: issue.url ?? null,
    labels: issue.labels,
  };
}

function pillTone(status: SymAgent["status"]): "ok" | "warn" | "err" | "blue" | "purple" | "muted" {
  switch (status) {
    case "running": return "ok";
    case "review":  return "blue";
    case "failed":  return "err";
    case "merged":  return "purple";
    default:        return "muted";
  }
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-panel p-3.5 min-w-0">
      <div className="text-[10px] uppercase tracking-[0.5px] text-textMute font-medium mb-1">{label}</div>
      <div className="text-[13px] text-text truncate">{value}</div>
    </div>
  );
}

function LogList({ log }: { log: ReturnType<typeof useSessionLog> }) {
  if (log === undefined) {
    return <div className="text-[12px] text-textMute text-center py-8">loading log…</div>;
  }
  if (log === null) {
    return (
      <div className="text-[12px] text-textMute text-center py-8">
        no session log (issue hasn't run, or log was evicted)
      </div>
    );
  }
  if (log.entries.length === 0) {
    return <div className="text-[12px] text-textMute text-center py-8">no events yet</div>;
  }
  return (
    <ol className="space-y-2">
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
    <li className="text-[11.5px] leading-snug">
      <div className="flex items-baseline gap-2">
        <span className="font-mono text-textMute tabular-nums shrink-0" style={{ width: 56 }}>
          {relTime(entry.at)}
        </span>
        <span className={`shrink-0 font-mono ${tone}`}>{entry.kind}</span>
      </div>
      <div className="ml-16 text-textDim break-words mt-0.5">{entry.summary}</div>
      {entry.detail !== null && entry.detail !== undefined && (
        <div className="ml-16 mt-1">
          <button
            onClick={() => setOpen((v) => !v)}
            className="text-[10px] text-textMute hover:text-textDim bg-transparent border-0 cursor-pointer p-0"
          >
            {open ? "hide raw" : "raw"}
          </button>
          {open && (
            <pre className="mt-1 bg-bg p-2 rounded overflow-x-auto text-[10px] text-textDim border border-borderS">
              {JSON.stringify(entry.detail, null, 2)}
            </pre>
          )}
        </div>
      )}
    </li>
  );
}

function toneFor(kind: string): string {
  if (kind.includes("failed") || kind.includes("error") || kind === "malformed") return "text-red";
  if (kind === "turn_completed" || kind === "session_started") return "text-accent";
  if (kind === "agent_message_delta") return "text-blue";
  if (kind.startsWith("approval")) return "text-amber";
  return "text-textMute";
}

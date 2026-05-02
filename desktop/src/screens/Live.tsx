import { useMemo } from "react";
import type { Snapshot } from "../types";
import {
  Card,
  HarnessAvatar,
  PageSearch,
  Pill,
  Progress,
  RepoChip,
  StatusDot,
  fmtElapsed,
  fmtNum,
  harnessById,
} from "../atoms";
import { IconBranch, IconRetry } from "../icons";
import { issueMatches } from "../search";

export function Live({
  snapshot,
  density,
  onOpenIssue,
  repoFilter,
  query,
  onQueryChange,
}: {
  snapshot: Snapshot;
  density: "comfortable" | "dense";
  onOpenIssue: (id: string) => void;
  repoFilter: string | null;
  query: string;
  onQueryChange: (q: string) => void;
}) {
  const pad = density === "dense" ? 14 : 20;
  const harnesses = snapshot.harnesses ?? [];

  const running = useMemo(() => {
    let r = snapshot.running;
    if (repoFilter) r = r.filter((row) => row.issue.repo === repoFilter);
    if (query) r = r.filter((row) => issueMatches(row.issue, query));
    return r;
  }, [snapshot.running, repoFilter, query]);

  const retrying = useMemo(() => {
    return snapshot.retrying;
  }, [snapshot.retrying]);

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <div className="flex items-end justify-between gap-3">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">Workspace</div>
          <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
            Live
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {running.length} running · {retrying.length} retrying
            {repoFilter && <> · scoped to <span className="font-mono">{repoFilter}</span></>}
          </div>
        </div>
        <PageSearch
          pageId="live"
          value={query}
          onChange={onQueryChange}
          placeholder="Search live…"
        />
      </div>

      <Card>
        {running.length === 0 ? (
          <div className="px-4 py-12 text-center text-textMute text-[12px]">
            No active workers. Assign a task in <span className="text-textDim">Inbox</span> or <span className="text-textDim">Tasks</span> to dispatch one.
          </div>
        ) : (
          running.map((r, idx) => {
            const h = harnessById(harnesses, r.harness ?? r.issue.assignee);
            const elapsed = Math.max(0, Math.floor((Date.now() - Date.parse(r.started_at)) / 1000));
            const progress = Math.min(0.95, 0.1 + r.turn_count * 0.08);
            return (
              <div
                key={r.issue.id}
                onClick={() => onOpenIssue(r.issue.id)}
                className="sym-row cursor-pointer"
                style={{
                  padding: "14px 16px",
                  borderTop: idx === 0 ? "none" : "1px solid var(--borderS)",
                  display: "grid",
                  gridTemplateColumns: "auto 110px 24px minmax(0, 1fr) 180px 90px",
                  alignItems: "center",
                  columnGap: 14,
                }}
              >
                <StatusDot status="running" />
                <span className="font-mono text-[11.5px] text-textDim font-medium">{r.issue.identifier}</span>
                <HarnessAvatar harness={h} size={20} />
                <div className="min-w-0">
                  <div className="text-[13px] text-text font-medium truncate">{r.issue.title}</div>
                  <div className="flex items-center gap-1.5 mt-1 text-[11px] text-textMute truncate">
                    <RepoChip repo={r.issue.repo} />
                    {r.issue.branch_name && (
                      <>
                        <IconBranch size={11} />
                        <span className="font-mono truncate">{r.issue.branch_name}</span>
                      </>
                    )}
                  </div>
                </div>
                <div className="min-w-0">
                  <div className="flex justify-between font-mono text-[10.5px] text-textMute mb-1 gap-1.5">
                    <span className="truncate">{r.last_event ?? "running"}</span>
                    <span className="shrink-0">turn {r.turn_count}</span>
                  </div>
                  <Progress value={progress} />
                  <div className="text-[10.5px] text-textMute font-mono mt-1 truncate">
                    {r.last_message_tail ?? "—"}
                  </div>
                </div>
                <div className="text-right">
                  <div className="font-mono text-[11.5px] text-textDim tabular-nums">{fmtElapsed(elapsed)}</div>
                  <div className="font-mono text-[10.5px] text-textMute tabular-nums">{fmtNum(r.tokens_total)} tok</div>
                </div>
              </div>
            );
          })
        )}
      </Card>

      {retrying.length > 0 && (
        <Card>
          <div className="px-3.5 py-2.5 border-b border-borderS flex items-center gap-2">
            <span className="text-[12px] font-semibold text-text">Retrying</span>
            <span className="font-mono text-[11px] text-textMute">{retrying.length}</span>
          </div>
          {retrying.map((r, idx) => (
            <div
              key={r.issue_id}
              onClick={() => onOpenIssue(r.issue_id)}
              className="sym-row cursor-pointer flex items-center gap-3"
              style={{
                padding: "10px 16px",
                borderTop: idx === 0 ? "none" : "1px solid var(--borderS)",
              }}
            >
              <span className="text-amber"><IconRetry size={13} /></span>
              <span className="font-mono text-[11.5px] text-textDim font-medium" style={{ width: 90 }}>
                {r.identifier}
              </span>
              <Pill tone="warn">attempt {r.attempt}</Pill>
              <span className="text-[12px] text-textDim flex-1 truncate">{r.error ?? "—"}</span>
              <span className="font-mono text-[10.5px] text-textMute tabular-nums shrink-0">
                due {shortTime(r.due_at)}
              </span>
            </div>
          ))}
        </Card>
      )}
    </div>
  );
}

function shortTime(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "—";
  return new Date(t).toTimeString().slice(0, 8);
}

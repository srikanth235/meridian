import { useMemo, useState } from "react";
import type { Snapshot } from "../types";
import { Card, Pill, Progress, StatusDot, fmtElapsed, STATUS_META, TabPill } from "../atoms";
import { IconBranch } from "../icons";
import { allAgents, type SymAgent } from "../symphonyMap";

type Filter = "all" | "running" | "queued" | "review" | "failed" | "merged";

export function AgentsList({
  snapshot,
  density,
  onOpenIssue,
}: {
  snapshot: Snapshot;
  density: "comfortable" | "dense";
  onOpenIssue: (id: string) => void;
}) {
  const [filter, setFilter] = useState<Filter>("all");
  const agents = useMemo(() => allAgents(snapshot), [snapshot]);
  const filtered = filter === "all" ? agents : agents.filter((a) => a.status === filter);
  const pad = density === "dense" ? 14 : 20;

  const counts = {
    all: agents.length,
    running: agents.filter((a) => a.status === "running").length,
    queued: agents.filter((a) => a.status === "queued").length,
    review: agents.filter((a) => a.status === "review").length,
    failed: agents.filter((a) => a.status === "failed").length,
    merged: agents.filter((a) => a.status === "merged").length,
  };

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <div className="flex items-end justify-between gap-3">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">
            Agents
          </div>
          <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
            All agent runs
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {counts.running} running · {counts.queued} queued · {counts.failed} failed · {counts.merged} merged
          </div>
        </div>
        <div className="flex gap-1">
          {(["all", "running", "queued", "review", "failed", "merged"] as Filter[]).map((f) => (
            <TabPill key={f} active={filter === f} onClick={() => setFilter(f)}>
              {cap(f)} <span className="font-mono text-textMute ml-1">{counts[f]}</span>
            </TabPill>
          ))}
        </div>
      </div>

      <Card>
        {filtered.length === 0 ? (
          <div className="px-4 py-16 text-center text-textMute text-[12px]">No agents in this filter.</div>
        ) : (
          filtered.map((a, idx) => (
            <AgentRow key={a.internalId} a={a} first={idx === 0} onClick={() => onOpenIssue(a.internalId)} />
          ))
        )}
      </Card>
    </div>
  );
}

function cap(s: string) {
  return s[0].toUpperCase() + s.slice(1);
}

function AgentRow({ a, first, onClick }: { a: SymAgent; first: boolean; onClick: () => void }) {
  return (
    <div
      onClick={onClick}
      className="sym-row sym-live-row cursor-pointer"
      style={{
        display: "grid",
        alignItems: "center",
        padding: "12px 16px",
        borderTop: first ? "none" : "1px solid #1a1a1a",
      }}
    >
      <span style={{ display: "flex", alignItems: "center" }}>
        <StatusDot status={a.status} />
      </span>
      <span className="sym-live-id font-mono text-[11.5px] text-textDim font-medium">{a.id}</span>
      <div className="sym-live-title min-w-0 overflow-hidden">
        <div className="text-[13px] text-text font-medium truncate">{a.title}</div>
        <div className="flex items-center gap-1.5 mt-1 text-[11px] text-textMute truncate">
          <IconBranch size={11} />
          <span className="font-mono truncate">{a.branch || "—"}</span>
          {a.workflow && (
            <>
              <span>·</span>
              <span className="shrink-0">{a.workflow}</span>
            </>
          )}
        </div>
      </div>
      <div className="sym-live-prog min-w-0">
        {a.status === "running" ? (
          <div>
            <div className="flex justify-between font-mono text-[10.5px] text-textMute mb-1 gap-1.5">
              <span className="truncate">{a.step}</span>
              <span className="shrink-0">{Math.round(a.progress * 100)}%</span>
            </div>
            <Progress value={a.progress} />
          </div>
        ) : (
          <Pill tone={pillTone(a.status)}>{STATUS_META[a.status].label}</Pill>
        )}
      </div>
      <span className="sym-live-elapsed font-mono text-[11.5px] text-textDim tabular-nums">{fmtElapsed(a.elapsedSec)}</span>
      <span className="sym-live-worker font-mono text-[11px] text-textMute text-right">{a.worker || "—"}</span>
    </div>
  );
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

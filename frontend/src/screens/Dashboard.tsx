import type { ReactNode } from "react";
import type { Snapshot } from "../types";
import {
  Card,
  CardHead,
  PrimaryBtn,
  SecondaryBtn,
  TabPill,
  StatusDot,
  Pill,
  Sparkline,
  Progress,
  fmtElapsed,
  fmtNum,
  STATUS_META,
} from "../atoms";
import {
  IconBranch,
  IconPlus,
  IconRetry,
  IconWorkflow,
} from "../icons";
import { dashboardStats, runningAgents, queuedAgents, type SymAgent } from "../symphonyMap";
import { DUMMY_WORKERS, DUMMY_WORKFLOWS, DUMMY_DASHBOARD } from "../dummyData";

export function Dashboard({
  snapshot,
  density,
  onOpenIssue,
}: {
  snapshot: Snapshot;
  density: "comfortable" | "dense";
  onOpenIssue: (id: string) => void;
}) {
  const stats = dashboardStats(snapshot);
  const running = runningAgents(snapshot);
  const queued = queuedAgents(snapshot);
  const tokens24h = snapshot.codex_totals.total_tokens;
  const totalsCost = (tokens24h / 1_000_000) * 8; // crude cost estimate ($8/M tokens placeholder)
  const cost24h = totalsCost > 0 ? totalsCost : DUMMY_DASHBOARD.cost24h;
  const successRate = DUMMY_DASHBOARD.successRate;
  const merged24h = stats.merged24h || DUMMY_DASHBOARD.merged24h;

  const pad = density === "dense" ? 14 : 20;
  const liveRows: SymAgent[] = [...running, ...queued.slice(0, 2)];

  const repos = snapshot.repos ?? [];
  const repoLabel = repos.length === 1 ? repos[0] : repos.length === 0 ? "—" : `${repos.length} repos`;

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      {/* Header */}
      <div className="flex items-end justify-between gap-3">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">
            Orchestrator · {repoLabel}
          </div>
          <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
            {greet()}, Maya.
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {stats.running} agents running · {stats.queued} queued · {stats.failed} need attention
          </div>
        </div>
        <div className="flex gap-2">
          <SecondaryBtn icon={<IconRetry size={13} />}>Sync issues</SecondaryBtn>
          <PrimaryBtn icon={<IconPlus size={13} />}>Dispatch</PrimaryBtn>
        </div>
      </div>

      {/* Stat strip */}
      <div
        className="sym-stat-strip"
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
          gap: 1,
          background: "#222222",
          border: "1px solid #222222",
          borderRadius: 8,
          overflow: "hidden",
        }}
      >
        <StatTile label="Active" value={stats.running} sublabel={`${stats.queued} queued`} accent="#10b981" spark={[2, 3, 1, 4, 3, 5, 4, 6, 5, 7, 6, 8]} />
        <StatTile label="In review" value={stats.review} sublabel={stats.review > 0 ? "checks running" : "—"} accent="#3b82f6" />
        <StatTile label="Merged 24h" value={merged24h} sublabel={`${(successRate * 100).toFixed(1)}% success`} accent="#a855f7" spark={DUMMY_DASHBOARD.runsPerHour.slice(-12)} />
        <StatTile label="Spend 24h" value={`$${cost24h.toFixed(2)}`} sublabel={`${fmtNum(tokens24h)} tokens`} accent="#f59e0b" />
      </div>

      {/* Live agents + activity */}
      <div className="sym-twocol" style={{ display: "grid", gridTemplateColumns: "1.6fr 1fr", gap: pad }}>
        <Card>
          <CardHead
            title="Live agents"
            right={
              <div className="flex gap-1">
                <TabPill active>All</TabPill>
                <TabPill>Mine</TabPill>
                <TabPill>Failed</TabPill>
              </div>
            }
          />
          <div>
            {liveRows.length === 0 ? (
              <div className="px-4 py-10 text-center text-textMute text-[12px]">
                No agents active. Dispatch a workflow to get started.
              </div>
            ) : (
              liveRows.map((a, idx) => (
                <LiveAgentRow
                  key={a.internalId}
                  a={a}
                  first={idx === 0}
                  onClick={() => onOpenIssue(a.internalId)}
                />
              ))
            )}
          </div>
        </Card>

        <Card>
          <CardHead
            title="Recent activity"
            right={
              <button className="bg-transparent border-0 text-textDim text-[11px] cursor-pointer p-0">
                View all →
              </button>
            }
          />
          <div className="py-1">
            <ActivityFeed snapshot={snapshot} />
          </div>
        </Card>
      </div>

      {/* Workers + workflows */}
      <div className="sym-twocol" style={{ display: "grid", gridTemplateColumns: "1.4fr 1fr", gap: pad }}>
        <Card>
          <CardHead
            title="Workers"
            right={
              <span className="text-[11px] text-textMute font-mono">
                {DUMMY_WORKERS.filter((w) => w.status === "online").length}/{DUMMY_WORKERS.length} online
              </span>
            }
          />
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(3, 1fr)",
              gap: 1,
              background: "#222222",
              borderTop: "1px solid #222222",
            }}
          >
            {DUMMY_WORKERS.map((w) => (
              <WorkerCell key={w.id} w={w} />
            ))}
          </div>
        </Card>

        <Card>
          <CardHead title="Workflows" right={<span className="text-[11px] text-textMute">{DUMMY_WORKFLOWS.length} defined</span>} />
          <div>
            {DUMMY_WORKFLOWS.map((w, i) => (
              <div
                key={w.id}
                className="flex items-center justify-between px-3.5 py-2.5"
                style={{ borderTop: i === 0 ? "none" : "1px solid #1a1a1a" }}
              >
                <div className="flex items-center gap-2.5 min-w-0">
                  <div className="w-6 h-6 rounded-md bg-panel3 flex items-center justify-center text-textDim shrink-0">
                    <IconWorkflow size={12} />
                  </div>
                  <div className="min-w-0">
                    <div className="text-[12.5px] font-medium text-text">{w.name}</div>
                    <div className="text-[11px] text-textMute font-mono truncate">{w.steps.join(" → ")}</div>
                  </div>
                </div>
                <div className="text-[11px] text-textDim tabular-nums shrink-0 ml-2">{w.uses}</div>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </div>
  );
}

function greet(): string {
  const h = new Date().getHours();
  if (h < 5) return "Late night";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

function StatTile({
  label,
  value,
  sublabel,
  accent,
  spark,
}: {
  label: string;
  value: ReactNode;
  sublabel: string;
  accent: string;
  spark?: number[];
}) {
  return (
    <div className="bg-panel p-4 flex flex-col gap-1.5 min-w-0" style={{ minHeight: 100 }}>
      <div className="flex items-center justify-between gap-1.5">
        <div className="text-[11px] text-textDim font-medium uppercase tracking-[0.3px] truncate">{label}</div>
        <div className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: accent }} />
      </div>
      <div className="flex items-end justify-between gap-2 min-w-0">
        <div className="text-[28px] font-semibold text-text leading-none tabular-nums" style={{ letterSpacing: -0.5 }}>
          {value}
        </div>
        {spark && <Sparkline data={spark} color={accent} w={72} h={22} />}
      </div>
      <div className="text-[11.5px] text-textMute truncate">{sublabel}</div>
    </div>
  );
}

function LiveAgentRow({ a, first, onClick }: { a: SymAgent; first: boolean; onClick: () => void }) {
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
      <span className="sym-live-worker font-mono text-[11px] text-textMute text-right">
        {a.worker ? a.worker : "—"}
      </span>
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

function WorkerCell({ w }: { w: typeof DUMMY_WORKERS[number] }) {
  const off = w.status === "offline";
  const c = off ? "#ef4444" : w.load > 0.8 ? "#f59e0b" : "#10b981";
  return (
    <div className="bg-panel p-3 flex flex-col gap-1.5">
      <div className="flex items-center gap-1.5">
        <span className="w-1.5 h-1.5 rounded-full" style={{ background: off ? "#6a6a6a" : c }} />
        <span className="font-mono text-[11.5px] text-text font-medium">{w.id.replace("w-", "")}</span>
        <span className="text-[10px] text-textMute ml-auto uppercase tracking-[0.5px]">{w.region}</span>
      </div>
      <div className="text-[10.5px] text-textMute font-mono">
        {w.cores}c · {w.ram}
      </div>
      {!off ? (
        <div className="flex items-center gap-1.5">
          <Progress value={w.load} color={c} />
          <span className="font-mono text-[10.5px] text-textDim tabular-nums text-right" style={{ minWidth: 26 }}>
            {(w.load * 100).toFixed(0)}%
          </span>
        </div>
      ) : (
        <div className="text-[10.5px] text-red font-mono">offline · ssh refused</div>
      )}
    </div>
  );
}

function ActivityFeed({ snapshot }: { snapshot: Snapshot }) {
  // Synthesize a recent-events feed from the live snapshot (newest first).
  type Ev = { t: string; kind: string; actor: string; msg: string };
  const events: Ev[] = [];
  for (const r of snapshot.running) {
    if (r.last_event && r.last_event_at) {
      events.push({
        t: r.last_event_at,
        kind: r.last_event,
        actor: r.issue.identifier || r.issue.id,
        msg: r.last_message_tail ?? r.last_event,
      });
    } else {
      events.push({
        t: r.started_at,
        kind: "dispatch",
        actor: r.issue.identifier || r.issue.id,
        msg: `dispatched · ${r.issue.title}`,
      });
    }
  }
  for (const r of snapshot.retrying) {
    events.push({
      t: r.due_at,
      kind: "retry",
      actor: r.identifier || r.issue_id,
      msg: r.error ?? `retry #${r.attempt}`,
    });
  }
  events.sort((a, b) => Date.parse(b.t) - Date.parse(a.t));
  const recent = events.slice(0, 8);

  if (recent.length === 0) {
    return <div className="px-4 py-10 text-center text-textMute text-[12px]">No recent activity.</div>;
  }

  return (
    <>
      {recent.map((a, i) => (
        <div key={i} className="flex items-start gap-2.5 px-3.5 py-2 text-[12px]">
          <span className="w-1.5 h-1.5 rounded-full mt-1.5 shrink-0" style={{ background: kindColor(a.kind) }} />
          <div className="flex-1 min-w-0">
            <div className="text-text">
              <span className="font-mono text-[11px] text-textDim">{a.actor}</span>
              <span className="ml-1.5 text-textDim truncate">{truncate(a.msg, 80)}</span>
            </div>
          </div>
          <span className="font-mono text-[10.5px] text-textMute tabular-nums shrink-0">{shortTime(a.t)}</span>
        </div>
      ))}
    </>
  );
}

function kindColor(kind: string): string {
  if (kind.includes("fail") || kind.includes("error")) return "#ef4444";
  if (kind.includes("merge")) return "#a855f7";
  if (kind.includes("pr") || kind.includes("review")) return "#3b82f6";
  if (kind.includes("dispatch") || kind.includes("started")) return "#10b981";
  if (kind.includes("retry")) return "#f59e0b";
  return "#9a9a9a";
}

function shortTime(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "—";
  const d = new Date(t);
  return d.toTimeString().slice(0, 8);
}

function truncate(s: string, n: number): string {
  if (s.length <= n) return s;
  return s.slice(0, n - 1) + "…";
}

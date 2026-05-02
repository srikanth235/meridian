import type { Snapshot } from "../types";
import { Card, CardHead, Progress } from "../atoms";
import { IconWorkflow } from "../icons";
import { DUMMY_WORKERS, DUMMY_WORKFLOWS } from "../dummyData";

export function Workers({ density }: { snapshot: Snapshot; density: "comfortable" | "dense" }) {
  const pad = density === "dense" ? 14 : 20;
  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <PageHeader eyebrow="Infrastructure" title="Workers" subtitle={`${DUMMY_WORKERS.filter(w => w.status === "online").length}/${DUMMY_WORKERS.length} workers online across 3 regions`} />
      <Card>
        <CardHead title="All workers" />
        <table className="w-full text-[12px]" style={{ borderCollapse: "collapse" }}>
          <thead>
            <tr className="text-[10px] uppercase tracking-[0.6px] text-textMute font-medium">
              <th className="text-left py-2.5 px-4 font-medium">Host</th>
              <th className="text-left py-2.5 font-medium">Region</th>
              <th className="text-left py-2.5 font-medium">Cores</th>
              <th className="text-left py-2.5 font-medium">RAM</th>
              <th className="text-left py-2.5 font-medium">Load</th>
              <th className="text-left py-2.5 px-4 font-medium">Status</th>
            </tr>
          </thead>
          <tbody>
            {DUMMY_WORKERS.map((w) => {
              const off = w.status === "offline";
              const c = off ? "#ef4444" : w.load > 0.8 ? "#f59e0b" : "#10b981";
              return (
                <tr key={w.id} className="border-t border-borderS">
                  <td className="py-3 px-4 font-mono text-[11.5px] text-text">{w.host}</td>
                  <td className="py-3 text-[11.5px] text-textDim uppercase tracking-[0.5px]">{w.region}</td>
                  <td className="py-3 text-[11.5px] text-textDim font-mono tabular-nums">{w.cores}</td>
                  <td className="py-3 text-[11.5px] text-textDim font-mono tabular-nums">{w.ram}</td>
                  <td className="py-3 pr-6" style={{ minWidth: 160 }}>
                    {off ? (
                      <span className="text-red font-mono text-[11px]">offline</span>
                    ) : (
                      <div className="flex items-center gap-2">
                        <Progress value={w.load} color={c} />
                        <span className="font-mono text-[11px] text-textDim tabular-nums">{(w.load * 100).toFixed(0)}%</span>
                      </div>
                    )}
                  </td>
                  <td className="py-3 px-4">
                    <span className="flex items-center gap-1.5 text-[11.5px]">
                      <span className="w-1.5 h-1.5 rounded-full" style={{ background: off ? "#6a6a6a" : c }} />
                      <span className="text-text capitalize">{w.status}</span>
                    </span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </Card>
    </div>
  );
}

export function Workflows({ density }: { snapshot: Snapshot; density: "comfortable" | "dense" }) {
  const pad = density === "dense" ? 14 : 20;
  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <PageHeader eyebrow="Recipes" title="Workflows" subtitle={`${DUMMY_WORKFLOWS.length} workflows defined in WORKFLOW.md`} />
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))", gap: pad }}>
        {DUMMY_WORKFLOWS.map((w) => (
          <Card key={w.id}>
            <div className="p-4 flex flex-col gap-2.5">
              <div className="flex items-center gap-2.5">
                <div className="w-7 h-7 rounded-md bg-panel3 flex items-center justify-center text-textDim shrink-0">
                  <IconWorkflow size={14} />
                </div>
                <div className="min-w-0">
                  <div className="text-[13px] font-semibold text-text">{w.name}</div>
                  <div className="text-[11px] text-textMute font-mono tabular-nums">{w.uses} uses · {w.timeoutMin}m timeout</div>
                </div>
              </div>
              <div className="text-[12px] text-textDim leading-snug">{w.description}</div>
              <div className="flex flex-wrap gap-1.5 pt-1">
                {w.steps.map((s, i) => (
                  <span
                    key={s + i}
                    className="font-mono text-[10.5px] text-textDim px-1.5 py-0.5 rounded"
                    style={{ background: "#1c1c1c", border: "1px solid #1a1a1a" }}
                  >
                    {s}
                  </span>
                ))}
              </div>
            </div>
          </Card>
        ))}
      </div>
    </div>
  );
}

export function Activity({ snapshot, density }: { snapshot: Snapshot; density: "comfortable" | "dense" }) {
  const pad = density === "dense" ? 14 : 20;
  type Ev = { t: string; kind: string; actor: string; msg: string };
  const events: Ev[] = [];
  for (const r of snapshot.running) {
    events.push({
      t: r.last_event_at ?? r.started_at,
      kind: r.last_event ?? "running",
      actor: r.issue.identifier || r.issue.id,
      msg: r.last_message_tail ?? r.issue.title,
    });
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

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <PageHeader eyebrow="Live feed" title="Activity" subtitle={`${events.length} recent events`} />
      <Card>
        {events.length === 0 ? (
          <div className="px-4 py-16 text-center text-textMute text-[12px]">No activity yet.</div>
        ) : (
          events.map((a, i) => (
            <div
              key={i}
              className="flex items-start gap-3 px-4 py-2.5 text-[12.5px]"
              style={{ borderTop: i === 0 ? "none" : "1px solid #1a1a1a" }}
            >
              <span className="w-1.5 h-1.5 rounded-full mt-2 shrink-0" style={{ background: kindColor(a.kind) }} />
              <span className="font-mono text-[11px] text-textDim shrink-0" style={{ width: 88 }}>
                {a.actor}
              </span>
              <span className="flex-1 text-textDim min-w-0 truncate">{a.msg}</span>
              <span className="font-mono text-[10.5px] text-textMute tabular-nums shrink-0">{shortTime(a.t)}</span>
            </div>
          ))
        )}
      </Card>
    </div>
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
  return new Date(t).toTimeString().slice(0, 8);
}

function PageHeader({ eyebrow, title, subtitle }: { eyebrow: string; title: string; subtitle: string }) {
  return (
    <div>
      <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">{eyebrow}</div>
      <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
        {title}
      </h1>
      <div className="text-[13px] text-textDim mt-1">{subtitle}</div>
    </div>
  );
}

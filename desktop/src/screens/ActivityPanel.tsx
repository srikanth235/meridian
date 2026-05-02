import { useEffect } from "react";
import type { Snapshot } from "../types";
import { IconX } from "../icons";

export function ActivityPanel({
  snapshot,
  onClose,
}: {
  snapshot: Snapshot;
  onClose: () => void;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

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
    <div
      onClick={onClose}
      className="fixed inset-0 z-40"
      style={{ background: "var(--overlay)" }}
    >
      <aside
        onClick={(e) => e.stopPropagation()}
        className="absolute top-0 right-0 h-full bg-panel border-l border-border flex flex-col"
        style={{ width: 420, maxWidth: "92vw", boxShadow: "var(--shadow)" }}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-borderS">
          <div>
            <div className="text-[10px] uppercase tracking-[0.6px] text-textMute font-semibold">Live feed</div>
            <div className="text-[14px] font-semibold text-text">Activity</div>
          </div>
          <button
            onClick={onClose}
            className="cursor-pointer text-textMute hover:text-text"
            style={{ width: 24, height: 24, borderRadius: 6, background: "transparent", border: "1px solid var(--border)" }}
          >
            <IconX size={11} />
          </button>
        </div>
        <div className="overflow-y-auto flex-1">
          {events.length === 0 ? (
            <div className="px-4 py-12 text-center text-textMute text-[12px]">No activity yet.</div>
          ) : (
            events.map((a, i) => (
              <div
                key={i}
                className="flex items-start gap-3 px-4 py-2.5 text-[12.5px]"
                style={{ borderTop: i === 0 ? "none" : "1px solid var(--borderS)" }}
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
        </div>
      </aside>
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

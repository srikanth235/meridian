import type { Snapshot } from "../types";
import { Card } from "../atoms";
import { IconWorkflow } from "../icons";
import { DUMMY_WORKFLOWS } from "../dummyData";

export function Routing({
  density,
}: {
  snapshot: Snapshot;
  density: "comfortable" | "dense";
}) {
  const pad = density === "dense" ? 14 : 20;
  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <div className="flex items-end justify-between gap-3">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">Sources</div>
          <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
            Routing
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {DUMMY_WORKFLOWS.length} recipes defined in WORKFLOW.md · manual assignment is in effect
          </div>
        </div>
      </div>
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
                    style={{ background: "var(--panel3)", border: "1px solid var(--borderS)" }}
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

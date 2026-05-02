import { useMemo } from "react";
import type { Snapshot } from "../types";
import { Card, Pill, SecondaryBtn } from "../atoms";
import { IconRepos, IconRetry } from "../icons";

export function Repos({
  snapshot,
  density,
  repoFilter,
  onSetRepoFilter,
}: {
  snapshot: Snapshot;
  density: "comfortable" | "dense";
  repoFilter: string | null;
  onSetRepoFilter: (slug: string | null) => void;
}) {
  const pad = density === "dense" ? 14 : 20;

  // Prefer repos_detail; fall back to deriving from repos[] + snapshot.
  const rows = useMemo(() => {
    if (snapshot.repos_detail && snapshot.repos_detail.length) return snapshot.repos_detail;
    return (snapshot.repos ?? []).map((slug) => {
      const inFlight = snapshot.running.filter((r) => r.issue.repo === slug).length;
      const errored = snapshot.retrying.length;
      let openIssues = 0;
      for (const col of snapshot.kanban?.columns ?? []) {
        for (const i of col.issues) {
          if ((i.repo ?? null) === slug && i.state !== "closed") openIssues += 1;
        }
      }
      return {
        slug,
        open_issues: openIssues,
        in_flight: inFlight,
        errored,
        last_synced_at: snapshot.generated_at,
      };
    });
  }, [snapshot]);

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <div className="flex items-end justify-between gap-3">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">Sources</div>
          <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
            Repos
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {rows.length} {rows.length === 1 ? "repo" : "repos"} connected
          </div>
        </div>
      </div>

      <Card>
        {rows.length === 0 ? (
          <div className="px-4 py-16 text-center text-textMute text-[12px] flex flex-col items-center gap-3">
            <span><IconRepos size={20} /></span>
            <div>No repos configured. Edit WORKFLOW.md to add a tracker target.</div>
          </div>
        ) : (
          <table className="w-full text-[12px]" style={{ borderCollapse: "collapse" }}>
            <thead>
              <tr className="text-[10px] uppercase tracking-[0.6px] text-textMute font-medium">
                <th className="text-left py-2.5 px-4 font-medium">Repo</th>
                <th className="text-right py-2.5 font-medium">Open</th>
                <th className="text-right py-2.5 font-medium">In flight</th>
                <th className="text-right py-2.5 font-medium">Errored</th>
                <th className="text-left py-2.5 font-medium">Last synced</th>
                <th className="text-right py-2.5 px-4 font-medium">Scope</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r, idx) => {
                const active = repoFilter === r.slug;
                return (
                  <tr
                    key={r.slug}
                    className="sym-row"
                    style={{
                      borderTop: idx === 0 ? "1px solid var(--borderS)" : "1px solid var(--borderS)",
                      background: active ? "var(--panel2)" : undefined,
                    }}
                  >
                    <td className="py-3 px-4 font-mono text-[12px] text-text">{r.slug}</td>
                    <td className="py-3 text-right tabular-nums text-textDim font-mono">{r.open_issues}</td>
                    <td className="py-3 text-right tabular-nums">
                      {r.in_flight > 0 ? (
                        <Pill tone="ok">{r.in_flight}</Pill>
                      ) : (
                        <span className="text-textMute font-mono">0</span>
                      )}
                    </td>
                    <td className="py-3 text-right tabular-nums">
                      {r.errored > 0 ? (
                        <Pill tone="err">{r.errored}</Pill>
                      ) : (
                        <span className="text-textMute font-mono">0</span>
                      )}
                    </td>
                    <td className="py-3 text-textMute font-mono text-[11px]">
                      {r.last_synced_at ? relTime(r.last_synced_at) : "—"}
                    </td>
                    <td className="py-3 px-4 text-right">
                      <div className="inline-flex items-center gap-2">
                        <SecondaryBtn icon={<IconRetry size={11} />}>Sync</SecondaryBtn>
                        <button
                          onClick={() => onSetRepoFilter(active ? null : r.slug)}
                          className="cursor-pointer"
                          style={{
                            height: 24,
                            padding: "0 10px",
                            background: active ? "var(--text)" : "transparent",
                            color: active ? "var(--bg)" : "var(--textDim)",
                            border: active ? "0" : "1px solid var(--border)",
                            borderRadius: 6,
                            fontSize: 11,
                            fontWeight: 500,
                          }}
                        >
                          {active ? "Scoped" : "Scope"}
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}

function relTime(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "—";
  const sec = Math.max(0, Math.floor((Date.now() - t) / 1000));
  if (sec < 60) return `${sec}s ago`;
  const m = Math.floor(sec / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

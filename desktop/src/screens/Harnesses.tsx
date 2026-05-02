import { useMemo, useState } from "react";
import type { Harness, HarnessId, Snapshot } from "../types";
import { Card, HarnessAvatar, PageSearch, Pill, Progress } from "../atoms";
import { harnessMatches } from "../search";

export function Harnesses({
  snapshot,
  density,
  query,
  onQueryChange,
  onSetConcurrency,
}: {
  snapshot: Snapshot;
  density: "comfortable" | "dense";
  query: string;
  onQueryChange: (q: string) => void;
  onSetConcurrency: (id: HarnessId, n: number) => void;
}) {
  const pad = density === "dense" ? 14 : 20;
  const harnesses = snapshot.harnesses ?? [];
  const filtered = useMemo(
    () => harnesses.filter((h) => harnessMatches(h, query)),
    [harnesses, query],
  );
  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);

  const onRefresh = async () => {
    if (refreshing) return;
    setRefreshing(true);
    setRefreshError(null);
    try {
      const res = await fetch("/api/harnesses/refresh", { method: "POST" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      // Snapshot is pushed via WS on StateChanged, so the UI will reflect
      // the new list automatically — no need to merge the response here.
    } catch (e) {
      setRefreshError(e instanceof Error ? e.message : String(e));
    } finally {
      setRefreshing(false);
    }
  };

  const inFlightById = useMemo(() => {
    const m = new Map<HarnessId, number>();
    for (const r of snapshot.running) {
      const h = (r.harness ?? r.issue.assignee) as HarnessId | null | undefined;
      if (!h) continue;
      m.set(h, (m.get(h) ?? 0) + 1);
    }
    return m;
  }, [snapshot.running]);

  const totalCap = harnesses.reduce((s, h) => s + h.concurrency, 0);
  const totalLoad = Array.from(inFlightById.values()).reduce((a, b) => a + b, 0);
  const available = harnesses.filter((h) => h.available).length;

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <div className="flex items-end justify-between gap-3">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">Team</div>
          <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
            Harnesses
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {available}/{harnesses.length} available · {totalLoad}/{totalCap} concurrent slots used
            {refreshError && (
              <span className="ml-2 text-err">· refresh failed: {refreshError}</span>
            )}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <PageSearch
            pageId="harnesses"
            value={query}
            onChange={onQueryChange}
            placeholder="Search harnesses…"
          />
          <button
            type="button"
            onClick={onRefresh}
            disabled={refreshing}
            title="Re-scan PATH for installed harness CLIs"
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-border bg-panel2 hover:bg-panel3 text-[12px] font-medium text-text disabled:opacity-60 disabled:cursor-not-allowed transition-colors"
          >
            <RefreshIcon spinning={refreshing} />
            {refreshing ? "Refreshing…" : "Refresh"}
          </button>
        </div>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))", gap: pad }}>
        {filtered.map((h) => (
          <HarnessCard
            key={h.id}
            harness={h}
            inFlight={inFlightById.get(h.id) ?? 0}
            onSetConcurrency={(n) => onSetConcurrency(h.id, n)}
          />
        ))}
      </div>
    </div>
  );
}

function RefreshIcon({ spinning }: { spinning: boolean }) {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={spinning ? { animation: "meridian-spin 0.8s linear infinite" } : undefined}
      aria-hidden
    >
      <path d="M21 12a9 9 0 0 1-15.36 6.36L3 16" />
      <path d="M3 12a9 9 0 0 1 15.36-6.36L21 8" />
      <path d="M21 3v5h-5" />
      <path d="M3 21v-5h5" />
    </svg>
  );
}

function HarnessCard({
  harness,
  inFlight,
  onSetConcurrency,
}: {
  harness: Harness;
  inFlight: number;
  onSetConcurrency: (n: number) => void;
}) {
  const cap = harness.concurrency;
  const load = cap === 0 ? 0 : inFlight / cap;
  return (
    <Card>
      <div className="p-4 flex flex-col gap-3">
        <div className="flex items-start gap-3">
          <HarnessAvatar harness={harness} size={36} />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <div className="text-[14px] font-semibold text-text truncate">{harness.name}</div>
              {harness.available ? (
                <Pill tone="ok">available</Pill>
              ) : (
                <Pill tone="err">offline</Pill>
              )}
            </div>
            <div className="text-[11px] text-textMute font-mono mt-0.5">
              {harness.binary}
              {harness.version ? ` · v${harness.version}` : ""}
            </div>
          </div>
        </div>

        {harness.capabilities && harness.capabilities.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {harness.capabilities.map((c) => (
              <Pill key={c} tone="muted">{c}</Pill>
            ))}
          </div>
        )}

        <div>
          <div className="flex items-center justify-between text-[10.5px] text-textMute font-mono mb-1.5">
            <span>In flight</span>
            <span className="tabular-nums">{inFlight}/{cap}</span>
          </div>
          <Progress value={load} color={harness.color} />
        </div>

        <div className="flex items-center gap-3">
          <span className="text-[10.5px] uppercase tracking-[0.6px] text-textMute font-medium">Concurrency</span>
          <input
            type="range"
            min={0}
            max={8}
            step={1}
            value={cap}
            disabled={!harness.available}
            onChange={(e) => onSetConcurrency(parseInt(e.target.value, 10))}
            style={{ flex: 1, accentColor: harness.color }}
          />
          <span className="font-mono text-[12px] text-text tabular-nums" style={{ minWidth: 20, textAlign: "right" }}>
            {cap}
          </span>
        </div>
      </div>
    </Card>
  );
}

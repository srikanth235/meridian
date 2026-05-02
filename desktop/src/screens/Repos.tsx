import { useMemo, useState } from "react";
import type { Repo, Snapshot } from "../types";
import { Card, PageSearch, Pill } from "../atoms";
import { IconRepos } from "../icons";
import { repoMatches } from "../search";

export function Repos({
  snapshot,
  density,
  repoFilter,
  query,
  onQueryChange,
  onSetRepoFilter,
}: {
  snapshot: Snapshot;
  density: "comfortable" | "dense";
  repoFilter: string | null;
  query: string;
  onQueryChange: (q: string) => void;
  onSetRepoFilter: (slug: string | null) => void;
}) {
  const pad = density === "dense" ? 14 : 20;

  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [pendingSlug, setPendingSlug] = useState<string | null>(null);
  const [addSlug, setAddSlug] = useState("");
  const [adding, setAdding] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);

  const rows: Repo[] = snapshot.available_repos ?? [];
  const status = snapshot.repo_status;
  const connectedCount = rows.filter((r) => r.connected).length;

  const filtered = useMemo(
    () => rows.filter((r) => repoMatches(r, query)),
    [rows, query],
  );
  const connectedRows = useMemo(() => filtered.filter((r) => r.connected), [filtered]);
  const availableRows = useMemo(() => filtered.filter((r) => !r.connected), [filtered]);

  const onRefresh = async () => {
    if (refreshing) return;
    setRefreshing(true);
    setRefreshError(null);
    try {
      const res = await fetch("/api/repos/refresh", { method: "POST" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
    } catch (e) {
      setRefreshError(e instanceof Error ? e.message : String(e));
    } finally {
      setRefreshing(false);
    }
  };

  const onAddSlug = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = addSlug.trim();
    if (!trimmed || adding) return;
    setAdding(true);
    setAddError(null);
    try {
      const res = await fetch("/api/repos/add", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ slug: trimmed }),
      });
      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        throw new Error(body.error ?? `HTTP ${res.status}`);
      }
      setAddSlug("");
    } catch (e) {
      setAddError(e instanceof Error ? e.message : String(e));
    } finally {
      setAdding(false);
    }
  };

  const toggleConnect = async (slug: string, next: boolean) => {
    setPendingSlug(slug);
    try {
      const res = await fetch("/api/repos/connect", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ slug, connected: next }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
    } catch (e) {
      console.error("toggle repo connect failed", e);
    } finally {
      setPendingSlug(null);
    }
  };

  // Per-repo runtime stats derived from the snapshot.
  const inFlightBySlug = useMemo(() => {
    const m = new Map<string, number>();
    for (const r of snapshot.running) {
      const slug = r.issue.repo;
      if (!slug) continue;
      m.set(slug, (m.get(slug) ?? 0) + 1);
    }
    return m;
  }, [snapshot.running]);

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <div className="flex items-end justify-between gap-3">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">Sources</div>
          <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
            Repos
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {connectedCount} connected · {rows.length} discoverable from gh
            {refreshError && (
              <span className="ml-2 text-err">· refresh failed: {refreshError}</span>
            )}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <PageSearch
            pageId="repos"
            value={query}
            onChange={onQueryChange}
            placeholder="Search repos…"
          />
          <form onSubmit={onAddSlug} className="flex items-center gap-1.5">
            <input
              type="text"
              value={addSlug}
              onChange={(e) => setAddSlug(e.target.value)}
              placeholder="owner/name"
              title="Add a repo by slug — useful when gh discovery misses it (e.g. org admin without team grant)"
              spellCheck={false}
              autoComplete="off"
              className="font-mono text-[12px] px-2.5 py-1.5 rounded-md border border-border bg-panel2 text-text placeholder:text-textMute focus:outline-none focus:border-textDim"
              style={{ width: 180 }}
            />
            <button
              type="submit"
              disabled={adding || !addSlug.trim()}
              className="inline-flex items-center px-3 py-1.5 rounded-md border border-border bg-panel2 hover:bg-panel3 text-[12px] font-medium text-text disabled:opacity-60 disabled:cursor-not-allowed transition-colors"
            >
              {adding ? "Adding…" : "Add"}
            </button>
          </form>
          <button
            type="button"
            onClick={onRefresh}
            disabled={refreshing}
            title="Re-run gh repo list"
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-border bg-panel2 hover:bg-panel3 text-[12px] font-medium text-text disabled:opacity-60 disabled:cursor-not-allowed transition-colors"
          >
            <RefreshIcon spinning={refreshing} />
            {refreshing ? "Refreshing…" : "Refresh"}
          </button>
        </div>
      </div>
      {addError && (
        <div className="text-[11px] text-err -mt-1">add failed: {addError}</div>
      )}

      {status && !status.gh_available && (
        <Card>
          <div className="px-4 py-4 text-[12px] text-textDim">
            <div className="text-text font-medium mb-1">gh CLI not detected</div>
            Install GitHub's <span className="font-mono">gh</span> CLI to discover your repos automatically.{" "}
            <a className="text-blue underline" href="https://cli.github.com/" target="_blank" rel="noreferrer">cli.github.com</a>
            {status.error && <div className="mt-1 text-textMute font-mono text-[11px]">{status.error}</div>}
          </div>
        </Card>
      )}

      {status && status.gh_available && !status.gh_authenticated && (
        <Card>
          <div className="px-4 py-4 text-[12px] text-textDim">
            <div className="text-text font-medium mb-1">gh isn't authenticated</div>
            Run <span className="font-mono bg-panel3 px-1.5 py-0.5 rounded">gh auth login</span> in your terminal, then refresh.
            {status.error && <div className="mt-1 text-textMute font-mono text-[11px]">{status.error}</div>}
          </div>
        </Card>
      )}

      <Card>
        {filtered.length === 0 ? (
          <div className="px-4 py-16 text-center text-textMute text-[12px] flex flex-col items-center gap-3">
            <span><IconRepos size={20} /></span>
            <div>{rows.length === 0 ? "No repos discovered yet. Try refreshing." : "No repos match filter."}</div>
          </div>
        ) : (
          <table className="w-full text-[12px]" style={{ borderCollapse: "collapse" }}>
            <thead>
              <tr className="text-[10px] uppercase tracking-[0.6px] text-textMute font-medium">
                <th className="text-left py-2.5 pl-4 font-medium" style={{ width: 130 }}>State</th>
                <th className="text-left py-2.5 font-medium">Repo</th>
                <th className="text-left py-2.5 font-medium">Language</th>
                <th className="text-right py-2.5 font-medium">In flight</th>
                <th className="text-left py-2.5 pl-4 font-medium">Last synced</th>
                <th className="text-right py-2.5 px-4 font-medium">Actions</th>
              </tr>
            </thead>
            <tbody>
              {connectedRows.length > 0 && (
                <SectionHeaderRow
                  label="Connected"
                  count={connectedRows.length}
                  tone="ok"
                />
              )}
              {connectedRows.map((r) => (
                <RepoRow
                  key={r.slug}
                  r={r}
                  active={repoFilter === r.slug}
                  inFlight={inFlightBySlug.get(r.slug) ?? 0}
                  pending={pendingSlug === r.slug}
                  onToggle={toggleConnect}
                  onSetRepoFilter={onSetRepoFilter}
                />
              ))}
              {availableRows.length > 0 && (
                <SectionHeaderRow
                  label="Available"
                  count={availableRows.length}
                  tone="muted"
                />
              )}
              {availableRows.map((r) => (
                <RepoRow
                  key={r.slug}
                  r={r}
                  active={repoFilter === r.slug}
                  inFlight={inFlightBySlug.get(r.slug) ?? 0}
                  pending={pendingSlug === r.slug}
                  onToggle={toggleConnect}
                  onSetRepoFilter={onSetRepoFilter}
                />
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}

function SectionHeaderRow({
  label,
  count,
  tone,
}: {
  label: string;
  count: number;
  tone: "ok" | "muted";
}) {
  const dot = tone === "ok" ? "var(--accent)" : "var(--textMute)";
  return (
    <tr>
      <td colSpan={6} className="px-4 pt-4 pb-2">
        <div className="flex items-center gap-2 text-[10px] uppercase tracking-[0.6px] font-medium text-textDim">
          <span
            aria-hidden
            style={{
              width: 6,
              height: 6,
              borderRadius: 999,
              background: dot,
              display: "inline-block",
            }}
          />
          <span>{label}</span>
          <span className="text-textMute">· {count}</span>
        </div>
      </td>
    </tr>
  );
}

function RepoRow({
  r,
  active,
  inFlight,
  pending,
  onToggle,
  onSetRepoFilter,
}: {
  r: Repo;
  active: boolean;
  inFlight: number;
  pending: boolean;
  onToggle: (slug: string, next: boolean) => void;
  onSetRepoFilter: (slug: string | null) => void;
}) {
  const slugColor = r.connected ? "var(--text)" : "var(--textDim)";
  return (
    <tr
      className="sym-row"
      style={{
        borderTop: "1px solid var(--borderS)",
        background: active ? "var(--panel2)" : undefined,
        opacity: r.is_archived ? 0.55 : 1,
      }}
    >
      <td className="py-3 pl-4">
        <ConnectToggle
          connected={r.connected}
          pending={pending}
          onToggle={(next) => onToggle(r.slug, next)}
        />
      </td>
      <td className="py-3">
        <div className="flex items-center gap-2">
          <span className="font-mono text-[12px]" style={{ color: slugColor }}>
            {r.slug}
          </span>
          {r.is_private && <Pill tone="muted">private</Pill>}
          {r.is_archived && <Pill tone="muted">archived</Pill>}
        </div>
        {r.description && (
          <div className="text-textMute text-[11px] mt-0.5 truncate" style={{ maxWidth: 480 }}>
            {r.description}
          </div>
        )}
      </td>
      <td className="py-3 text-textDim text-[11px]">
        {r.primary_language ?? "—"}
      </td>
      <td className="py-3 text-right tabular-nums">
        {inFlight > 0 ? (
          <Pill tone="ok">{inFlight}</Pill>
        ) : (
          <span className="text-textMute font-mono">0</span>
        )}
      </td>
      <td className="py-3 pl-4 text-textMute font-mono text-[11px]">
        {r.last_synced_at ? relTime(r.last_synced_at) : "—"}
      </td>
      <td className="py-3 px-4 text-right">
        <div className="inline-flex items-center gap-2">
          {r.url && (
            <a
              href={r.url}
              target="_blank"
              rel="noreferrer"
              className="text-textDim hover:text-text text-[11px]"
              title="Open on GitHub"
            >
              ↗
            </a>
          )}
          {r.connected && (
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
          )}
        </div>
      </td>
    </tr>
  );
}

function ConnectToggle({
  connected,
  pending,
  onToggle,
}: {
  connected: boolean;
  pending: boolean;
  onToggle: (next: boolean) => void;
}) {
  // Filled green pill when connected, outlined neutral when not. Whole pill
  // is the click target so the connection state is also the connect button.
  const base: React.CSSProperties = {
    height: 22,
    padding: "0 8px",
    borderRadius: 999,
    fontSize: 11,
    fontWeight: 500,
    display: "inline-flex",
    alignItems: "center",
    gap: 5,
    cursor: pending ? "wait" : "pointer",
    transition: "background-color 0.12s, border-color 0.12s, color 0.12s",
  };
  const styled: React.CSSProperties = connected
    ? {
        ...base,
        background: "rgb(16 185 129 / 0.16)",
        color: "var(--accent)",
        border: "1px solid rgb(16 185 129 / 0.35)",
      }
    : {
        ...base,
        background: "transparent",
        color: "var(--textDim)",
        border: "1px solid var(--border)",
      };
  return (
    <button
      type="button"
      disabled={pending}
      onClick={() => onToggle(!connected)}
      title={connected ? "Click to disconnect" : "Click to connect"}
      style={styled}
    >
      <span
        aria-hidden
        style={{
          width: 7,
          height: 7,
          borderRadius: 999,
          background: connected ? "var(--accent)" : "transparent",
          border: connected ? "0" : "1px solid var(--textMute)",
          display: "inline-block",
        }}
      />
      {connected ? "Connected" : "Connect"}
    </button>
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

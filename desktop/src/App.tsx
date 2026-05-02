import { useEffect, useMemo, useState } from "react";
import { useSnapshot } from "./useSnapshot";
import type { Issue, Snapshot } from "./types";
import { Kbd, fmtElapsed } from "./atoms";
import {
  IconActivity,
  IconAgents,
  IconBranch,
  IconDashboard,
  IconLogo,
  IconPause,
  IconPlay,
  IconRetry,
  IconSearch,
  IconSettings,
  IconWorkers,
  IconWorkflow,
} from "./icons";
import { Dashboard } from "./screens/Dashboard";
import { AgentsList } from "./screens/Agents";
import { Workers, Workflows, Activity } from "./screens/Other";
import { IssueDetail } from "./screens/IssueDetail";

type Route =
  | { name: "dashboard" }
  | { name: "agents" }
  | { name: "workflows" }
  | { name: "workers" }
  | { name: "activity" }
  | { name: "issue"; id: string };

type ConnState = "connecting" | "open" | "closed";
type Density = "comfortable" | "dense";

export function App() {
  const { snapshot, conn } = useSnapshot();
  const [route, setRoute] = useState<Route>({ name: "dashboard" });
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [density] = useState<Density>("comfortable");

  // Re-render every second so relative timestamps and elapsed counters tick.
  const [, force] = useState(0);
  useEffect(() => {
    const id = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  // ⌘K to open palette
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setPaletteOpen((p) => !p);
      } else if (e.key === "Escape") {
        setPaletteOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const selectedIssue = useMemo<Issue | null>(() => {
    if (route.name !== "issue" || !snapshot) return null;
    for (const col of snapshot.kanban?.columns ?? []) {
      const f = col.issues.find((i) => i.id === route.id);
      if (f) return f;
    }
    for (const i of snapshot.kanban?.unsorted ?? []) if (i.id === route.id) return i;
    for (const r of snapshot.running) if (r.issue.id === route.id) return r.issue;
    return null;
  }, [route, snapshot]);

  const openIssue = (id: string) => setRoute({ name: "issue", id });

  return (
    <div className="w-screen h-screen flex flex-col bg-bg text-text overflow-hidden" style={{ fontSize: 13 }}>
      <Titlebar conn={conn} snapshot={snapshot} />
      <div className="flex-1 flex min-h-0">
        <Sidebar
          snapshot={snapshot}
          route={route}
          setRoute={setRoute}
          onPalette={() => setPaletteOpen(true)}
        />
        <main className="flex-1 overflow-auto min-w-0 bg-bg">
          {!snapshot ? (
            <div className="h-full flex items-center justify-center text-textMute text-[13px]">
              connecting…
            </div>
          ) : route.name === "dashboard" ? (
            <Dashboard snapshot={snapshot} density={density} onOpenIssue={openIssue} />
          ) : route.name === "agents" ? (
            <AgentsList snapshot={snapshot} density={density} onOpenIssue={openIssue} />
          ) : route.name === "workflows" ? (
            <Workflows snapshot={snapshot} density={density} />
          ) : route.name === "workers" ? (
            <Workers snapshot={snapshot} density={density} />
          ) : route.name === "activity" ? (
            <Activity snapshot={snapshot} density={density} />
          ) : route.name === "issue" && selectedIssue ? (
            <IssueDetail
              snapshot={snapshot}
              issue={selectedIssue}
              onBack={() => setRoute({ name: "agents" })}
            />
          ) : (
            <div className="h-full flex items-center justify-center text-textMute text-[13px]">
              issue not found in current snapshot
            </div>
          )}
        </main>
      </div>

      {paletteOpen && snapshot && (
        <CommandPalette
          snapshot={snapshot}
          onClose={() => setPaletteOpen(false)}
          setRoute={setRoute}
          openIssue={openIssue}
        />
      )}
    </div>
  );
}

/* ─────────────────────────  Titlebar  ───────────────────────── */

function Titlebar({ conn, snapshot }: { conn: ConnState; snapshot: Snapshot | null }) {
  // The Electron shell uses macOS hidden-inset chrome — traffic lights live on
  // the left (~80px reserved), so we pad-left and use this strip as a drag region.
  const repos = snapshot?.repos ?? [];
  const subtitle =
    repos.length === 0 ? "—" : repos.length === 1 ? repos[0] : `${repos.length} repos`;

  return (
    <header
      className="shrink-0 app-drag bg-chrome border-b border-border"
      style={{ height: 38 }}
    >
      <div className="pl-24 pr-3.5 h-full flex items-center gap-3">
        <div className="flex-1 text-center text-[12px] font-medium text-textDim" style={{ letterSpacing: 0.2 }}>
          Symphony — {subtitle}
        </div>
        <PauseToggle snapshot={snapshot} />
        <ConnPill state={conn} />
      </div>
      {snapshot?.paused && (
        <div className="border-t border-amber/30 text-amber text-[11px] px-4 py-1 text-center" style={{ background: "rgba(245,158,11,0.1)" }}>
          paused — no new agents will dispatch. in-flight workers continue.
        </div>
      )}
    </header>
  );
}

function PauseToggle({ snapshot }: { snapshot: Snapshot | null }) {
  const [busy, setBusy] = useState(false);
  if (!snapshot) return null;
  const paused = snapshot.paused;
  return (
    <button
      disabled={busy}
      onClick={async () => {
        setBusy(true);
        try {
          await fetch(`/api/control/${paused ? "resume" : "pause"}`, { method: "POST" });
        } finally {
          setBusy(false);
        }
      }}
      className={`inline-flex items-center gap-1 h-6 px-2 rounded-md text-[11px] font-medium border cursor-pointer ${
        paused
          ? "text-amber border-amber/30"
          : "text-accent border-accent/30"
      }`}
      style={{ background: paused ? "rgba(245,158,11,0.08)" : "rgba(16,185,129,0.08)" }}
    >
      {busy ? "…" : paused ? <><IconPlay size={10} /> resume</> : <><IconPause size={10} /> pause</>}
    </button>
  );
}

function ConnPill({ state }: { state: ConnState }) {
  const cls =
    state === "open" ? "text-accent" : state === "connecting" ? "text-amber" : "text-red";
  return (
    <span className={`inline-flex items-center gap-1.5 font-mono text-[11px] ${cls}`}>
      <span
        className="w-1.5 h-1.5 rounded-full"
        style={{
          background: state === "open" ? "#10b981" : state === "connecting" ? "#f59e0b" : "#ef4444",
        }}
      />
      {state}
    </span>
  );
}

/* ─────────────────────────  Sidebar  ───────────────────────── */

function Sidebar({
  snapshot,
  route,
  setRoute,
  onPalette,
}: {
  snapshot: Snapshot | null;
  route: Route;
  setRoute: (r: Route) => void;
  onPalette: () => void;
}) {
  const items: Array<{
    id: Route["name"];
    label: string;
    icon: React.ComponentType<{ size?: number }>;
    badge?: number;
  }> = [
    { id: "dashboard", label: "Dashboard", icon: IconDashboard },
    {
      id: "agents",
      label: "Agents",
      icon: IconAgents,
      badge: snapshot?.running.length ?? 0,
    },
    {
      id: "workflows",
      label: "Workflows",
      icon: IconWorkflow,
    },
    { id: "workers", label: "Workers", icon: IconWorkers },
    { id: "activity", label: "Activity", icon: IconActivity },
  ];

  const matches = (id: Route["name"]) =>
    route.name === id || (id === "agents" && route.name === "issue");

  const pinned: Issue[] = useMemo(() => {
    if (!snapshot) return [];
    const out: Issue[] = [];
    const seen = new Set<string>();
    for (const r of snapshot.running) {
      if (!seen.has(r.issue.id)) {
        seen.add(r.issue.id);
        out.push(r.issue);
        if (out.length >= 3) break;
      }
    }
    if (out.length < 3) {
      for (const col of snapshot.kanban?.columns ?? []) {
        for (const i of col.issues) {
          if (out.length >= 3) break;
          if (!seen.has(i.id)) {
            seen.add(i.id);
            out.push(i);
          }
        }
        if (out.length >= 3) break;
      }
    }
    return out;
  }, [snapshot]);

  return (
    <aside
      className="shrink-0 bg-chrome border-r border-border flex flex-col"
      style={{ width: 232, padding: "12px 10px" }}
    >
      {/* Brand */}
      <div className="flex items-center gap-2 px-2 pb-3.5 pt-1.5">
        <div className="text-accent">
          <IconLogo size={20} />
        </div>
        <div>
          <div className="text-[13.5px] font-semibold text-text" style={{ letterSpacing: -0.1 }}>
            Symphony
          </div>
          <div className="text-[10px] text-textMute font-mono mt-0.5">
            v0.4.2 · main
          </div>
        </div>
      </div>

      {/* Search */}
      <button
        onClick={onPalette}
        className="flex items-center gap-2 mb-3 bg-panel2 border border-border rounded-md text-textMute text-[12px] cursor-pointer text-left hover:bg-panel3"
        style={{ height: 30, padding: "0 10px" }}
      >
        <IconSearch size={13} />
        <span className="flex-1">Search or jump to…</span>
        <Kbd>⌘</Kbd>
        <Kbd>K</Kbd>
      </button>

      {/* Nav */}
      <div className="flex flex-col gap-px flex-1 overflow-y-auto">
        <SectionLabel>Workspace</SectionLabel>
        {items.map((item) => {
          const Icon = item.icon;
          const active = matches(item.id);
          return (
            <button
              key={item.id}
              onClick={() => setRoute({ name: item.id } as Route)}
              className={`sym-nav flex items-center gap-2.5 rounded-md text-left cursor-pointer relative ${
                active ? "text-text font-medium" : "text-textDim"
              }`}
              style={{
                height: 30,
                padding: "0 10px",
                background: active ? "#1c1c1c" : "transparent",
                border: 0,
                fontSize: 12.5,
              }}
            >
              {active && (
                <span
                  className="absolute bg-accent rounded-sm"
                  style={{ left: 0, top: 7, bottom: 7, width: 2 }}
                />
              )}
              <Icon size={14} />
              <span className="flex-1">{item.label}</span>
              {item.badge != null && item.badge > 0 && (
                <span
                  className="font-mono text-[10.5px] tabular-nums px-1.5 rounded-sm"
                  style={{
                    background: active ? "#111111" : "#1c1c1c",
                    color: active ? "#ededed" : "#6a6a6a",
                  }}
                >
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}

        {pinned.length > 0 && (
          <>
            <SectionLabel className="mt-3.5">Pinned issues</SectionLabel>
            {pinned.map((i) => (
              <button
                key={i.id}
                onClick={() => setRoute({ name: "issue", id: i.id })}
                className={`sym-nav flex items-center gap-2 rounded-md text-left cursor-pointer ${
                  route.name === "issue" && route.id === i.id
                    ? "text-text"
                    : "text-textDim"
                }`}
                style={{
                  height: 26,
                  padding: "0 10px",
                  background:
                    route.name === "issue" && route.id === i.id ? "#1c1c1c" : "transparent",
                  border: 0,
                  fontSize: 11.5,
                }}
              >
                <span
                  className={`w-1.5 h-1.5 rounded-full shrink-0 ${
                    snapshot?.running.some((r) => r.issue.id === i.id)
                      ? "bg-accent"
                      : "bg-textMute"
                  }`}
                />
                <span className="font-mono text-[10.5px] text-textMute shrink-0">
                  {i.identifier}
                </span>
                <span className="flex-1 truncate">{i.title}</span>
              </button>
            ))}
          </>
        )}
      </div>

      {/* Footer — user */}
      <div
        className="flex items-center gap-2 mt-2 pt-2.5"
        style={{ padding: "10px 8px", borderTop: "1px solid #222222" }}
      >
        <div
          className="rounded-full text-white text-[11px] font-semibold flex items-center justify-center"
          style={{
            width: 26,
            height: 26,
            background: "linear-gradient(135deg, #10b981, #3b82f6)",
          }}
        >
          MR
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-[12px] font-medium text-text">Maya Rodriguez</div>
          <div className="text-[10.5px] text-textMute font-mono">@m.rodriguez</div>
        </div>
        <button className="bg-transparent border-0 text-textMute hover:text-textDim cursor-pointer p-1">
          <IconSettings size={14} />
        </button>
      </div>
    </aside>
  );
}

function SectionLabel({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return (
    <div
      className={`text-[10px] text-textMute font-semibold uppercase ${className}`}
      style={{ letterSpacing: 0.6, padding: "8px 10px 4px" }}
    >
      {children}
    </div>
  );
}

/* ─────────────────────────  Command Palette  ───────────────────────── */

interface Cmd {
  id: string;
  label: string;
  kind: string;
  icon: React.ComponentType<{ size?: number }>;
  run: () => void;
}

function CommandPalette({
  snapshot,
  onClose,
  setRoute,
  openIssue,
}: {
  snapshot: Snapshot;
  onClose: () => void;
  setRoute: (r: Route) => void;
  openIssue: (id: string) => void;
}) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);

  const navCmds: Cmd[] = [
    { id: "go-dashboard", label: "Go to Dashboard", kind: "Navigate", icon: IconDashboard, run: () => setRoute({ name: "dashboard" }) },
    { id: "go-agents",    label: "Go to Agents",    kind: "Navigate", icon: IconAgents,    run: () => setRoute({ name: "agents" }) },
    { id: "go-workflows", label: "Go to Workflows", kind: "Navigate", icon: IconWorkflow,  run: () => setRoute({ name: "workflows" }) },
    { id: "go-workers",   label: "Go to Workers",   kind: "Navigate", icon: IconWorkers,   run: () => setRoute({ name: "workers" }) },
    { id: "go-activity",  label: "Go to Activity",  kind: "Navigate", icon: IconActivity,  run: () => setRoute({ name: "activity" }) },
    { id: "pause-all",    label: snapshot.paused ? "Resume orchestrator" : "Pause orchestrator", kind: "Action", icon: snapshot.paused ? IconPlay : IconPause, run: () => { void fetch(`/api/control/${snapshot.paused ? "resume" : "pause"}`, { method: "POST" }); } },
    { id: "sync",         label: "Sync issues from tracker",  kind: "Action", icon: IconRetry, run: () => {} },
  ];

  const seen = new Set<string>();
  const issueCmds: Cmd[] = [];
  for (const r of snapshot.running) {
    if (seen.has(r.issue.id)) continue;
    seen.add(r.issue.id);
    issueCmds.push({
      id: "iss-" + r.issue.id,
      label: `${r.issue.identifier} · ${r.issue.title}`,
      kind: "Issue",
      icon: IconBranch,
      run: () => openIssue(r.issue.id),
    });
  }
  for (const col of snapshot.kanban?.columns ?? []) {
    for (const i of col.issues) {
      if (seen.has(i.id)) continue;
      seen.add(i.id);
      issueCmds.push({
        id: "iss-" + i.id,
        label: `${i.identifier} · ${i.title}`,
        kind: "Issue",
        icon: IconBranch,
        run: () => openIssue(i.id),
      });
    }
  }

  const all = [...navCmds, ...issueCmds];
  const filtered = q
    ? all.filter((c) => c.label.toLowerCase().includes(q.toLowerCase()))
    : all.slice(0, 12);

  useEffect(() => {
    setSel(0);
  }, [q]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(s + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(0, s - 1));
    } else if (e.key === "Enter") {
      filtered[sel]?.run();
      onClose();
    }
  };

  return (
    <div
      onClick={onClose}
      className="fixed inset-0 z-50 flex items-start justify-center"
      style={{ background: "rgba(0,0,0,0.55)", backdropFilter: "blur(2px)", paddingTop: "14vh" }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="bg-panel border border-borderL rounded-lg overflow-hidden"
        style={{ width: 580, maxWidth: "92vw", boxShadow: "0 24px 64px rgba(0,0,0,0.5)" }}
      >
        <div className="flex items-center gap-2.5 px-4 py-3.5 border-b border-borderS">
          <span className="text-textMute">
            <IconSearch size={16} />
          </span>
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Type a command, search issues, or jump to a screen…"
            className="flex-1 bg-transparent border-0 outline-0 text-text text-[14px]"
          />
          <Kbd>esc</Kbd>
        </div>
        <div className="overflow-auto p-1.5" style={{ maxHeight: 360 }}>
          {filtered.length === 0 ? (
            <div className="py-8 text-center text-textMute text-[12.5px]">No matches.</div>
          ) : (
            filtered.map((c, i) => {
              const Icon = c.icon;
              return (
                <button
                  key={c.id}
                  onClick={() => {
                    c.run();
                    onClose();
                  }}
                  onMouseEnter={() => setSel(i)}
                  className="w-full flex items-center gap-3 rounded-md text-left cursor-pointer"
                  style={{
                    padding: "8px 10px",
                    background: i === sel ? "#1c1c1c" : "transparent",
                    color: "#ededed",
                    border: 0,
                    fontSize: 13,
                  }}
                >
                  <span className="text-textDim inline-flex">
                    <Icon size={14} />
                  </span>
                  <span className="flex-1 truncate">{c.label}</span>
                  <span className="font-mono text-[10.5px] text-textMute uppercase" style={{ letterSpacing: 0.4 }}>
                    {c.kind}
                  </span>
                </button>
              );
            })
          )}
        </div>
        <div className="flex gap-3.5 px-4 py-2 border-t border-borderS text-[11px] text-textMute font-mono">
          <span>
            <Kbd>↑</Kbd> <Kbd>↓</Kbd> navigate
          </span>
          <span>
            <Kbd>↵</Kbd> select
          </span>
          <span className="ml-auto">{filtered.length} results</span>
        </div>
      </div>
    </div>
  );
}

// Re-export for any deep references; the new design supersedes the old shell.
export { fmtElapsed };

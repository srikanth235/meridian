import { useEffect, useMemo, useState, useRef } from "react";
import { useSnapshot } from "./useSnapshot";
import type { HarnessId, Issue, Snapshot } from "./types";
import { Kbd } from "./atoms";
import {
  IconActivity,
  IconBranch,
  IconChevronDown,
  IconInbox,
  IconLive,
  IconLogo,
  IconMoon,
  IconPause,
  IconPlay,
  IconRepos,
  IconRetry,
  IconSearch,
  IconSettings,
  IconSun,
  IconTasks,
  IconTeam,
  IconWorkflow,
} from "./icons";
import { Tasks } from "./screens/Tasks";
import { Inbox } from "./screens/Inbox";
import { Live } from "./screens/Live";
import { Harnesses } from "./screens/Harnesses";
import { Repos } from "./screens/Repos";
import { Routing } from "./screens/Routing";
import { ActivityPanel } from "./screens/ActivityPanel";
import { IssueDetail } from "./screens/IssueDetail";
import { inboxIssues } from "./symphonyMap";

type Route =
  | { name: "inbox" }
  | { name: "tasks" }
  | { name: "live" }
  | { name: "harnesses" }
  | { name: "repos" }
  | { name: "routing" }
  | { name: "issue"; id: string };

type ConnState = "connecting" | "open" | "closed";
type Theme = "dark" | "light";

const THEME_KEY = "meridian.theme";
const REPO_FILTER_KEY = "meridian.repoFilter";
const OVERRIDES_KEY = "meridian.overrides";

interface IssueOverride {
  assignee?: HarnessId | null;
  triaged?: boolean;
  ignored?: boolean;
}

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "light") root.classList.add("theme-light");
  else root.classList.remove("theme-light");
}

function loadJson<T>(key: string, fallback: T): T {
  if (typeof localStorage === "undefined") return fallback;
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function saveJson<T>(key: string, val: T) {
  try { localStorage.setItem(key, JSON.stringify(val)); } catch { /* ignore */ }
}

export function App() {
  const { snapshot: rawSnapshot, conn } = useSnapshot();
  const [route, setRoute] = useState<Route>({ name: "inbox" });
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [activityOpen, setActivityOpen] = useState(false);
  const [theme, setTheme] = useState<Theme>(() => {
    const stored = typeof localStorage !== "undefined" ? localStorage.getItem(THEME_KEY) : null;
    return stored === "light" ? "light" : "dark";
  });
  const [repoFilter, setRepoFilter] = useState<string | null>(() =>
    loadJson<string | null>(REPO_FILTER_KEY, null),
  );
  const [overrides, setOverrides] = useState<Record<string, IssueOverride>>(() =>
    loadJson(OVERRIDES_KEY, {} as Record<string, IssueOverride>),
  );
  // Per-page search query. Keyed by page id so switching tabs doesn't lose
  // the current filter; not persisted across sessions (Linear-style: search
  // is ephemeral).
  const [search, setSearch] = useState<Record<string, string>>({});
  const setPageQuery = (page: string, q: string) =>
    setSearch((prev) => ({ ...prev, [page]: q }));

  useEffect(() => { applyTheme(theme); try { localStorage.setItem(THEME_KEY, theme); } catch {} }, [theme]);
  useEffect(() => { saveJson(REPO_FILTER_KEY, repoFilter); }, [repoFilter]);
  useEffect(() => { saveJson(OVERRIDES_KEY, overrides); }, [overrides]);

  const toggleTheme = () => setTheme((t) => (t === "dark" ? "light" : "dark"));

  // Re-render every second so relative timestamps and elapsed counters tick.
  const [, force] = useState(0);
  useEffect(() => {
    const id = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  // ⌘K palette
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

  // Apply local overrides on top of the live snapshot. The orchestrator doesn't
  // yet persist assignment / triage state, so the UI tracks it client-side.
  const snapshot = useMemo<Snapshot | null>(() => {
    if (!rawSnapshot) return null;
    return mergeOverrides(rawSnapshot, overrides);
  }, [rawSnapshot, overrides]);

  const selectedIssue = useMemo<Issue | null>(() => {
    if (route.name !== "issue" || !snapshot) return null;
    return findIssue(snapshot, route.id);
  }, [route, snapshot]);

  const openIssue = (id: string) => setRoute({ name: "issue", id });

  const assignTask = (issueId: string, harness: HarnessId | null) => {
    setOverrides((prev) => ({
      ...prev,
      [issueId]: {
        ...(prev[issueId] ?? {}),
        assignee: harness,
        // Assigning marks the item triaged → leaves Inbox.
        triaged: harness !== null ? true : (prev[issueId]?.triaged ?? false),
      },
    }));
  };

  const ignoreTask = (issueId: string) => {
    setOverrides((prev) => ({
      ...prev,
      [issueId]: { ...(prev[issueId] ?? {}), ignored: true, triaged: true },
    }));
  };

  const setHarnessConcurrency = async (id: HarnessId, n: number) => {
    try {
      const res = await fetch("/api/harnesses/concurrency", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id, concurrency: n }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
    } catch (e) {
      console.error("set concurrency failed", e);
    }
  };

  return (
    <div className="w-screen h-screen flex flex-col bg-bg text-text overflow-hidden" style={{ fontSize: 13 }}>
      <Titlebar
        conn={conn}
        snapshot={snapshot}
        theme={theme}
        onToggleTheme={toggleTheme}
        repoFilter={repoFilter}
        setRepoFilter={setRepoFilter}
        onOpenActivity={() => setActivityOpen(true)}
      />
      <div className="flex-1 flex min-h-0">
        <Sidebar
          snapshot={snapshot}
          route={route}
          setRoute={setRoute}
          onPalette={() => setPaletteOpen(true)}
          onOpenActivity={() => setActivityOpen(true)}
        />
        <main className="flex-1 overflow-auto min-w-0 bg-bg">
          {!snapshot ? (
            <div className="h-full flex items-center justify-center text-textMute text-[13px]">
              connecting…
            </div>
          ) : route.name === "inbox" ? (
            <Inbox
              snapshot={snapshot}
              density="comfortable"
              repoFilter={repoFilter}
              query={search.inbox ?? ""}
              onQueryChange={(q) => setPageQuery("inbox", q)}
              onOpenIssue={openIssue}
              onAssign={assignTask}
              onIgnore={ignoreTask}
            />
          ) : route.name === "tasks" ? (
            <Tasks
              snapshot={snapshot}
              density="comfortable"
              onOpenIssue={openIssue}
              repoFilter={repoFilter}
              query={search.tasks ?? ""}
              onQueryChange={(q) => setPageQuery("tasks", q)}
              onAssign={assignTask}
            />
          ) : route.name === "live" ? (
            <Live
              snapshot={snapshot}
              density="comfortable"
              onOpenIssue={openIssue}
              repoFilter={repoFilter}
              query={search.live ?? ""}
              onQueryChange={(q) => setPageQuery("live", q)}
            />
          ) : route.name === "harnesses" ? (
            <Harnesses
              snapshot={snapshot}
              density="comfortable"
              query={search.harnesses ?? ""}
              onQueryChange={(q) => setPageQuery("harnesses", q)}
              onSetConcurrency={setHarnessConcurrency}
            />
          ) : route.name === "repos" ? (
            <Repos
              snapshot={snapshot}
              density="comfortable"
              repoFilter={repoFilter}
              query={search.repos ?? ""}
              onQueryChange={(q) => setPageQuery("repos", q)}
              onSetRepoFilter={setRepoFilter}
            />
          ) : route.name === "routing" ? (
            <Routing snapshot={snapshot} density="comfortable" />
          ) : route.name === "issue" && selectedIssue ? (
            <IssueDetail
              snapshot={snapshot}
              issue={selectedIssue}
              onBack={() => setRoute({ name: "tasks" })}
              onAssign={assignTask}
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

      {activityOpen && snapshot && (
        <ActivityPanel snapshot={snapshot} onClose={() => setActivityOpen(false)} />
      )}
    </div>
  );
}

function mergeOverrides(
  snap: Snapshot,
  overrides: Record<string, IssueOverride>,
): Snapshot {
  const apply = (i: Issue): Issue => {
    const o = overrides[i.id];
    if (!o) return i;
    return {
      ...i,
      assignee: o.assignee !== undefined ? o.assignee : i.assignee,
      triaged: o.triaged !== undefined ? o.triaged : i.triaged,
    };
  };
  // Drop ignored items entirely from any list — they shouldn't appear anywhere.
  const isIgnored = (id: string) => overrides[id]?.ignored === true;

  return {
    ...snap,
    running: snap.running.filter((r) => !isIgnored(r.issue.id)).map((r) => ({ ...r, issue: apply(r.issue) })),
    kanban: {
      ...snap.kanban,
      columns: (snap.kanban?.columns ?? []).map((c) => ({
        ...c,
        issues: c.issues.filter((i) => !isIgnored(i.id)).map(apply),
      })),
      unsorted: (snap.kanban?.unsorted ?? []).filter((i) => !isIgnored(i.id)).map(apply),
    },
    inbox: snap.inbox
      ? snap.inbox
          .filter((i) => !isIgnored(i.id))
          .map(apply)
          // An inbox item with overridden triaged=true leaves the inbox.
          .filter((i) => i.triaged === false)
      : undefined,
  };
}

function findIssue(snapshot: Snapshot, id: string): Issue | null {
  for (const r of snapshot.running) if (r.issue.id === id) return r.issue;
  for (const col of snapshot.kanban?.columns ?? []) {
    const f = col.issues.find((i) => i.id === id);
    if (f) return f;
  }
  for (const i of snapshot.kanban?.unsorted ?? []) if (i.id === id) return i;
  for (const i of snapshot.inbox ?? []) if (i.id === id) return i;
  return null;
}

/* ─────────────────────────  Titlebar  ───────────────────────── */

function Titlebar({
  conn,
  snapshot,
  theme,
  onToggleTheme,
  repoFilter,
  setRepoFilter,
  onOpenActivity,
}: {
  conn: ConnState;
  snapshot: Snapshot | null;
  theme: Theme;
  onToggleTheme: () => void;
  repoFilter: string | null;
  setRepoFilter: (s: string | null) => void;
  onOpenActivity: () => void;
}) {
  return (
    <header className="shrink-0 app-drag bg-chrome border-b border-border" style={{ height: 38 }}>
      <div className="pl-24 pr-3.5 h-full flex items-center gap-3">
        <RepoPicker snapshot={snapshot} repoFilter={repoFilter} setRepoFilter={setRepoFilter} />
        <div className="flex-1" />
        <button
          onClick={onOpenActivity}
          title="Activity"
          className="inline-flex items-center justify-center h-6 w-6 rounded-md text-textDim hover:text-text border border-border cursor-pointer"
          style={{ background: "transparent" }}
        >
          <IconActivity size={12} />
        </button>
        <ThemeToggle theme={theme} onToggle={onToggleTheme} />
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

function RepoPicker({
  snapshot,
  repoFilter,
  setRepoFilter,
}: {
  snapshot: Snapshot | null;
  repoFilter: string | null;
  setRepoFilter: (s: string | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!ref.current) return;
      if (!ref.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", onDoc);
    return () => window.removeEventListener("mousedown", onDoc);
  }, [open]);

  const repos = snapshot?.repos ?? [];
  const label = repoFilter ?? (repos.length === 0 ? "—" : "All repos");

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        className="inline-flex items-center gap-1.5 cursor-pointer rounded-md text-textDim hover:text-text"
        style={{ height: 24, padding: "0 8px", background: "transparent", border: "1px solid var(--border)", fontSize: 11.5 }}
      >
        <span className="font-mono">{label}</span>
        <IconChevronDown size={11} />
      </button>
      {open && (
        <div
          className="absolute z-30 left-0 top-[calc(100%+4px)] bg-panel border border-borderL rounded-md overflow-hidden"
          style={{ width: 240, boxShadow: "var(--shadow)" }}
        >
          <button
            onClick={() => { setRepoFilter(null); setOpen(false); }}
            className="w-full text-left cursor-pointer"
            style={{
              padding: "7px 10px",
              background: repoFilter === null ? "var(--panel3)" : "transparent",
              border: 0, color: "var(--text)", fontSize: 12,
            }}
          >
            All repos
          </button>
          <div style={{ borderTop: "1px solid var(--borderS)" }} />
          {repos.map((r) => (
            <button
              key={r}
              onClick={() => { setRepoFilter(r); setOpen(false); }}
              className="w-full text-left cursor-pointer"
              style={{
                padding: "7px 10px",
                background: repoFilter === r ? "var(--panel3)" : "transparent",
                border: 0, color: "var(--text)", fontSize: 12,
                fontFamily: "var(--font-mono, ui-monospace, monospace)",
              }}
            >
              {r}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function ThemeToggle({ theme, onToggle }: { theme: Theme; onToggle: () => void }) {
  const isDark = theme === "dark";
  return (
    <button
      onClick={onToggle}
      title={isDark ? "Switch to light mode" : "Switch to dark mode"}
      aria-label="Toggle theme"
      className="inline-flex items-center justify-center h-6 w-6 rounded-md text-textDim hover:text-text border border-border cursor-pointer"
      style={{ background: "transparent" }}
    >
      {isDark ? <IconSun size={12} /> : <IconMoon size={12} />}
    </button>
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
        paused ? "text-amber border-amber/30" : "text-accent border-accent/30"
      }`}
      style={{ background: paused ? "rgba(245,158,11,0.08)" : "rgba(16,185,129,0.08)" }}
    >
      {busy ? "…" : paused ? <><IconPlay size={10} /> resume</> : <><IconPause size={10} /> pause</>}
    </button>
  );
}

function ConnPill({ state }: { state: ConnState }) {
  const cls = state === "open" ? "text-accent" : state === "connecting" ? "text-amber" : "text-red";
  return (
    <span className={`inline-flex items-center gap-1.5 font-mono text-[11px] ${cls}`}>
      <span
        className="w-1.5 h-1.5 rounded-full"
        style={{ background: state === "open" ? "#10b981" : state === "connecting" ? "#f59e0b" : "#ef4444" }}
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
  onOpenActivity,
}: {
  snapshot: Snapshot | null;
  route: Route;
  setRoute: (r: Route) => void;
  onPalette: () => void;
  onOpenActivity: () => void;
}) {
  const inboxCount = snapshot ? inboxIssues(snapshot).length : 0;
  const liveCount = snapshot?.running.length ?? 0;
  const harnesses = snapshot?.harnesses ?? [];
  // Sidebar badge shows the count of *connected* repos, not all available.
  const connectedCount = snapshot?.available_repos?.filter((r) => r.connected).length ?? null;

  type NavItem = {
    id: Route["name"];
    label: string;
    icon: React.ComponentType<{ size?: number }>;
    badge?: number | null;
    matchAlso?: Route["name"][];
  };

  const work: NavItem[] = [
    { id: "inbox", label: "Inbox", icon: IconInbox, badge: inboxCount > 0 ? inboxCount : null },
    { id: "tasks", label: "Tasks", icon: IconTasks, matchAlso: ["issue"] },
    { id: "live", label: "Live", icon: IconLive, badge: liveCount > 0 ? liveCount : null },
  ];
  const team: NavItem[] = [
    { id: "harnesses", label: "Harnesses", icon: IconTeam, badge: harnesses.length || null },
  ];
  const sources: NavItem[] = [
    { id: "repos", label: "Repos", icon: IconRepos, badge: connectedCount ?? null },
    { id: "routing", label: "Routing", icon: IconWorkflow },
  ];

  const matches = (item: NavItem) =>
    route.name === item.id || (item.matchAlso?.includes(route.name) ?? false);

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
            Meridian
          </div>
          <div className="text-[10px] text-textMute font-mono mt-0.5">
            v0.5.0 · main
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
        <SectionLabel>Work</SectionLabel>
        {work.map((item) => (
          <NavRow key={item.id} item={item} active={matches(item)} onClick={() => setRoute({ name: item.id } as Route)} />
        ))}

        <SectionLabel className="mt-3.5">Team</SectionLabel>
        {team.map((item) => (
          <NavRow key={item.id} item={item} active={matches(item)} onClick={() => setRoute({ name: item.id } as Route)} />
        ))}

        <SectionLabel className="mt-3.5">Sources</SectionLabel>
        {sources.map((item) => (
          <NavRow key={item.id} item={item} active={matches(item)} onClick={() => setRoute({ name: item.id } as Route)} />
        ))}

        {snapshot && snapshot.running.length > 0 && (
          <>
            <SectionLabel className="mt-3.5">Pinned</SectionLabel>
            {snapshot.running.slice(0, 3).map((r) => (
              <button
                key={r.issue.id}
                onClick={() => setRoute({ name: "issue", id: r.issue.id })}
                className={`sym-nav flex items-center gap-2 rounded-md text-left cursor-pointer ${
                  route.name === "issue" && (route as { id?: string }).id === r.issue.id
                    ? "text-text"
                    : "text-textDim"
                }`}
                style={{
                  height: 26, padding: "0 10px", background: "transparent", border: 0, fontSize: 11.5,
                }}
              >
                <span className="w-1.5 h-1.5 rounded-full bg-accent shrink-0" />
                <span className="font-mono text-[10.5px] text-textMute shrink-0">{r.issue.identifier}</span>
                <span className="flex-1 truncate">{r.issue.title}</span>
              </button>
            ))}
          </>
        )}
      </div>

      {/* Footer — activity + user */}
      <div
        className="flex items-center gap-2 mt-2 pt-2.5"
        style={{ padding: "10px 8px", borderTop: "1px solid var(--border)" }}
      >
        <button
          onClick={onOpenActivity}
          title="Activity"
          className="inline-flex items-center justify-center cursor-pointer text-textMute hover:text-textDim"
          style={{ width: 26, height: 26, borderRadius: 6, background: "transparent", border: "1px solid var(--border)" }}
        >
          <IconActivity size={12} />
        </button>
        <div
          className="rounded-full text-white text-[11px] font-semibold flex items-center justify-center"
          style={{ width: 26, height: 26, background: "linear-gradient(135deg, #10b981, #3b82f6)" }}
        >
          S
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-[12px] font-medium text-text">Srikanth</div>
          <div className="text-[10.5px] text-textMute font-mono">@srikanth</div>
        </div>
        <button className="bg-transparent border-0 text-textMute hover:text-textDim cursor-pointer p-1">
          <IconSettings size={14} />
        </button>
      </div>
    </aside>
  );
}

function NavRow({
  item,
  active,
  onClick,
}: {
  item: { id: string; label: string; icon: React.ComponentType<{ size?: number }>; badge?: number | null };
  active: boolean;
  onClick: () => void;
}) {
  const Icon = item.icon;
  return (
    <button
      onClick={onClick}
      className={`sym-nav flex items-center gap-2.5 rounded-md text-left cursor-pointer relative ${
        active ? "text-text font-medium" : "text-textDim"
      }`}
      style={{
        height: 30,
        padding: "0 10px",
        background: active ? "var(--panel3)" : "transparent",
        border: 0,
        fontSize: 12.5,
      }}
    >
      {active && (
        <span className="absolute bg-accent rounded-sm" style={{ left: 0, top: 7, bottom: 7, width: 2 }} />
      )}
      <Icon size={14} />
      <span className="flex-1">{item.label}</span>
      {item.badge != null && item.badge > 0 && (
        <span
          className="font-mono text-[10.5px] tabular-nums px-1.5 rounded-sm"
          style={{
            background: active ? "var(--panel)" : "var(--panel3)",
            color: active ? "var(--text)" : "var(--textMute)",
          }}
        >
          {item.badge}
        </span>
      )}
    </button>
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
    { id: "go-inbox",     label: "Go to Inbox",     kind: "Navigate", icon: IconInbox,    run: () => setRoute({ name: "inbox" }) },
    { id: "go-tasks",     label: "Go to Tasks",     kind: "Navigate", icon: IconTasks,    run: () => setRoute({ name: "tasks" }) },
    { id: "go-live",      label: "Go to Live",      kind: "Navigate", icon: IconLive,     run: () => setRoute({ name: "live" }) },
    { id: "go-harnesses", label: "Go to Harnesses", kind: "Navigate", icon: IconTeam,     run: () => setRoute({ name: "harnesses" }) },
    { id: "go-repos",     label: "Go to Repos",     kind: "Navigate", icon: IconRepos,    run: () => setRoute({ name: "repos" }) },
    { id: "go-routing",   label: "Go to Routing",   kind: "Navigate", icon: IconWorkflow, run: () => setRoute({ name: "routing" }) },
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
  for (const i of snapshot.inbox ?? []) {
    if (seen.has(i.id)) continue;
    seen.add(i.id);
    issueCmds.push({
      id: "iss-" + i.id,
      label: `${i.identifier} · ${i.title}`,
      kind: "Inbox",
      icon: IconInbox,
      run: () => openIssue(i.id),
    });
  }

  // Repos: jump to the Repos page scoped to the picked repo. Connected ones
  // first so they're easier to reach when typing.
  const repoCmds: Cmd[] = [];
  const sortedRepos = [...(snapshot.available_repos ?? [])].sort(
    (a, b) => Number(b.connected) - Number(a.connected),
  );
  for (const r of sortedRepos) {
    repoCmds.push({
      id: "repo-" + r.slug,
      label: r.slug + (r.connected ? "" : "  · not connected"),
      kind: "Repo",
      icon: IconRepos,
      run: () => setRoute({ name: "repos" }),
    });
  }

  // Harnesses: jump to the Harnesses page; useful for tweaking concurrency.
  const harnessCmds: Cmd[] = [];
  for (const h of snapshot.harnesses ?? []) {
    harnessCmds.push({
      id: "harness-" + h.id,
      label: `${h.name}${h.version ? ` · v${h.version}` : ""}${h.available ? "" : " · offline"}`,
      kind: "Harness",
      icon: IconTeam,
      run: () => setRoute({ name: "harnesses" }),
    });
  }

  const all = [...navCmds, ...issueCmds, ...repoCmds, ...harnessCmds];
  const filtered = q ? all.filter((c) => c.label.toLowerCase().includes(q.toLowerCase())) : all.slice(0, 12);

  useEffect(() => { setSel(0); }, [q]);

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
      style={{ background: "var(--overlay)", backdropFilter: "blur(2px)", paddingTop: "14vh" }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="bg-panel border border-borderL rounded-lg overflow-hidden"
        style={{ width: 580, maxWidth: "92vw", boxShadow: "var(--shadow)" }}
      >
        <div className="flex items-center gap-2.5 px-4 py-3.5 border-b border-borderS">
          <span className="text-textMute"><IconSearch size={16} /></span>
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
                  onClick={() => { c.run(); onClose(); }}
                  onMouseEnter={() => setSel(i)}
                  className="w-full flex items-center gap-3 rounded-md text-left cursor-pointer"
                  style={{
                    padding: "8px 10px",
                    background: i === sel ? "var(--panel3)" : "transparent",
                    color: "var(--text)",
                    border: 0,
                    fontSize: 13,
                  }}
                >
                  <span className="text-textDim inline-flex"><Icon size={14} /></span>
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
          <span><Kbd>↑</Kbd> <Kbd>↓</Kbd> navigate</span>
          <span><Kbd>↵</Kbd> select</span>
          <span className="ml-auto">{filtered.length} results</span>
        </div>
      </div>
    </div>
  );
}

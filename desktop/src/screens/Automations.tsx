import { useEffect, useMemo, useState } from "react";
import type {
  Automation,
  AutomationRun,
  AutomationsRuntime,
  InboxEntry,
} from "../types";
import { Card, PageSearch, Pill } from "../atoms";
import {
  IconAutomation,
  IconPlay,
  IconCheck,
  IconX,
  IconRetry,
  IconPullRequest,
  IconInbox,
  IconRepos,
  IconBranch,
} from "../icons";

interface Preset {
  id: string;
  icon: React.ReactNode;
  title: string;
  hint: string;
  prompt: string;
}

const GITHUB_PRESETS: Preset[] = [
  {
    id: "assigned-issues-morning",
    icon: <IconInbox size={13} />,
    title: "Morning issue digest",
    hint: "Daily at 9am",
    prompt:
      "Every morning at 9am, pull all open issues assigned to me from my repos into the inbox, one entry per issue, deduped by issue URL.",
  },
  {
    id: "review-requested-prs",
    icon: <IconPullRequest size={13} />,
    title: "PRs awaiting my review",
    hint: "Every 30 min",
    prompt:
      "Every 30 minutes, find open PRs across my repos where I'm a requested reviewer and surface them as inbox entries deduped by PR URL.",
  },
  {
    id: "new-prs-tabs",
    icon: <IconBranch size={13} />,
    title: "New PRs as tabs",
    hint: "Every hour",
    prompt:
      "Every hour, find PRs opened or updated in my repos since the last run and open each as a tab deduped by PR URL.",
  },
  {
    id: "urgent-bug-alerts",
    icon: <IconRepos size={13} />,
    title: "Urgent bug alerts",
    hint: "Every 15 min",
    prompt:
      "Every 15 minutes, scan my repos for open issues labeled 'urgent' or 'sev1' and create an inbox entry for each, deduped by issue URL.",
  },
  {
    id: "stale-prs",
    icon: <IconPullRequest size={13} />,
    title: "Stale PR sweep",
    hint: "Daily at 8am",
    prompt:
      "Every weekday at 8am, list open PRs in my repos that have not been updated in 7+ days and add them to the inbox tagged 'stale'.",
  },
  {
    id: "shipped-this-day",
    icon: <IconCheck size={13} />,
    title: "Shipped today",
    hint: "Every 2 hours",
    prompt:
      "Every 2 hours, find PRs in my repos merged since the last run and add an inbox entry per PR tagged 'shipped', deduped by PR URL.",
  },
  {
    id: "mentions-watch",
    icon: <IconInbox size={13} />,
    title: "Mentions watcher",
    hint: "Every hour",
    prompt:
      "Every hour, find issues and PRs in my repos that mention me and were updated since the last run, and add each to the inbox deduped by URL.",
  },
  {
    id: "new-issues-triage",
    icon: <IconRepos size={13} />,
    title: "Triage queue",
    hint: "Every hour",
    prompt:
      "Every hour, find newly opened issues in my repos that have no labels yet and surface them in the inbox tagged 'triage', deduped by issue URL.",
  },
];

interface Props {
  density: "comfortable" | "dense";
  query: string;
  onQueryChange: (q: string) => void;
}

export function Automations({ density, query, onQueryChange }: Props) {
  const pad = density === "dense" ? 14 : 20;
  const [automations, setAutomations] = useState<Automation[] | null>(null);
  const [inbox, setInbox] = useState<InboxEntry[]>([]);
  const [runtime, setRuntime] = useState<AutomationsRuntime | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [nl, setNl] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [submitMsg, setSubmitMsg] = useState<string | null>(null);

  async function refresh() {
    try {
      const [a, i, rt] = await Promise.all([
        fetch("/api/automations").then((r) => r.json()),
        fetch("/api/inbox").then((r) => r.json()),
        fetch("/api/automations/runtime").then((r) => r.json()),
      ]);
      setAutomations(a.automations ?? []);
      setInbox(i.entries ?? []);
      setRuntime(rt.runtime ?? null);
    } catch {
      /* offline */
    }
  }

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 4000);
    return () => clearInterval(id);
  }, []);

  const filtered = useMemo(() => {
    const list = automations ?? [];
    const q = query.trim().toLowerCase();
    if (!q) return list;
    return list.filter(
      (a) => a.name.toLowerCase().includes(q) || a.id.toLowerCase().includes(q),
    );
  }, [automations, query]);

  const requests = useMemo(
    () => inbox.filter((e) => e.kind === "automation-request"),
    [inbox],
  );
  const errors = useMemo(
    () => inbox.filter((e) => e.kind === "automation-error"),
    [inbox],
  );

  const selected = useMemo(
    () => filtered.find((a) => a.id === selectedId) ?? null,
    [filtered, selectedId],
  );

  async function submitRequest() {
    if (!nl.trim() || submitting) return;
    setSubmitting(true);
    setSubmitMsg(null);
    try {
      const res = await fetch("/api/automations/request", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ nl }),
      });
      if (!res.ok) {
        const j = await res.json().catch(() => ({}));
        setSubmitMsg(`error: ${j.error ?? res.statusText}`);
      } else {
        setNl("");
        setSubmitMsg("Spec drafted into your inbox.");
        refresh();
      }
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <div className="flex items-end justify-between gap-3">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">
            Workspace
          </div>
          <h1
            className="text-[22px] font-semibold text-text leading-tight m-0"
            style={{ letterSpacing: -0.3 }}
          >
            Automations
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {automations === null
              ? "loading…"
              : `${filtered.length} script${filtered.length === 1 ? "" : "s"}`}
            {requests.length > 0 && (
              <>
                {" · "}
                <span className="text-amber">{requests.length} pending request{requests.length === 1 ? "" : "s"}</span>
              </>
            )}
            {errors.length > 0 && (
              <>
                {" · "}
                <span className="text-red-400">{errors.length} recent error{errors.length === 1 ? "" : "s"}</span>
              </>
            )}
          </div>
        </div>
        <PageSearch
          pageId="automations"
          value={query}
          onChange={onQueryChange}
          placeholder="Search automations…"
        />
      </div>

      <RuntimeBanner runtime={runtime} />

      <NlComposer
        value={nl}
        onChange={setNl}
        onSubmit={submitRequest}
        submitting={submitting}
        message={submitMsg}
      />

      <PresetsCard
        onPick={(prompt) => {
          setNl(prompt);
          setSubmitMsg(null);
          if (typeof window !== "undefined") {
            window.scrollTo({ top: 0, behavior: "smooth" });
          }
        }}
      />

      {requests.length > 0 && (
        <Card>
          <SectionHeader
            title="Pending requests"
            sub="Open the inbox entry in your coding harness to write the script."
          />
          {requests.map((e, idx) => (
            <RequestRow
              key={e.id}
              entry={e}
              first={idx === 0}
              onDismiss={async () => {
                await fetch(`/api/inbox/${e.id}/dismiss`, { method: "POST" });
                refresh();
              }}
            />
          ))}
        </Card>
      )}

      <Card>
        <SectionHeader
          title="Scripts"
          sub="Files in automations/. Edit the file → behavior changes."
        />
        {filtered.length === 0 ? (
          <div className="px-4 py-12 text-center text-textMute text-[12px] flex flex-col items-center gap-3">
            <span style={{ color: "var(--textMute)" }}>
              <IconAutomation size={20} />
            </span>
            <div>
              No automations yet. Type a request above and your harness will
              draft one.
            </div>
          </div>
        ) : (
          filtered.map((a, idx) => (
            <AutomationRow
              key={a.id}
              automation={a}
              first={idx === 0}
              expanded={selectedId === a.id}
              onClick={() =>
                setSelectedId((prev) => (prev === a.id ? null : a.id))
              }
              onChanged={refresh}
            />
          ))
        )}
      </Card>

      {errors.length > 0 && (
        <Card>
          <SectionHeader
            title="Recent failures"
            sub="Surfaced from the run log. Dismiss once acknowledged."
          />
          {errors.map((e, idx) => (
            <ErrorRow
              key={e.id}
              entry={e}
              first={idx === 0}
              onDismiss={async () => {
                await fetch(`/api/inbox/${e.id}/dismiss`, { method: "POST" });
                refresh();
              }}
            />
          ))}
        </Card>
      )}

      {selected && <AutomationDetail automation={selected} />}
    </div>
  );
}

function RuntimeBanner({ runtime }: { runtime: AutomationsRuntime | null }) {
  if (!runtime) return null;

  const tone: "ok" | "warn" | "err" = runtime.missing
    ? "err"
    : runtime.hint
      ? "warn"
      : "ok";
  const colors: Record<typeof tone, { bg: string; border: string; fg: string; dot: string }> = {
    ok: {
      bg: "rgba(16,185,129,0.06)",
      border: "rgba(16,185,129,0.3)",
      fg: "var(--text)",
      dot: "var(--ok, #10b981)",
    },
    warn: {
      bg: "rgba(245,158,11,0.06)",
      border: "rgba(245,158,11,0.35)",
      fg: "var(--text)",
      dot: "#f59e0b",
    },
    err: {
      bg: "rgba(239,68,68,0.07)",
      border: "rgba(239,68,68,0.4)",
      fg: "var(--text)",
      dot: "#ef4444",
    },
  };
  const c = colors[tone];

  let label: string;
  if (runtime.missing) {
    label = "No JS runtime detected";
  } else {
    const kindLabel =
      runtime.kind === "bun" ? "Bun" : runtime.kind === "node" ? "Node.js" : "Custom";
    label = `${kindLabel} ${runtime.version || ""}`.trim();
  }

  const tsLabel = runtime.missing
    ? "TypeScript: unavailable"
    : runtime.supports_ts
      ? "TypeScript: native"
      : "TypeScript: unavailable";

  return (
    <div
      className="flex items-center gap-3"
      style={{
        padding: "10px 14px",
        background: c.bg,
        border: `1px solid ${c.border}`,
        borderRadius: 8,
        color: c.fg,
        fontSize: 12,
      }}
    >
      <span
        className="w-1.5 h-1.5 rounded-full shrink-0"
        style={{ background: c.dot }}
      />
      <div className="flex-1 min-w-0">
        <div className="font-medium">{label}</div>
        <div className="text-textMute text-[11px] mt-0.5">
          {tsLabel}
          {runtime.command && (
            <>
              {" · "}
              <span className="font-mono">{runtime.command}</span>
            </>
          )}
          {runtime.source && runtime.source !== "missing" && (
            <>
              {" · source: "}
              <span className="font-mono">{runtime.source}</span>
            </>
          )}
        </div>
        {runtime.hint && (
          <div className="text-[11px] mt-1" style={{ color: c.dot }}>
            {runtime.hint}
          </div>
        )}
      </div>
    </div>
  );
}

function NlComposer({
  value,
  onChange,
  onSubmit,
  submitting,
  message,
}: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
  submitting: boolean;
  message: string | null;
}) {
  return (
    <Card>
      <div style={{ padding: 14 }}>
        <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-2">
          New automation
        </div>
        <textarea
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder="Describe what you want — e.g. &quot;every morning pull my assigned issues into the inbox&quot;"
          rows={3}
          className="w-full bg-panel2 border border-border rounded-md text-text text-[13px] outline-none focus:border-accent"
          style={{ padding: 10, resize: "vertical", fontFamily: "inherit" }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) onSubmit();
          }}
        />
        <div className="flex items-center justify-between mt-2 text-[11px] text-textMute">
          <span>
            {message
              ? message
              : "Drafts a self-contained spec into your inbox; your harness writes the file."}
          </span>
          <button
            onClick={onSubmit}
            disabled={submitting || !value.trim()}
            className="bg-accent text-white text-[12px] font-medium rounded-md cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
            style={{ padding: "6px 12px" }}
          >
            {submitting ? "Drafting…" : "Draft request"}
          </button>
        </div>
      </div>
    </Card>
  );
}

function PresetsCard({ onPick }: { onPick: (prompt: string) => void }) {
  return (
    <Card>
      <SectionHeader
        title="GitHub presets"
        sub="Click to load the prompt above. Edit if you want, then draft."
      />
      <div
        style={{
          padding: 12,
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))",
          gap: 8,
        }}
      >
        {GITHUB_PRESETS.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => onPick(p.prompt)}
            className="text-left bg-panel2 border border-border rounded-md cursor-pointer hover:border-accent"
            style={{ padding: 10 }}
          >
            <div className="flex items-center gap-2">
              <span style={{ color: "var(--textDim)" }}>{p.icon}</span>
              <span className="text-[12px] font-medium text-text truncate">
                {p.title}
              </span>
              <span className="ml-auto text-[10px] text-textMute shrink-0">
                {p.hint}
              </span>
            </div>
            <div
              className="text-[11px] text-textMute mt-1.5"
              style={{
                display: "-webkit-box",
                WebkitLineClamp: 2,
                WebkitBoxOrient: "vertical",
                overflow: "hidden",
              }}
            >
              {p.prompt}
            </div>
          </button>
        ))}
      </div>
    </Card>
  );
}

function SectionHeader({ title, sub }: { title: string; sub?: string }) {
  return (
    <div
      className="border-b border-borderS"
      style={{ padding: "10px 16px" }}
    >
      <div className="text-[12px] font-semibold text-text">{title}</div>
      {sub && <div className="text-[11px] text-textMute mt-0.5">{sub}</div>}
    </div>
  );
}

function AutomationRow({
  automation,
  first,
  expanded,
  onClick,
  onChanged,
}: {
  automation: Automation;
  first: boolean;
  expanded: boolean;
  onClick: () => void;
  onChanged: () => void;
}) {
  const schedule = useMemo(() => {
    try {
      const s = JSON.parse(automation.schedule_json);
      if ("cron" in s) return `cron ${s.cron}`;
      if ("every" in s) return `every ${s.every}`;
    } catch {
      /* fall through */
    }
    return "—";
  }, [automation.schedule_json]);

  const status = automation.parse_error
    ? "parse-error"
    : automation.running_since
      ? "running"
      : automation.last_error
        ? "error"
        : automation.enabled
          ? "ok"
          : "off";

  return (
    <div
      onClick={onClick}
      className="sym-row cursor-pointer flex items-center gap-3"
      style={{
        padding: "12px 16px",
        borderTop: first ? "none" : "1px solid var(--borderS)",
      }}
    >
      <StatusDot status={status} />
      <div className="flex-1 min-w-0">
        <div className="text-[13px] text-text font-medium truncate">
          {automation.name}
        </div>
        <div className="flex items-center gap-1.5 mt-1 flex-wrap">
          <Pill tone="muted">{schedule}</Pill>
          <span className="text-[11px] text-textDim font-mono">
            {automation.id}
          </span>
          {automation.last_run_at && (
            <span className="text-[11px] text-textMute">
              · last run {timeago(automation.last_run_at)}
            </span>
          )}
          {automation.next_run_at && automation.enabled && !automation.parse_error && (
            <span className="text-[11px] text-textMute">
              · next {timeago(automation.next_run_at)}
            </span>
          )}
        </div>
      </div>
      <div
        className="flex items-center gap-1.5 shrink-0"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          onClick={async () => {
            await fetch(`/api/automations/${automation.id}/run`, {
              method: "POST",
              headers: { "content-type": "application/json" },
              body: JSON.stringify({ dry_run: true }),
            });
            onChanged();
          }}
          title="Dry-run"
          className="text-[11px] font-medium text-textDim hover:text-text border border-border rounded-md cursor-pointer"
          style={{ padding: "3px 8px" }}
        >
          Dry
        </button>
        <button
          onClick={async () => {
            await fetch(`/api/automations/${automation.id}/run`, {
              method: "POST",
              headers: { "content-type": "application/json" },
              body: JSON.stringify({ dry_run: false }),
            });
            onChanged();
          }}
          title="Run now"
          className="text-[11px] font-medium text-textDim hover:text-text border border-border rounded-md cursor-pointer flex items-center gap-1"
          style={{ padding: "3px 8px" }}
        >
          <IconPlay size={10} /> Run
        </button>
        <button
          onClick={async () => {
            await fetch(`/api/automations/${automation.id}/enable`, {
              method: "POST",
              headers: { "content-type": "application/json" },
              body: JSON.stringify({ enabled: !automation.enabled }),
            });
            onChanged();
          }}
          title={automation.enabled ? "Disable" : "Enable"}
          className="text-[11px] font-medium border rounded-md cursor-pointer"
          style={{
            padding: "3px 8px",
            color: automation.enabled ? "var(--ok)" : "var(--textMute)",
            borderColor: automation.enabled
              ? "var(--okBorder, var(--border))"
              : "var(--border)",
          }}
        >
          {automation.enabled ? "On" : "Off"}
        </button>
        <span className="text-[10px] text-textMute mr-1">
          {expanded ? "▾" : "▸"}
        </span>
      </div>
    </div>
  );
}

function StatusDot({ status }: { status: string }) {
  const color =
    status === "running"
      ? "#3b82f6"
      : status === "ok"
        ? "var(--ok, #10b981)"
        : status === "error" || status === "parse-error"
          ? "#ef4444"
          : "#6b7280";
  return (
    <span
      className="w-1.5 h-1.5 rounded-full shrink-0"
      style={{ background: color }}
    />
  );
}

function AutomationDetail({ automation }: { automation: Automation }) {
  const [data, setData] = useState<{ source: string | null } | null>(null);
  const [runs, setRuns] = useState<AutomationRun[]>([]);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try {
        const [d, r] = await Promise.all([
          fetch(`/api/automations/${automation.id}`).then((r) => r.json()),
          fetch(`/api/automations/${automation.id}/runs`).then((r) => r.json()),
        ]);
        if (cancelled) return;
        setData({ source: d.source ?? null });
        setRuns(r.runs ?? []);
      } catch {
        /* offline */
      }
    };
    tick();
    const id = setInterval(tick, 3000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [automation.id]);

  return (
    <Card>
      <SectionHeader title={automation.name} sub={automation.file_path} />
      {automation.parse_error && (
        <div
          className="text-[12px]"
          style={{
            padding: "10px 16px",
            background: "var(--panel2)",
            color: "#ef4444",
            borderBottom: "1px solid var(--borderS)",
            fontFamily: "ui-monospace, monospace",
            whiteSpace: "pre-wrap",
          }}
        >
          parse error: {automation.parse_error}
        </div>
      )}
      <div
        style={{
          padding: 16,
          fontSize: 12,
          fontFamily: "ui-monospace, monospace",
          color: "var(--text)",
          background: "var(--panel2)",
          maxHeight: 360,
          overflow: "auto",
          whiteSpace: "pre",
          borderBottom: "1px solid var(--borderS)",
        }}
      >
        {data?.source ?? "(loading source…)"}
      </div>
      <SectionHeader title="Run history" />
      {runs.length === 0 ? (
        <div className="px-4 py-6 text-textMute text-[12px]">No runs yet.</div>
      ) : (
        runs.map((r, idx) => <RunRow key={r.id} run={r} first={idx === 0} />)
      )}
    </Card>
  );
}

function RunRow({ run, first }: { run: AutomationRun; first: boolean }) {
  const [open, setOpen] = useState(false);
  const icon =
    run.status === "succeeded" ? (
      <IconCheck size={12} />
    ) : run.status === "failed" ? (
      <IconX size={12} />
    ) : (
      <IconRetry size={12} />
    );
  const color =
    run.status === "succeeded"
      ? "var(--ok, #10b981)"
      : run.status === "failed"
        ? "#ef4444"
        : "#3b82f6";
  return (
    <div
      onClick={() => setOpen((o) => !o)}
      className="cursor-pointer"
      style={{
        padding: "10px 16px",
        borderTop: first ? "none" : "1px solid var(--borderS)",
      }}
    >
      <div className="flex items-center gap-2">
        <span style={{ color }}>{icon}</span>
        <span className="text-[12px] text-text">
          {run.dry_run ? "Dry-run" : "Run"} #{run.id}
        </span>
        <span className="text-[11px] text-textMute">
          {timeago(run.started_at)}
          {run.ended_at && ` · ${durationMs(run.started_at, run.ended_at)}`}
        </span>
        {run.error && (
          <span className="text-[11px] text-red-400 truncate">
            {run.error}
          </span>
        )}
        <span className="ml-auto text-[10px] text-textMute">
          {open ? "▾" : "▸"}
        </span>
      </div>
      {open && run.log && (
        <pre
          style={{
            marginTop: 8,
            padding: 10,
            background: "var(--panel2)",
            borderRadius: 6,
            fontSize: 11,
            color: "var(--textDim)",
            maxHeight: 240,
            overflow: "auto",
            whiteSpace: "pre-wrap",
          }}
        >
          {run.log}
        </pre>
      )}
    </div>
  );
}

function RequestRow({
  entry,
  first,
  onDismiss,
}: {
  entry: InboxEntry;
  first: boolean;
  onDismiss: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div
      style={{
        padding: "12px 16px",
        borderTop: first ? "none" : "1px solid var(--borderS)",
      }}
    >
      <div
        className="flex items-center gap-3 cursor-pointer"
        onClick={() => setOpen((o) => !o)}
      >
        <span className="w-1.5 h-1.5 rounded-full bg-amber shrink-0" />
        <div className="flex-1 min-w-0">
          <div className="text-[13px] text-text truncate">{entry.title}</div>
          <div className="text-[11px] text-textMute mt-0.5">
            {timeago(entry.created_at)} · {open ? "click to collapse" : "click to expand spec"}
          </div>
        </div>
        <button
          onClick={(e) => {
            e.stopPropagation();
            if (entry.body) navigator.clipboard?.writeText(entry.body);
          }}
          className="text-[11px] font-medium text-textDim hover:text-text border border-border rounded-md cursor-pointer"
          style={{ padding: "3px 8px" }}
        >
          Copy spec
        </button>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDismiss();
          }}
          className="text-[11px] text-textMute hover:text-text"
          style={{ padding: "3px 6px" }}
          title="Dismiss"
        >
          ×
        </button>
      </div>
      {open && entry.body && (
        <pre
          style={{
            marginTop: 10,
            padding: 12,
            background: "var(--panel2)",
            borderRadius: 6,
            fontSize: 11,
            color: "var(--text)",
            maxHeight: 320,
            overflow: "auto",
            whiteSpace: "pre-wrap",
          }}
        >
          {entry.body}
        </pre>
      )}
    </div>
  );
}

function ErrorRow({
  entry,
  first,
  onDismiss,
}: {
  entry: InboxEntry;
  first: boolean;
  onDismiss: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div
      style={{
        padding: "12px 16px",
        borderTop: first ? "none" : "1px solid var(--borderS)",
      }}
    >
      <div
        className="flex items-center gap-3 cursor-pointer"
        onClick={() => setOpen((o) => !o)}
      >
        <span
          className="w-1.5 h-1.5 rounded-full shrink-0"
          style={{ background: "#ef4444" }}
        />
        <div className="flex-1 min-w-0">
          <div className="text-[13px] text-text truncate">{entry.title}</div>
          <div className="text-[11px] text-textMute mt-0.5">
            {timeago(entry.created_at)}
          </div>
        </div>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDismiss();
          }}
          className="text-[11px] text-textMute hover:text-text"
          style={{ padding: "3px 6px" }}
          title="Dismiss"
        >
          ×
        </button>
      </div>
      {open && entry.body && (
        <pre
          style={{
            marginTop: 10,
            padding: 12,
            background: "var(--panel2)",
            borderRadius: 6,
            fontSize: 11,
            color: "var(--textDim)",
            maxHeight: 240,
            overflow: "auto",
            whiteSpace: "pre-wrap",
          }}
        >
          {entry.body}
        </pre>
      )}
    </div>
  );
}

function timeago(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const diff = Date.now() - t;
  const future = diff < 0;
  const abs = Math.abs(diff);
  const sec = Math.floor(abs / 1000);
  const min = Math.floor(sec / 60);
  const hr = Math.floor(min / 60);
  const day = Math.floor(hr / 24);
  const str = day > 0 ? `${day}d` : hr > 0 ? `${hr}h` : min > 0 ? `${min}m` : `${sec}s`;
  return future ? `in ${str}` : `${str} ago`;
}

function durationMs(startIso: string, endIso: string): string {
  const ms = Date.parse(endIso) - Date.parse(startIso);
  if (!Number.isFinite(ms) || ms < 0) return "";
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

import { useEffect, useMemo, useState } from "react";
import type { InboxEntry, Page, PagesResponse } from "../types";
import { Card, PageSearch } from "../atoms";
import { IconPages, IconPullRequest, IconRepos, IconCheck, IconBranch } from "../icons";

interface Preset {
  id: string;
  icon: React.ReactNode;
  title: string;
  hint: string;
  prompt: string;
}

const PRESETS: Preset[] = [
  {
    id: "issues-by-state",
    icon: <IconPages size={13} />,
    title: "Issues by state",
    hint: "Bar chart",
    prompt:
      "Show a bar chart of all issues grouped by their current workflow state, sorted by count descending.",
  },
  {
    id: "prs-merged-this-week",
    icon: <IconPullRequest size={13} />,
    title: "PRs merged this week",
    hint: "Grouped by author",
    prompt:
      "Show PRs merged in the last 7 days grouped by author with counts and links to each PR.",
  },
  {
    id: "automations-health",
    icon: <IconCheck size={13} />,
    title: "Automations health",
    hint: "Last 50 runs",
    prompt:
      "Show a table of automations with their last run status, last run time, and the success rate over the last 50 runs.",
  },
  {
    id: "repos-by-language",
    icon: <IconRepos size={13} />,
    title: "Repos by language",
    hint: "Pie chart",
    prompt:
      "Show a pie chart of connected repos grouped by primary_language. Include a legend with counts.",
  },
  {
    id: "issue-velocity",
    icon: <IconBranch size={13} />,
    title: "Issue velocity",
    hint: "Line chart",
    prompt:
      "Show a line chart of issues opened vs closed per day over the last 30 days.",
  },
];

interface Props {
  density: "comfortable" | "dense";
  query: string;
  onQueryChange: (q: string) => void;
  onOpenPage: (slug: string) => void;
}

export function Pages({ density, query, onQueryChange, onOpenPage }: Props) {
  const pad = density === "dense" ? 14 : 20;
  const [pages, setPages] = useState<Page[] | null>(null);
  const [requests, setRequests] = useState<InboxEntry[]>([]);
  const [nl, setNl] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [submitMsg, setSubmitMsg] = useState<string | null>(null);

  async function refresh() {
    try {
      const r = await fetch("/api/pages");
      if (!r.ok) return;
      const j: PagesResponse = await r.json();
      setPages(j.pages ?? []);
      setRequests(j.requests ?? []);
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
    const list = pages ?? [];
    const q = query.trim().toLowerCase();
    if (!q) return list;
    return list.filter(
      (p) => p.title.toLowerCase().includes(q) || p.slug.toLowerCase().includes(q),
    );
  }, [pages, query]);

  async function submitRequest() {
    if (!nl.trim() || submitting) return;
    setSubmitting(true);
    setSubmitMsg(null);
    try {
      const res = await fetch("/api/pages/request", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ nl }),
      });
      if (!res.ok) {
        const j = await res.json().catch(() => ({}));
        setSubmitMsg(`error: ${j.error ?? res.statusText}`);
      } else {
        setNl("");
        setSubmitMsg("Spec drafted into your inbox. Your harness will write the page.");
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
            Pages
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {pages === null
              ? "loading…"
              : `${filtered.length} page${filtered.length === 1 ? "" : "s"}`}
            {requests.length > 0 && (
              <>
                {" · "}
                <span className="text-amber">
                  {requests.length} pending request{requests.length === 1 ? "" : "s"}
                </span>
              </>
            )}
          </div>
        </div>
        <PageSearch
          pageId="pages"
          value={query}
          onChange={onQueryChange}
          placeholder="Search pages…"
        />
      </div>

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
            sub="Open the inbox entry in your coding harness to author the page."
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
          title="Pages"
          sub="Folders under pages/. page.tsx + meta.toml. Sandboxed iframe runtime."
        />
        {filtered.length === 0 ? (
          <div className="px-4 py-12 text-center text-textMute text-[12px] flex flex-col items-center gap-3">
            <span style={{ color: "var(--textMute)" }}>
              <IconPages size={20} />
            </span>
            <div>
              No pages yet. Type a description above and your harness will draft one.
            </div>
          </div>
        ) : (
          filtered.map((p, idx) => (
            <PageRow
              key={p.slug}
              page={p}
              first={idx === 0}
              onOpen={() => onOpenPage(p.slug)}
            />
          ))
        )}
      </Card>
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
          New page
        </div>
        <textarea
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder="Describe what you want to see — e.g. &quot;PRs merged this week grouped by author&quot;"
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
        title="Starter prompts"
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
        {PRESETS.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => onPick(p.prompt)}
            className="text-left bg-panel2 border border-border rounded-md cursor-pointer hover:border-accent"
            style={{ padding: 10 }}
          >
            <div className="flex items-center gap-2">
              <span style={{ color: "var(--textDim)" }}>{p.icon}</span>
              <span className="text-[12px] font-medium text-text truncate">{p.title}</span>
              <span className="ml-auto text-[10px] text-textMute shrink-0">{p.hint}</span>
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
    <div className="border-b border-borderS" style={{ padding: "10px 16px" }}>
      <div className="text-[12px] font-semibold text-text">{title}</div>
      {sub && <div className="text-[11px] text-textMute mt-0.5">{sub}</div>}
    </div>
  );
}

function PageRow({
  page,
  first,
  onOpen,
}: {
  page: Page;
  first: boolean;
  onOpen: () => void;
}) {
  const status = page.parse_error ? "parse-error" : page.last_error ? "error" : "ok";
  return (
    <div
      onClick={onOpen}
      className="sym-row cursor-pointer flex items-center gap-3"
      style={{
        padding: "12px 16px",
        borderTop: first ? "none" : "1px solid var(--borderS)",
      }}
    >
      <StatusDot status={status} />
      <div className="flex-1 min-w-0">
        <div className="text-[13px] text-text font-medium truncate">{page.title}</div>
        <div className="flex items-center gap-2 mt-1">
          <span className="text-[11px] text-textDim font-mono">{page.slug}</span>
          {page.last_opened_at && (
            <span className="text-[11px] text-textMute">· last opened {timeago(page.last_opened_at)}</span>
          )}
          {page.parse_error && (
            <span className="text-[11px]" style={{ color: "#ef4444" }}>
              · {page.parse_error}
            </span>
          )}
        </div>
      </div>
      <span className="text-[10px] text-textMute mr-1">›</span>
    </div>
  );
}

function StatusDot({ status }: { status: string }) {
  const color =
    status === "ok"
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

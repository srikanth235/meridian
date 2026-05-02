import { useMemo } from "react";
import type { HarnessId, Issue, Snapshot } from "../types";
import { Card, PageSearch, Pill, RepoChip, TypePill } from "../atoms";
import { IconInbox, IconX } from "../icons";
import { inboxIssues } from "../symphonyMap";
import { issueMatches } from "../search";
import { AssignMenu } from "./AssignMenu";

export function Inbox({
  snapshot,
  density,
  repoFilter,
  query,
  onQueryChange,
  onOpenIssue,
  onAssign,
  onIgnore,
}: {
  snapshot: Snapshot;
  density: "comfortable" | "dense";
  repoFilter: string | null;
  query: string;
  onQueryChange: (q: string) => void;
  onOpenIssue: (id: string) => void;
  onAssign: (issueId: string, harness: HarnessId | null) => void;
  onIgnore: (issueId: string) => void;
}) {
  const harnesses = snapshot.harnesses ?? [];
  const pad = density === "dense" ? 14 : 20;
  const items = useMemo(() => {
    let all = inboxIssues(snapshot);
    if (repoFilter) all = all.filter((i) => i.repo === repoFilter);
    if (query) all = all.filter((i) => issueMatches(i, query));
    return all;
  }, [snapshot, repoFilter, query]);

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad }}>
      <div className="flex items-end justify-between gap-3">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">Workspace</div>
          <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
            Inbox
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {items.length} {items.length === 1 ? "untriaged item" : "untriaged items"}
            {repoFilter && <> · scoped to <span className="font-mono">{repoFilter}</span></>}
          </div>
        </div>
        <PageSearch
          pageId="inbox"
          value={query}
          onChange={onQueryChange}
          placeholder="Search inbox…"
        />
      </div>

      <Card>
        {items.length === 0 ? (
          <div className="px-4 py-16 text-center text-textMute text-[12px] flex flex-col items-center gap-3">
            <span style={{ color: "var(--textMute)" }}><IconInbox size={20} /></span>
            <div>Inbox zero. Nothing new to triage.</div>
          </div>
        ) : (
          items.map((i, idx) => (
            <InboxRow
              key={i.id}
              issue={i}
              first={idx === 0}
              harnesses={harnesses}
              onOpen={() => onOpenIssue(i.id)}
              onAssign={(h) => onAssign(i.id, h)}
              onIgnore={() => onIgnore(i.id)}
            />
          ))
        )}
      </Card>
    </div>
  );
}

function InboxRow({
  issue,
  first,
  harnesses,
  onOpen,
  onAssign,
  onIgnore,
}: {
  issue: Issue;
  first: boolean;
  harnesses: import("../types").Harness[];
  onOpen: () => void;
  onAssign: (h: HarnessId | null) => void;
  onIgnore: () => void;
}) {
  return (
    <div
      onClick={onOpen}
      className="sym-row cursor-pointer flex items-center gap-3"
      style={{
        padding: "12px 16px",
        borderTop: first ? "none" : "1px solid var(--borderS)",
      }}
    >
      <span className="w-1.5 h-1.5 rounded-full bg-amber shrink-0" />
      <span className="font-mono text-[11.5px] text-textDim font-medium shrink-0" style={{ width: 90 }}>
        {issue.identifier}
      </span>
      <div className="flex-1 min-w-0">
        <div className="text-[13px] text-text font-medium truncate">{issue.title}</div>
        <div className="flex items-center gap-1.5 mt-1 flex-wrap">
          <TypePill type={issue.type} />
          <RepoChip repo={issue.repo} />
          {issue.labels.slice(0, 3).map((l) => (
            <Pill key={l} tone="muted">{l}</Pill>
          ))}
        </div>
      </div>
      <div className="flex items-center gap-1.5 shrink-0" onClick={(e) => e.stopPropagation()}>
        <AssignMenu
          harnesses={harnesses}
          current={null}
          onAssign={onAssign}
          size="sm"
        />
        <button
          onClick={onIgnore}
          title="Ignore (don't surface again)"
          className="inline-flex items-center justify-center cursor-pointer text-textMute hover:text-textDim"
          style={{
            width: 24, height: 24, borderRadius: 6,
            background: "transparent", border: "1px solid var(--border)",
          }}
        >
          <IconX size={11} />
        </button>
      </div>
    </div>
  );
}

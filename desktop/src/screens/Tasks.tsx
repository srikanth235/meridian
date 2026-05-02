import { useMemo, useState } from "react";
import type { Harness, HarnessId, Issue, Snapshot, TaskStatus, TaskType } from "../types";
import {
  Card,
  HarnessAvatar,
  PageSearch,
  Pill,
  RepoChip,
  TabPill,
  TypePill,
  harnessById,
} from "../atoms";
import {
  TASK_STATUS_META,
  TASK_STATUS_ORDER,
  taskStatusOf,
  triagedIssues,
} from "../symphonyMap";
import { issueMatches } from "../search";
import { AssignMenu } from "./AssignMenu";

type AssigneeFilter = HarnessId | "all" | "unassigned";
type TypeFilter = TaskType | "all";

export function Tasks({
  snapshot,
  density,
  onOpenIssue,
  repoFilter,
  query,
  onQueryChange,
  onAssign,
}: {
  snapshot: Snapshot;
  density: "comfortable" | "dense";
  onOpenIssue: (id: string) => void;
  repoFilter: string | null;
  query: string;
  onQueryChange: (q: string) => void;
  onAssign: (issueId: string, harness: HarnessId | null) => void;
}) {
  const harnesses = snapshot.harnesses ?? [];
  const [assignee, setAssignee] = useState<AssigneeFilter>("all");
  const [type, setType] = useState<TypeFilter>("all");
  const pad = density === "dense" ? 14 : 20;

  const all = useMemo(() => triagedIssues(snapshot), [snapshot]);
  const filtered = useMemo(() => {
    return all.filter((i) => {
      if (repoFilter && i.repo !== repoFilter) return false;
      if (type !== "all" && (i.type ?? "issue") !== type) return false;
      if (assignee === "unassigned" && i.assignee) return false;
      if (assignee !== "all" && assignee !== "unassigned" && i.assignee !== assignee) return false;
      if (!issueMatches(i, query)) return false;
      return true;
    });
  }, [all, repoFilter, type, assignee, query]);

  const runningIds = useMemo(
    () => new Set(snapshot.running.map((r) => r.issue.id)),
    [snapshot.running],
  );

  const columns = useMemo(() => {
    const out: Record<TaskStatus, Issue[]> = {
      todo: [], in_progress: [], in_review: [], done: [],
    };
    for (const i of filtered) {
      out[taskStatusOf(i, runningIds)].push(i);
    }
    return out;
  }, [filtered, runningIds]);

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: pad, height: "100%", minHeight: 0 }}>
      <div className="flex items-end justify-between gap-3 shrink-0">
        <div>
          <div className="text-[11px] font-medium text-textMute uppercase tracking-[0.6px] mb-1.5">Workspace</div>
          <h1 className="text-[22px] font-semibold text-text leading-tight m-0" style={{ letterSpacing: -0.3 }}>
            Tasks
          </h1>
          <div className="text-[13px] text-textDim mt-1">
            {filtered.length} {filtered.length === 1 ? "task" : "tasks"}
            {repoFilter && <> · scoped to <span className="font-mono">{repoFilter}</span></>}
          </div>
        </div>
        <div className="flex items-center gap-2 flex-wrap justify-end">
          <PageSearch
            pageId="tasks"
            value={query}
            onChange={onQueryChange}
            placeholder="Search tasks…"
          />
          <Filters
            label="Type"
            options={[
              { id: "all", label: "All" },
              { id: "issue", label: "Issues" },
              { id: "pr", label: "PRs" },
            ]}
            value={type}
            onChange={(v) => setType(v as TypeFilter)}
          />
          <Filters
            label="Assignee"
            options={[
              { id: "all", label: "All" },
              { id: "unassigned", label: "Unassigned" },
              ...harnesses.map((h) => ({ id: h.id, label: h.name })),
            ]}
            value={assignee}
            onChange={(v) => setAssignee(v as AssigneeFilter)}
          />
        </div>
      </div>

      <div
        className="flex-1 min-h-0 overflow-x-auto overflow-y-hidden"
        style={{ display: "grid", gridTemplateColumns: "repeat(4, minmax(260px, 1fr))", gap: 12 }}
      >
        {TASK_STATUS_ORDER.map((s) => (
          <Column
            key={s}
            status={s}
            issues={columns[s]}
            harnesses={harnesses}
            onOpen={onOpenIssue}
            onAssign={onAssign}
          />
        ))}
      </div>
    </div>
  );
}

function Filters<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: Array<{ id: T; label: string }>;
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex items-center gap-1">
      <span className="text-[10px] uppercase tracking-[0.6px] text-textMute font-medium mr-0.5">{label}</span>
      {options.map((o) => (
        <TabPill key={o.id} active={value === o.id} onClick={() => onChange(o.id)}>
          {o.label}
        </TabPill>
      ))}
    </div>
  );
}

function Column({
  status,
  issues,
  harnesses,
  onOpen,
  onAssign,
}: {
  status: TaskStatus;
  issues: Issue[];
  harnesses: Harness[];
  onOpen: (id: string) => void;
  onAssign: (issueId: string, harness: HarnessId | null) => void;
}) {
  const meta = TASK_STATUS_META[status];
  return (
    <Card className="flex flex-col min-h-0">
      <div className="flex items-center justify-between px-3 py-2 border-b border-borderS shrink-0">
        <div className="flex items-center gap-2">
          <span className="w-1.5 h-1.5 rounded-full" style={{ background: meta.color }} />
          <span className="text-[12px] font-semibold text-text">{meta.label}</span>
        </div>
        <span className="font-mono text-[11px] text-textMute tabular-nums">{issues.length}</span>
      </div>
      <div className="overflow-y-auto p-2 flex flex-col gap-1.5 flex-1">
        {issues.length === 0 ? (
          <div className="px-2 py-6 text-center text-textMute text-[11.5px]">No tasks.</div>
        ) : (
          issues.map((i) => (
            <TaskCard
              key={i.id}
              issue={i}
              harnesses={harnesses}
              onOpen={() => onOpen(i.id)}
              onAssign={(h) => onAssign(i.id, h)}
            />
          ))
        )}
      </div>
    </Card>
  );
}

function TaskCard({
  issue,
  harnesses,
  onOpen,
  onAssign,
}: {
  issue: Issue;
  harnesses: Harness[];
  onOpen: () => void;
  onAssign: (h: HarnessId | null) => void;
}) {
  const harness = harnessById(harnesses, issue.assignee);
  return (
    <div
      onClick={onOpen}
      className="cursor-pointer rounded-md"
      style={{
        background: "var(--panel2)",
        border: "1px solid var(--borderS)",
        padding: 10,
        display: "flex",
        flexDirection: "column",
        gap: 6,
      }}
    >
      <div className="flex items-center gap-1.5">
        <span className="font-mono text-[10.5px] text-textMute font-medium">{issue.identifier}</span>
        <TypePill type={issue.type} />
        <div className="ml-auto" onClick={(e) => e.stopPropagation()}>
          <HarnessAvatar harness={harness} size={18} title={harness?.name ?? "Unassigned — click to assign"} />
        </div>
      </div>
      <div className="text-[12.5px] text-text leading-snug" style={{ display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden" }}>
        {issue.title}
      </div>
      <div className="flex items-center gap-1.5 flex-wrap">
        <RepoChip repo={issue.repo} />
        {issue.labels.slice(0, 2).map((l) => (
          <Pill key={l} tone="muted">{l}</Pill>
        ))}
        <div className="ml-auto" onClick={(e) => e.stopPropagation()}>
          <AssignMenu
            harnesses={harnesses}
            current={issue.assignee ?? null}
            onAssign={onAssign}
            size="sm"
          />
        </div>
      </div>
    </div>
  );
}

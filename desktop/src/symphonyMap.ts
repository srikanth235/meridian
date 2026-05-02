// Map raw snapshot data → the shape Meridian screens expect.
// Where the orchestrator doesn't yet expose a field (worker assignment, workflow,
// progress %), we derive a sensible value so the UI stays meaningful.

import type {
  HarnessId,
  Issue,
  RetryRow,
  RunningRow,
  Snapshot,
  TaskStatus,
} from "./types";
import type { SymStatus } from "./atoms";

export interface SymAgent {
  id: string;
  internalId: string;
  title: string;
  branch: string | null;
  workflow: string | null;
  worker: string | null;
  status: SymStatus;
  step: string;
  progress: number;
  elapsedSec: number;
  url: string | null;
  labels: string[];
  harness?: HarnessId | null;
  repo?: string | null;
}

function workflowFromLabels(labels: string[]): string {
  const set = new Set(labels.map((l) => l.toLowerCase()));
  if (set.has("review")) return "PR review";
  if (set.has("bug") || set.has("fix")) return "Bugfix";
  if (set.has("feature") || set.has("feat")) return "Feature";
  if (set.has("refactor")) return "Refactor";
  if (set.has("docs")) return "Docs";
  if (set.has("triage")) return "Triage";
  return "Bugfix";
}

function elapsedFrom(iso: string | null | undefined): number {
  if (!iso) return 0;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return 0;
  return Math.max(0, Math.floor((Date.now() - t) / 1000));
}

export function runningAgents(snapshot: Snapshot): SymAgent[] {
  return snapshot.running.map((r: RunningRow) => {
    const i = r.issue;
    return {
      id: i.identifier || i.id,
      internalId: i.id,
      title: i.title,
      branch: i.branch_name ?? null,
      workflow: workflowFromLabels(i.labels),
      worker: r.harness ?? i.assignee ?? null,
      status: "running",
      step: r.last_event ?? "running",
      progress: Math.min(0.95, 0.1 + r.turn_count * 0.08),
      elapsedSec: elapsedFrom(r.started_at),
      url: i.url ?? null,
      labels: i.labels,
      harness: r.harness ?? i.assignee ?? null,
      repo: i.repo ?? null,
    };
  });
}

export function queuedAgents(snapshot: Snapshot): SymAgent[] {
  return snapshot.retrying.map((r: RetryRow) => ({
    id: r.identifier || r.issue_id,
    internalId: r.issue_id,
    title: r.error ?? "Queued for retry",
    branch: null,
    workflow: null,
    worker: null,
    status: "queued",
    step: `retry #${r.attempt}`,
    progress: 0,
    elapsedSec: 0,
    url: null,
    labels: [],
  }));
}

export function statusOf(
  issue: Issue,
  runningIds: Set<string>,
  retryIds: Set<string>,
): SymStatus {
  if (runningIds.has(issue.id)) return "running";
  if (retryIds.has(issue.id)) return "failed";
  if (issue.state === "closed") return "merged";
  if (issue.state.startsWith("status:review")) return "review";
  return "queued";
}

// Linear-style task status (the kanban lanes).
export function taskStatusOf(
  issue: Issue,
  runningIds: Set<string>,
): TaskStatus {
  if (runningIds.has(issue.id)) return "in_progress";
  if (issue.state === "closed") return "done";
  if (issue.state.startsWith("status:review")) return "in_review";
  return "todo";
}

export const TASK_STATUS_META: Record<
  TaskStatus,
  { label: string; color: string; tone: "ok" | "blue" | "purple" | "muted" }
> = {
  todo: { label: "Todo", color: "#9a9a9a", tone: "muted" },
  in_progress: { label: "In progress", color: "#10b981", tone: "ok" },
  in_review: { label: "In review", color: "#3b82f6", tone: "blue" },
  done: { label: "Done", color: "#a855f7", tone: "purple" },
};

export const TASK_STATUS_ORDER: TaskStatus[] = [
  "todo",
  "in_progress",
  "in_review",
  "done",
];

// Collect all triaged issues across the snapshot, deduped.
export function triagedIssues(snapshot: Snapshot): Issue[] {
  const seen = new Set<string>();
  const out: Issue[] = [];
  const consider = (i: Issue) => {
    if (seen.has(i.id)) return;
    if (i.triaged === false) return; // belongs to inbox
    seen.add(i.id);
    out.push(i);
  };
  for (const r of snapshot.running) consider(r.issue);
  for (const col of snapshot.kanban?.columns ?? []) {
    for (const i of col.issues) consider(i);
  }
  for (const i of snapshot.kanban?.unsorted ?? []) consider(i);
  return out;
}

export function inboxIssues(snapshot: Snapshot): Issue[] {
  if (snapshot.inbox && snapshot.inbox.length) return snapshot.inbox;
  // Fallback: derive from kanban / unsorted by `triaged === false`.
  const out: Issue[] = [];
  const seen = new Set<string>();
  const consider = (i: Issue) => {
    if (seen.has(i.id)) return;
    if (i.triaged === false) {
      seen.add(i.id);
      out.push(i);
    }
  };
  for (const col of snapshot.kanban?.columns ?? []) {
    for (const i of col.issues) consider(i);
  }
  for (const i of snapshot.kanban?.unsorted ?? []) consider(i);
  return out;
}

export function allAgents(snapshot: Snapshot): SymAgent[] {
  const runningIds = new Set(snapshot.running.map((r) => r.issue.id));
  const retryIds = new Set(snapshot.retrying.map((r) => r.issue_id));
  const seen = new Set<string>();
  const out: SymAgent[] = [...runningAgents(snapshot)];
  for (const a of out) seen.add(a.internalId);

  const collect = (issues: Issue[]) => {
    for (const i of issues) {
      if (seen.has(i.id)) continue;
      if (i.triaged === false) continue;
      seen.add(i.id);
      const status = statusOf(i, runningIds, retryIds);
      out.push({
        id: i.identifier || i.id,
        internalId: i.id,
        title: i.title,
        branch: i.branch_name ?? null,
        workflow: workflowFromLabels(i.labels),
        worker: status === "running" ? (i.assignee ?? null) : null,
        status,
        step: i.state,
        progress: status === "merged" ? 1 : status === "review" ? 0.95 : 0,
        elapsedSec: 0,
        url: i.url ?? null,
        labels: i.labels,
        harness: i.assignee ?? null,
        repo: i.repo ?? null,
      });
    }
  };
  for (const col of snapshot.kanban?.columns ?? []) collect(col.issues);
  collect(snapshot.kanban?.unsorted ?? []);
  return out;
}

export function dashboardStats(snapshot: Snapshot) {
  const running = snapshot.running.length;
  const queued = snapshot.retrying.length;
  const all = allAgents(snapshot);
  const review = all.filter((a) => a.status === "review").length;
  const merged24h = all.filter((a) => a.status === "merged").length;
  const failed = all.filter((a) => a.status === "failed").length;
  return { running, queued, review, failed, merged24h };
}

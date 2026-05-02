// Map raw snapshot data → the shape Symphony screens expect.
// Where the orchestrator doesn't yet expose a field (worker assignment, workflow,
// progress %), we derive a sensible value so the UI stays meaningful.

import type { Issue, RetryRow, RunningRow, Snapshot } from "./types";
import type { SymStatus } from "./atoms";

export interface SymAgent {
  id: string;            // display id (e.g. CDX-2841 or repo's identifier)
  internalId: string;    // snapshot issue id (for routing)
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
}

const WORKERS = ["aurora", "borealis", "cygnus", "draco", "eridanus"];

function workflowFromLabels(labels: string[]): string {
  const set = new Set(labels.map((l) => l.toLowerCase()));
  if (set.has("bug") || set.has("fix")) return "Bugfix";
  if (set.has("feature") || set.has("feat")) return "Feature";
  if (set.has("refactor")) return "Refactor";
  if (set.has("docs")) return "Docs";
  if (set.has("triage")) return "Triage";
  return "Bugfix";
}

// Stable hash → pick a deterministic worker for an issue id so screens don't
// reshuffle between renders. Replace with real assignment when surfaced.
function fakeWorker(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) | 0;
  return WORKERS[Math.abs(h) % WORKERS.length];
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
      worker: fakeWorker(i.id),
      status: "running",
      step: r.last_event ?? "running",
      // Heuristic: each turn is roughly ~10% of the run, capped at 95% until done.
      progress: Math.min(0.95, 0.1 + r.turn_count * 0.08),
      elapsedSec: elapsedFrom(r.started_at),
      url: i.url ?? null,
      labels: i.labels,
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
  retryIds: Set<string>
): SymStatus {
  if (runningIds.has(issue.id)) return "running";
  if (retryIds.has(issue.id)) return "failed";
  if (issue.state === "closed") return "merged";
  if (issue.state.startsWith("status:review")) return "review";
  return "queued";
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
      seen.add(i.id);
      const status = statusOf(i, runningIds, retryIds);
      out.push({
        id: i.identifier || i.id,
        internalId: i.id,
        title: i.title,
        branch: i.branch_name ?? null,
        workflow: workflowFromLabels(i.labels),
        worker: status === "running" ? fakeWorker(i.id) : null,
        status,
        step: i.state,
        progress: status === "merged" ? 1 : status === "review" ? 0.95 : 0,
        elapsedSec: 0,
        url: i.url ?? null,
        labels: i.labels,
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

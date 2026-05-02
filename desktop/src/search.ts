// Tiny case-insensitive substring matchers used by per-page Linear-style
// search. Centralized so every screen agrees on what fields are searched.

import type { Harness, Issue, Repo } from "./types";

const norm = (s: string | null | undefined): string => (s ?? "").toLowerCase();

/// True when any of `q`'s whitespace-separated tokens appears in any of the
/// issue's searchable fields. Empty `q` returns true.
export function issueMatches(issue: Issue, q: string): boolean {
  if (!q.trim()) return true;
  const haystack = [
    issue.identifier,
    issue.title,
    issue.repo ?? "",
    issue.state,
    issue.assignee ?? "",
    ...issue.labels,
  ]
    .map(norm)
    .join(" ");
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((tok) => haystack.includes(tok));
}

export function repoMatches(repo: Repo, q: string): boolean {
  if (!q.trim()) return true;
  const haystack = [
    repo.slug,
    repo.description ?? "",
    repo.primary_language ?? "",
    repo.default_branch ?? "",
  ]
    .map(norm)
    .join(" ");
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((tok) => haystack.includes(tok));
}

export function harnessMatches(harness: Harness, q: string): boolean {
  if (!q.trim()) return true;
  const haystack = [
    harness.name,
    harness.binary,
    harness.id,
    harness.version ?? "",
    ...(harness.capabilities ?? []),
  ]
    .map(norm)
    .join(" ");
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((tok) => haystack.includes(tok));
}

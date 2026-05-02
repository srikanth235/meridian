// Fixture data used to fill Meridian screens before the orchestrator surfaces
// the corresponding feeds. Replace with backend data when available.

import type { Repo } from "./types";

export interface DummyWorkflow {
  id: string;
  name: string;
  description: string;
  steps: string[];
  timeoutMin: number;
  uses: number;
}

export const DUMMY_REPOS: Repo[] = [
  {
    slug: "openai/codex",
    open_issues: 18,
    in_flight: 3,
    errored: 1,
    last_synced_at: new Date(Date.now() - 45_000).toISOString(),
  },
  {
    slug: "anthropics/claude-code",
    open_issues: 9,
    in_flight: 1,
    errored: 0,
    last_synced_at: new Date(Date.now() - 75_000).toISOString(),
  },
  {
    slug: "srikanth235/meridian",
    open_issues: 6,
    in_flight: 0,
    errored: 0,
    last_synced_at: new Date(Date.now() - 20_000).toISOString(),
  },
];

export const DUMMY_WORKFLOWS: DummyWorkflow[] = [
  { id: "wf-bugfix",   name: "Bugfix",   description: "Reproduce, write failing test, fix, verify.",  steps: ["reproduce", "write-failing-test", "patch", "verify", "open-pr"], timeoutMin: 45,  uses: 142 },
  { id: "wf-feature",  name: "Feature",  description: "Design doc → implementation → tests → PR.",    steps: ["design", "scaffold", "implement", "test", "open-pr"],          timeoutMin: 120, uses: 87  },
  { id: "wf-refactor", name: "Refactor", description: "No behavior change. Diff stays small.",        steps: ["plan", "apply", "verify-no-regression", "open-pr"],            timeoutMin: 60,  uses: 41  },
  { id: "wf-docs",     name: "Docs",     description: "Update inline docs and README from code.",     steps: ["scan", "draft", "lint", "open-pr"],                            timeoutMin: 20,  uses: 213 },
  { id: "wf-triage",   name: "Triage",   description: "Read issue, label, suggest workflow.",         steps: ["read", "classify", "comment"],                                 timeoutMin: 5,   uses: 506 },
  { id: "wf-review",   name: "PR review", description: "Read diff, post structured review comments.", steps: ["fetch-pr", "read-diff", "comment"],                            timeoutMin: 15,  uses: 318 },
];

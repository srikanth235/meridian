// A self-contained demo snapshot for screenshots / showcasing the UI without
// the orchestrator daemon. Activated by appending `?demo=1` to the URL.

import type { HarnessId, Issue, Snapshot } from "./types";
import { DUMMY_REPOS } from "./dummyData";

function isoMinutesAgo(m: number): string {
  return new Date(Date.now() - m * 60 * 1000).toISOString();
}

function mkIssue(opts: {
  id: string;
  identifier: string;
  title: string;
  state: string;
  labels: string[];
  branch: string | null;
  repo: string;
  type?: "issue" | "pr";
  assignee?: HarnessId | null;
  triaged?: boolean;
  description?: string;
}): Issue {
  return {
    id: opts.id,
    identifier: opts.identifier,
    title: opts.title,
    state: opts.state,
    labels: opts.labels,
    branch_name: opts.branch,
    url: `https://github.com/${opts.repo}/${opts.type === "pr" ? "pull" : "issues"}/${opts.identifier.replace(/^[A-Z]+-?/, "")}`,
    blocked_by: [],
    created_at: isoMinutesAgo(120),
    updated_at: isoMinutesAgo(5),
    repo: opts.repo,
    type: opts.type ?? "issue",
    assignee: opts.assignee ?? null,
    triaged: opts.triaged ?? true,
    description: opts.description ?? null,
  };
}

export function makeDemoSnapshot(): Snapshot {
  // Triaged + running
  const i1 = mkIssue({ id: "i1", identifier: "CDX-2841", title: "Streaming responses drop final chunk on slow networks", state: "open", labels: ["bug", "streaming", "p1"], branch: "fix/streaming-tail-CDX-2841", repo: "openai/codex", assignee: "codex" });
  const i2 = mkIssue({ id: "i2", identifier: "CDX-2839", title: "Add --json flag to `codex exec` for machine-readable output", state: "open", labels: ["feature", "cli"], branch: "feat/exec-json-CDX-2839", repo: "openai/codex", assignee: "codex" });
  const i5 = mkIssue({ id: "i5", identifier: "CDX-2828", title: "Refactor sandbox.rs: extract policy module", state: "open", labels: ["refactor", "sandbox"], branch: "refactor/sandbox-policy-CDX-2828", repo: "openai/codex", assignee: "claude-code" });
  const i7 = mkIssue({ id: "i7", identifier: "CDX-2821", title: "Triage: customer report — wrong shell quoting on zsh", state: "open", labels: ["triage"], branch: null, repo: "openai/codex", assignee: "gemini" });

  // Triaged kanban (todo / review / done)
  const i3 = mkIssue({ id: "i3", identifier: "CDX-2837", title: "Memory leak in long-running interactive sessions", state: "status:review", labels: ["bug", "memory", "p0"], branch: "fix/leak-interactive-CDX-2837", repo: "openai/codex", type: "pr", assignee: "claude-code" });
  const i4 = mkIssue({ id: "i4", identifier: "CDX-2832", title: "Update auth flow docs after device-code change", state: "open", labels: ["docs"], branch: "docs/auth-device-CDX-2832", repo: "openai/codex", assignee: "claude-code" });
  const i6 = mkIssue({ id: "i6", identifier: "CDX-2825", title: "Codex agent should respect .gitignore on file scan", state: "open", labels: ["bug", "p2"], branch: "fix/scan-gitignore-CDX-2825", repo: "openai/codex", assignee: "codex" });
  const i8 = mkIssue({ id: "i8", identifier: "CDX-2818", title: "Add OpenTelemetry traces to dispatcher", state: "open", labels: ["feature", "observability"], branch: "feat/otel-dispatcher-CDX-2818", repo: "openai/codex", assignee: null });
  const i9 = mkIssue({ id: "i9", identifier: "CDX-2815", title: "Improve error message when worker SSH key is missing", state: "closed", labels: ["bug", "dx"], branch: "fix/ssh-err-msg-CDX-2815", repo: "openai/codex", assignee: "codex" });

  // anthropics/claude-code repo
  const a1 = mkIssue({ id: "a1", identifier: "CC-412", title: "Slash command autocompletion drops after typing space", state: "open", labels: ["bug", "ui"], branch: "fix/slash-complete-CC-412", repo: "anthropics/claude-code", assignee: "claude-code" });
  const a2 = mkIssue({ id: "a2", identifier: "CC-410", title: "Review #408 — Add background-task notification API", state: "open", labels: ["review"], branch: null, repo: "anthropics/claude-code", type: "pr", assignee: "pi-mono" });
  const a3 = mkIssue({ id: "a3", identifier: "CC-405", title: "Document new permission modes in /docs/permissions.md", state: "open", labels: ["docs"], branch: null, repo: "anthropics/claude-code", assignee: null });

  // srikanth235/meridian repo
  const m1 = mkIssue({ id: "m1", identifier: "MD-22", title: "Per-harness concurrency lives on Harness profile, not global cap", state: "open", labels: ["meta"], branch: null, repo: "srikanth235/meridian", assignee: null });
  const m2 = mkIssue({ id: "m2", identifier: "MD-21", title: "Inbox status: untriaged scan items need first-class screen", state: "open", labels: ["ui"], branch: null, repo: "srikanth235/meridian", assignee: null });

  // Untriaged inbox items (just-scanned, no assignee, triaged: false)
  const inb1 = mkIssue({ id: "inb1", identifier: "CDX-2848", title: "[bug] Codex hangs when stdin closes mid-turn", state: "open", labels: ["bug"], branch: null, repo: "openai/codex", assignee: null, triaged: false });
  const inb2 = mkIssue({ id: "inb2", identifier: "CDX-2847", title: "[feature] Honor XDG_CONFIG_HOME when locating config.toml", state: "open", labels: ["feature"], branch: null, repo: "openai/codex", assignee: null, triaged: false });
  const inb3 = mkIssue({ id: "inb3", identifier: "CC-414", title: "Review #413 — Tool-use streaming refactor", state: "open", labels: ["review"], branch: null, repo: "anthropics/claude-code", type: "pr", assignee: null, triaged: false });
  const inb4 = mkIssue({ id: "inb4", identifier: "MD-23", title: "[ui] Repo picker should remember last selection across reloads", state: "open", labels: ["ui"], branch: null, repo: "srikanth235/meridian", assignee: null, triaged: false });
  const inb5 = mkIssue({ id: "inb5", identifier: "CC-415", title: "[bug] /clear sometimes leaves stale context lines visible", state: "open", labels: ["bug"], branch: null, repo: "anthropics/claude-code", assignee: null, triaged: false });

  return {
    running: [
      {
        issue: i1,
        workspace_path: "/work/openai/codex/CDX-2841",
        started_at: isoMinutesAgo(31),
        session_id: "s-9a3f",
        last_event: "patch",
        last_event_at: isoMinutesAgo(1),
        turn_count: 7,
        tokens_input: 184_201,
        tokens_output: 23_410,
        tokens_total: 207_611,
        last_message_tail: "applied patch to src/stream/buffer.ts (+34 −8)",
        harness: "codex",
      },
      {
        issue: i2,
        workspace_path: "/work/openai/codex/CDX-2839",
        started_at: isoMinutesAgo(49),
        session_id: "s-c1be",
        last_event: "implement",
        last_event_at: isoMinutesAgo(2),
        turn_count: 4,
        tokens_input: 312_440,
        tokens_output: 41_002,
        tokens_total: 353_442,
        last_message_tail: "drafting CLI flag wiring in src/cli/exec.ts",
        harness: "codex",
      },
      {
        issue: i5,
        workspace_path: "/work/openai/codex/CDX-2828",
        started_at: isoMinutesAgo(68),
        session_id: "s-d7ef",
        last_event: "apply",
        last_event_at: isoMinutesAgo(3),
        turn_count: 9,
        tokens_input: 244_033,
        tokens_output: 31_804,
        tokens_total: 275_837,
        last_message_tail: "moving policy fns into sandbox::policy",
        harness: "claude-code",
      },
      {
        issue: i7,
        workspace_path: "/work/openai/codex/CDX-2821",
        started_at: isoMinutesAgo(3),
        session_id: "s-e110",
        last_event: "classify",
        last_event_at: isoMinutesAgo(0.5),
        turn_count: 1,
        tokens_input: 12_004,
        tokens_output: 1_840,
        tokens_total: 13_844,
        last_message_tail: "labeling as bug, suggesting wf-bugfix",
        harness: "gemini",
      },
      {
        issue: a2,
        workspace_path: "/work/anthropics/claude-code/CC-410",
        started_at: isoMinutesAgo(11),
        session_id: "s-f203",
        last_event: "review",
        last_event_at: isoMinutesAgo(0.8),
        turn_count: 3,
        tokens_input: 88_120,
        tokens_output: 6_433,
        tokens_total: 94_553,
        last_message_tail: "noting 4 review comments on src/streaming/tool_use.rs",
        harness: "pi-mono",
      },
    ],
    retrying: [
      {
        issue_id: "i6",
        identifier: "CDX-2825",
        attempt: 2,
        due_at: new Date(Date.now() + 4 * 60 * 1000).toISOString(),
        error: "Test suite timed out after 600s on integration/scan.spec.ts",
      },
    ],
    codex_totals: {
      input_tokens: 18_000_000,
      output_tokens: 402_117,
      total_tokens: 18_402_117,
      seconds_running: 36_000,
    },
    rate_limits: null,
    generated_at: new Date().toISOString(),
    poll_interval_ms: 30_000,
    max_concurrent_agents: 8,
    kanban: {
      loaded: true,
      columns: [
        { state: "open", issues: [i4, i6, i8, a1, a3, m1, m2] },
        { state: "status:review", issues: [i3] },
        { state: "closed", issues: [i9] },
      ],
      unsorted: [],
    },
    repos: DUMMY_REPOS.filter((r) => r.connected).map((r) => r.slug),
    available_repos: DUMMY_REPOS,
    repo_status: {
      gh_available: true,
      gh_authenticated: true,
      error: null,
      last_refreshed_at: new Date().toISOString(),
    },
    paused: false,
    harnesses: [],
    inbox: [inb1, inb2, inb3, inb4, inb5],
  };
}

export function isDemoMode(): boolean {
  if (typeof window === "undefined") return false;
  return new URLSearchParams(window.location.search).has("demo");
}

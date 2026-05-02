// A self-contained demo snapshot for screenshots / showcasing the UI without
// the orchestrator daemon. Activated by appending `?demo=1` to the URL.

import type { Snapshot } from "./types";

function isoMinutesAgo(m: number): string {
  return new Date(Date.now() - m * 60 * 1000).toISOString();
}

const issue = (
  id: string,
  identifier: string,
  title: string,
  state: string,
  labels: string[],
  branch: string | null,
  repo: string,
): import("./types").Issue => ({
  id,
  identifier,
  title,
  state,
  labels,
  branch_name: branch,
  url: `https://github.com/${repo}/issues/${identifier.replace(/^[A-Z]+-?/, "")}`,
  blocked_by: [],
  created_at: isoMinutesAgo(120),
  updated_at: isoMinutesAgo(5),
  repo,
});

export function makeDemoSnapshot(): Snapshot {
  const i1 = issue("i1", "CDX-2841", "Streaming responses drop final chunk on slow networks", "open", ["bug", "streaming", "p1"], "fix/streaming-tail-CDX-2841", "openai/codex");
  const i2 = issue("i2", "CDX-2839", "Add --json flag to `codex exec` for machine-readable output", "open", ["feature", "cli"], "feat/exec-json-CDX-2839", "openai/codex");
  const i3 = issue("i3", "CDX-2837", "Memory leak in long-running interactive sessions", "status:review", ["bug", "memory", "p0"], "fix/leak-interactive-CDX-2837", "openai/codex");
  const i4 = issue("i4", "CDX-2832", "Update auth flow docs after device-code change", "open", ["docs"], "docs/auth-device-CDX-2832", "openai/codex");
  const i5 = issue("i5", "CDX-2828", "Refactor sandbox.rs: extract policy module", "open", ["refactor", "sandbox"], "refactor/sandbox-policy-CDX-2828", "openai/codex");
  const i6 = issue("i6", "CDX-2825", "Codex agent should respect .gitignore on file scan", "open", ["bug", "p2"], "fix/scan-gitignore-CDX-2825", "openai/codex");
  const i7 = issue("i7", "CDX-2821", "Triage: customer report — wrong shell quoting on zsh", "open", ["triage"], null, "openai/codex");
  const i8 = issue("i8", "CDX-2818", "Add OpenTelemetry traces to dispatcher", "open", ["feature", "observability"], "feat/otel-dispatcher-CDX-2818", "openai/codex");
  const i9 = issue("i9", "CDX-2815", "Improve error message when worker SSH key is missing", "closed", ["bug", "dx"], "fix/ssh-err-msg-CDX-2815", "openai/codex");

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
      seconds_running: 36000,
    },
    rate_limits: null,
    generated_at: new Date().toISOString(),
    poll_interval_ms: 30000,
    max_concurrent_agents: 8,
    kanban: {
      loaded: true,
      columns: [
        { state: "open", issues: [i4, i6, i8] },
        { state: "status:review", issues: [i3] },
        { state: "closed", issues: [i9] },
      ],
      unsorted: [],
    },
    repos: ["openai/codex"],
    paused: false,
  };
}

export function isDemoMode(): boolean {
  if (typeof window === "undefined") return false;
  return new URLSearchParams(window.location.search).has("demo");
}

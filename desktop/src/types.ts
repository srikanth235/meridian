export type HarnessId =
  | "codex"
  | "claude-code"
  | "gemini"
  | "pi-mono"
  | "opencode";

export type TaskType = "issue" | "pr";

export type TaskStatus = "todo" | "in_progress" | "in_review" | "done";

export interface Harness {
  id: HarnessId;
  name: string;
  binary: string;
  color: string;
  concurrency: number;
  in_flight: number;
  capabilities?: string[];
  available: boolean;
  version?: string | null;
  last_seen_at?: string | null;
}

export interface Repo {
  slug: string;
  open_issues: number;
  in_flight: number;
  errored: number;
  last_synced_at: string | null;
}

export interface Issue {
  id: string;
  identifier: string;
  title: string;
  description?: string | null;
  priority?: number | null;
  state: string;
  branch_name?: string | null;
  url?: string | null;
  labels: string[];
  blocked_by: Array<{ id?: string | null; identifier?: string | null; state?: string | null }>;
  created_at?: string | null;
  updated_at?: string | null;
  repo?: string | null;
  type?: TaskType;
  assignee?: HarnessId | null;
  triaged?: boolean;
}

export interface RunningRow {
  issue: Issue;
  workspace_path: string;
  started_at: string;
  session_id: string | null;
  last_event: string | null;
  last_event_at: string | null;
  turn_count: number;
  tokens_input: number;
  tokens_output: number;
  tokens_total: number;
  last_message_tail: string | null;
  harness?: HarnessId | null;
}

export interface RetryRow {
  issue_id: string;
  identifier: string;
  attempt: number;
  due_at: string;
  error: string | null;
}

export interface TotalsRow {
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  seconds_running: number;
}

export interface KanbanColumn {
  state: string;
  issues: Issue[];
}

export interface SessionLogEntry {
  at: string;
  kind: string;
  summary: string;
  detail: unknown | null;
}

export interface SessionLog {
  issue_id: string;
  entries: SessionLogEntry[];
  active: boolean;
}

export interface KanbanBoard {
  columns: KanbanColumn[];
  unsorted: Issue[];
  loaded: boolean;
}

export interface Snapshot {
  running: RunningRow[];
  retrying: RetryRow[];
  codex_totals: TotalsRow;
  rate_limits: unknown | null;
  generated_at: string;
  poll_interval_ms: number;
  max_concurrent_agents: number;
  kanban: KanbanBoard;
  repos: string[];
  paused: boolean;
  harnesses?: Harness[];
  repos_detail?: Repo[];
  inbox?: Issue[];
}

export interface WorkflowResponse {
  source_path: string;
  config: Record<string, unknown>;
  prompt_template: string;
}

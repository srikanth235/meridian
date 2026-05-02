---
tracker:
  kind: github
  # One or more repos to poll. `repo:` (singular) is also accepted for
  # back-compat; both forms get folded into a single list.
  repos:
    - srikanth235/meridian
  # `status:*` labels become kanban columns. `closed` is the special token
  # for the GitHub Closed state.
  active_states:
    - status:todo
    - status:in-progress
    - status:in-review
  terminal_states:
    - closed
  # When true, also poll open PRs and surface them as `kind: pr_review`
  # tasks. The synthetic states `pr:pending-review` (active) plus
  # `pr:reviewed` / `pr:approved` / `pr:merged` / `pr:closed` (terminal)
  # are auto-appended to active_states / terminal_states.
  review_prs: false
  # Optional: explicit column ordering (otherwise active ++ terminal).
  # columns:
  #   - status:todo
  #   - status:in-progress
  #   - status:in-review
  #   - closed

polling:
  interval_ms: 30000

workspace:
  root: ~/.meridian/workspaces

agent:
  # Start paused so opening the app doesn't auto-dispatch agents on every
  # status:todo issue. Toggle via the UI button (or POST /api/control/resume).
  paused: true
  max_concurrent_agents: 4
  max_turns: 20
  max_retry_backoff_ms: 300000

codex:
  command: codex app-server
  approval_policy: never
  thread_sandbox: danger-full-access
  turn_sandbox_policy:
    type: dangerFullAccess
  turn_timeout_ms: 3600000
  read_timeout_ms: 5000
  stall_timeout_ms: 300000

server:
  port: 7878

hooks:
  timeout_ms: 60000
  # after_create: |
  #   git clone git@github.com:org/repo.git .
  # before_run: |
  #   git fetch origin && git checkout main && git pull --ff-only
---

{% comment %}
================================================================================
Symphony task scenarios
================================================================================

Symphony is a tracker-driven dispatcher: a poller writes rows into the SQLite
tracker, the orchestrator picks up rows in `active_states`, and a Codex agent
is dispatched per row. There are two scenarios in scope today.

--------------------------------------------------------------------------------
Scenario A — Issue implementation (existing)
--------------------------------------------------------------------------------
Trigger:    A GitHub issue is created (by a human or another agent) on a
            configured repo and lands in one of the `active_states` (e.g.
            `status:todo`).
Ingestion:  The GitHub tracker poller upserts the issue into SQLite as a task
            with `task.kind = "issue"`.
Codex job:  Implement the issue. Move it through the kanban columns, make
            code changes, optionally open a PR.
Prompt:     The `task.kind == "issue"` branch below.

--------------------------------------------------------------------------------
Scenario B — PR review (new)
--------------------------------------------------------------------------------
Trigger:    A pull request is opened on a configured repo by *some other
            agent* (Symphony does not author PRs in this scenario) and is
            sitting in a "pending review" state — e.g. `review_requested`,
            label `status:in-review`, or simply `state:open` with no
            approving review yet.
Ingestion:  A PR poller scans open PRs on each configured repo and upserts
            each one into SQLite as a task with `task.kind = "pr_review"`,
            keyed by `<owner>/<repo>#<pr_number>` for idempotency. New
            commits or new review-request events on the same PR re-fire the
            task; an approving review or a merge moves it to a terminal
            state.
Codex job:  Read the PR diff and conversation, then post review comments
            (line comments + a summary review) via `gh pr review`. Codex
            does NOT push commits to the PR in this scenario; its only side
            effect is review feedback.
Prompt:     The `task.kind == "pr_review"` branch below.

--------------------------------------------------------------------------------
Implementation status
--------------------------------------------------------------------------------
- [x] Scenario A — issue ingestion + dispatch + prompt
- [x] Scenario B — `task.kind` field on `Issue` (`"issue"` | `"pr_review"`)
- [x] Scenario B — `tracker.review_prs: true` toggle (this file)
- [x] Scenario B — GitHub PR poller (`gh pr list` → tasks with
      `kind = "pr_review"`, id `<owner>/<repo>/pr/<num>`)
- [x] Scenario B — synthetic `pr:*` states auto-extend active/terminal
- [x] Scenario B — review-mode Codex prompt (branch below) bound via
      `task.kind` and `pr.*` Liquid namespaces
- [x] Scenario B — terminal on approval / merge / close / submitted review;
      re-fire happens naturally each poll tick while a PR stays in
      `pr:pending-review`

Notes:
- Re-fire on new commits is implicit: GitHub's `reviewDecision` resets to
  `REVIEW_REQUIRED` when commits invalidate prior reviews (depends on repo
  settings), which moves the task back into `pr:pending-review` and the
  next poll dispatches a fresh review turn.
- The agent must NOT push to the PR branch in this scenario — see the
  prompt below for the contract.
{% endcomment %}

{% assign kind = task.kind | default: "issue" %}

{% if kind == "pr_review" %}
You are the Meridian review agent for pull request **{{ pr.identifier }}**: {{ pr.title }}.

Repo: `{{ pr.repo }}`  ·  PR: `#{{ pr.number }}`  ·  Author: `{{ pr.author }}`

{% if pr.body %}
## PR description

{{ pr.body }}
{% endif %}

{% if pr.labels.size > 0 %}
**Labels:** {{ pr.labels | join: ", " }}
{% endif %}

{% if attempt %}
> This is retry/continuation attempt {{ attempt }}. Re-review only what changed since the previous attempt.
{% endif %}

## Your task

You are reviewing this PR — you must NOT push commits, rebase, or modify the
branch. Your only output is review feedback on GitHub.

1. Fetch the PR and inspect the diff:

   ```bash
   gh pr view {{ pr.number }} --repo {{ pr.repo }} --json title,body,files,commits
   gh pr diff {{ pr.number }} --repo {{ pr.repo }}
   ```
2. Read the existing review thread so you don't repeat prior feedback:

   ```bash
   gh pr view {{ pr.number }} --repo {{ pr.repo }} --comments
   ```
3. Form a review. Focus on correctness, security, test coverage, and
   adherence to repo conventions. Skip nits unless they're material.
4. Post line-level comments where they belong, then submit a single review
   with one of `--approve`, `--request-changes`, or `--comment`:

   ```bash
   gh pr review {{ pr.number }} --repo {{ pr.repo }} \
     --request-changes \
     --body "<summary of findings>"
   ```

If the PR is not actually ready for review (draft, conflicts, CI red), post
a single comment explaining why and stop:

```bash
gh pr comment {{ pr.number }} --repo {{ pr.repo }} --body "Skipping review: <reason>"
```

The `gh` CLI is authenticated. Do not push to the PR branch, do not run the
code, and do not ask interactive questions.

{% else %}
You are the Meridian agent working on GitHub issue **{{ issue.identifier }}**: {{ issue.title }}.

{% if issue.description %}
## Description

{{ issue.description }}
{% endif %}

{% if issue.labels.size > 0 %}
**Labels:** {{ issue.labels | join: ", " }}
{% endif %}

{% if attempt %}
> This is retry/continuation attempt {{ attempt }}. Pick up where the prior turn left off.
{% endif %}

## Your task

1. Read the issue carefully and form a plan.
2. Move the issue into the In-Progress column:

   ```bash
   gh issue edit {{ issue.id }} \
     --add-label "status:in-progress" \
     --remove-label "status:todo"
   ```
3. Make the code changes needed in this workspace directory (your `cwd`).
4. Run any tests / type-check / linter that make sense.
5. When the work is complete, close the issue:

   ```bash
   gh issue close {{ issue.id }} \
     --comment "Completed by Meridian agent. Workspace: $(pwd)" \
     --reason completed
   ```

If you are blocked and cannot complete the issue, post a comment instead and stop:

```bash
gh issue comment {{ issue.id }} --body "Blocked: <reason>"
```

The `gh` CLI is already authenticated for you. Stay inside this workspace
directory and do not ask interactive questions — there is no human in the loop.
{% endif %}

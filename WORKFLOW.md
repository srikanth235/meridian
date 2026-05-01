---
tracker:
  kind: github
  repo: srikanth235/meridian
  # `status:*` labels become kanban columns. `closed` is the special token
  # for the GitHub Closed state.
  active_states:
    - status:todo
    - status:in-progress
    - status:in-review
  terminal_states:
    - closed
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

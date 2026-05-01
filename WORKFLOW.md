---
tracker:
  kind: linear
  api_key: $LINEAR_API_KEY
  project_slug: 3b5400401c46
  active_states:
    - Todo
    - In Progress
  terminal_states:
    - Done
    - Cancelled
    - Closed
    - Duplicate

polling:
  interval_ms: 30000

workspace:
  root: ~/.meridian/workspaces

agent:
  max_concurrent_agents: 4
  max_turns: 20
  max_retry_backoff_ms: 300000

codex:
  command: codex app-server
  # AskForApproval enum: untrusted | on-failure | on-request | never
  approval_policy: never
  # SandboxMode enum: read-only | workspace-write | danger-full-access
  thread_sandbox: danger-full-access
  # SandboxPolicy object form. Other variants:
  #   {type: readOnly, networkAccess: false}
  #   {type: workspaceWrite, networkAccess: false}
  #   {type: externalSandbox, networkAccess: restricted}
  turn_sandbox_policy:
    type: dangerFullAccess
  turn_timeout_ms: 3600000
  # Wait this long for thread/start and turn/start ack. Codex may warm up
  # MCP servers before responding; meridian auto-relaxes this to 30s for
  # those two methods, but other RPCs use this value.
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

You are the Meridian agent working on Linear issue **{{ issue.identifier }}**: {{ issue.title }}.

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
2. Make the code changes needed in this workspace directory (your `cwd`).
3. Run any tests / type-check / linter that make sense.
4. When the work is complete, transition the Linear issue to **Done**.

## Closing the issue in Linear

When you finish, close out the issue with a single GraphQL call. The `LINEAR_API_KEY`
environment variable is already set for you.

```bash
curl -sS https://api.linear.app/graphql \
  -H "Authorization: $LINEAR_API_KEY" \
  -H "Content-Type: application/json" \
  -d @- <<'JSON'
{
  "query": "mutation($id: String!, $stateId: String!, $body: String!) { issueUpdate(id: $id, input: { stateId: $stateId }) { success } commentCreate(input: { issueId: $id, body: $body }) { success } }",
  "variables": {
    "id": "{{ issue.id }}",
    "stateId": "83bee53a-5f89-4b40-b016-6df022c3bfc4",
    "body": "Completed by Meridian agent. Workspace: $(pwd)"
  }
}
JSON
```

The `stateId` above is the **Done** state for the `Testing235` team. If you are blocked
and cannot complete the issue, instead post a comment explaining the blocker (use
`commentCreate` only) and stop without changing state.

Stay inside this workspace directory. Do not ask interactive questions — there is no
human in the loop.

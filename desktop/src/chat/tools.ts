// Tools the in-app chat agent can call. Each tool is a thin wrapper around
// a Meridian HTTP endpoint. Definitions follow the AgentTool contract from
// `@mariozechner/pi-agent-core` — typebox schema for parameters, async
// `execute` returning `{ content, details }`.

import type { AgentTool, AgentToolResult } from "@mariozechner/pi-agent-core";
import { Type, type Static } from "typebox";

const QueryParams = Type.Object({
  sql: Type.String({
    description:
      "Read-only SQL. Must start with SELECT or WITH. Bind params with positional ? markers.",
  }),
  params: Type.Optional(
    Type.Array(Type.Any(), {
      description: "Positional bind parameters for ? markers in sql. JSON values.",
    }),
  ),
});

const WritePageParams = Type.Object({
  slug: Type.String({
    description:
      "URL-safe folder name (a-z 0-9 - _). Becomes the immutable identity.",
  }),
  page_tsx: Type.String({
    description:
      "Full TSX module source. Must default-export a React component. Allowed imports: react, recharts, date-fns, @symphony/page-runtime ({ query }).",
  }),
  meta_toml: Type.String({
    description:
      'Full meta.toml. Required key: title. Optional: icon, position, meta_version. Example:\ntitle = "PRs this week"\nposition = 10\n',
  }),
});

const ReadPageParams = Type.Object({
  slug: Type.String(),
});

const ListPagesParams = Type.Object({});

const ListTablesParams = Type.Object({});

const DescribeTableParams = Type.Object({
  table: Type.String({ description: "SQLite table name." }),
});

function ok(text: string): AgentToolResult<unknown> {
  return {
    content: [{ type: "text", text }],
    details: undefined,
  };
}

function jsonText(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

async function fetchJson(
  input: string,
  init?: RequestInit,
): Promise<{ ok: boolean; status: number; body: unknown }> {
  const res = await fetch(input, init);
  let body: unknown = null;
  try {
    body = await res.json();
  } catch {
    /* leave null */
  }
  return { ok: res.ok, status: res.status, body };
}

export const queryTool: AgentTool<typeof QueryParams> = {
  name: "query_sql",
  label: "SQL query",
  description:
    "Run a read-only SELECT against the local SQLite store. Connection is opened in SQLITE_OPEN_READ_ONLY mode; PRAGMAs and writes are rejected. Returns columns, rows (array-of-arrays), and a `truncated` flag (row cap = 10000, timeout = 5s).",
  parameters: QueryParams,
  execute: async (
    _id: string,
    params: Static<typeof QueryParams>,
  ): Promise<AgentToolResult<unknown>> => {
    const r = await fetchJson("/api/sql/query", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ sql: params.sql, params: params.params ?? [] }),
    });
    if (!r.ok) {
      throw new Error(`query failed (${r.status}): ${asError(r.body)}`);
    }
    return ok(jsonText(r.body));
  },
};

export const listTablesTool: AgentTool<typeof ListTablesParams> = {
  name: "list_tables",
  label: "List tables",
  description:
    "List user tables in the SQLite store (excludes sqlite_* internal tables).",
  parameters: ListTablesParams,
  execute: async (): Promise<AgentToolResult<unknown>> => {
    const r = await fetchJson("/api/sql/query", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        sql: "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        params: [],
      }),
    });
    if (!r.ok) throw new Error(asError(r.body));
    return ok(jsonText(r.body));
  },
};

export const describeTableTool: AgentTool<typeof DescribeTableParams> = {
  name: "describe_table",
  label: "Describe table",
  description:
    "Return PRAGMA table_info equivalent (columns, types, nullability, primary key) for a table. Use before writing queries against unfamiliar tables.",
  parameters: DescribeTableParams,
  execute: async (
    _id: string,
    params: Static<typeof DescribeTableParams>,
  ): Promise<AgentToolResult<unknown>> => {
    // table_info() takes the name as an argument, not a bind param. We can't
    // bind table identifiers, so quote-escape and inline carefully.
    const safe = params.table.replace(/"/g, '""');
    const r = await fetchJson("/api/sql/query", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        sql: `SELECT name, type, "notnull" AS not_null, dflt_value AS default_value, pk FROM pragma_table_info("${safe}")`,
        params: [],
      }),
    });
    if (!r.ok) throw new Error(asError(r.body));
    return ok(jsonText(r.body));
  },
};

export const listPagesTool: AgentTool<typeof ListPagesParams> = {
  name: "list_pages",
  label: "List pages",
  description: "List all pages currently registered (slug, title, parse errors).",
  parameters: ListPagesParams,
  execute: async (): Promise<AgentToolResult<unknown>> => {
    const r = await fetchJson("/api/pages");
    if (!r.ok) throw new Error(asError(r.body));
    const data = r.body as { pages?: unknown };
    return ok(jsonText(data.pages ?? []));
  },
};

export const readPageTool: AgentTool<typeof ReadPageParams> = {
  name: "read_page",
  label: "Read page",
  description:
    "Fetch the current page.tsx source plus its meta.toml-derived record for a given slug. Use this before editing.",
  parameters: ReadPageParams,
  execute: async (
    _id: string,
    params: Static<typeof ReadPageParams>,
  ): Promise<AgentToolResult<unknown>> => {
    const slug = encodeURIComponent(params.slug);
    const r = await fetchJson(`/api/pages/${slug}`);
    if (!r.ok) throw new Error(asError(r.body));
    return ok(jsonText(r.body));
  },
};

export const writePageTool: AgentTool<typeof WritePageParams> = {
  name: "write_page",
  label: "Write page",
  description:
    "Create or replace pages/<slug>/page.tsx and pages/<slug>/meta.toml. Both files are written atomically. The fs watcher refreshes the registry; the new page appears in the sidebar. Returns the resulting page record.",
  parameters: WritePageParams,
  execute: async (
    _id: string,
    params: Static<typeof WritePageParams>,
  ): Promise<AgentToolResult<unknown>> => {
    const r = await fetchJson("/api/pages/write", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(params),
    });
    if (!r.ok) throw new Error(asError(r.body));
    return ok(jsonText(r.body));
  },
};

function asError(body: unknown): string {
  if (body && typeof body === "object" && "error" in body) {
    return String((body as { error: unknown }).error);
  }
  return JSON.stringify(body ?? null);
}

export function defaultPagesTools(): AgentTool[] {
  return [
    queryTool,
    listTablesTool,
    describeTableTool,
    listPagesTool,
    readPageTool,
    writePageTool,
  ];
}

export const PAGES_SYSTEM_PROMPT = `You help the user build "pages" — small React modules that render dashboards over the local SQLite store inside Meridian.

Each page lives at \`pages/<slug>/page.tsx\` (the component) plus \`pages/<slug>/meta.toml\` (sidebar metadata). The user's working folder is already configured; you author by calling the \`write_page\` tool with both file contents.

────────────────────────────────────────────────────────────────────────
\`page.tsx\` contract — sandboxed iframe, pinned runtime:

- Default-export a React functional component.
- Allowed imports (anything else throws at runtime):
    import { useEffect, useMemo, useState } from "react";
    import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer,
             LineChart, Line, PieChart, Pie, Cell, CartesianGrid, Legend } from "recharts";
    import { format, parseISO, subDays } from "date-fns";
    import { query } from "@symphony/page-runtime";
- \`query(sql, params?)\` is async and returns \`{ columns: string[]; rows: (string|number|null)[][]; truncated: boolean }\`. Read-only. Row cap 10000. 5s timeout.
- No \`fetch\`, \`localStorage\`, \`WebSocket\`, etc. — those globals are nullified.
- Style with CSS variables to match the host theme: \`var(--bg)\`, \`var(--text)\`, \`var(--textDim)\`, \`var(--textMute)\`, \`var(--border)\`, \`var(--accent)\`, \`var(--panel)\`, \`var(--panel2)\`.
- Show empty / loading / error states explicitly.

────────────────────────────────────────────────────────────────────────
\`meta.toml\` schema:

  title         = string                     (required)
  icon          = string                     (optional: "bar-chart", "line-chart", "pie-chart", "table")
  position      = integer                    (optional, sidebar order; lower = higher)
  meta_version  = 1                          (always 1 today)

────────────────────────────────────────────────────────────────────────
Workflow:

1. Use \`list_tables\` and \`describe_table\` to discover the schema.
2. Sketch a query, validate with \`query_sql\` to confirm the shape.
3. Pick a slug (lowercase, dashes — e.g. \`prs-this-week\`). Confirm there's no clash with \`list_pages\`.
4. Call \`write_page\` with both files.
5. The page appears in the sidebar; the user can open it.

For edits, first call \`read_page\` to fetch the current source, then \`write_page\` with the modified content.

────────────────────────────────────────────────────────────────────────
Style:
- Keep components small and self-contained.
- Aggregate in SQL, not in JS. Smaller payloads, faster pages.
- Don't add features the user didn't ask for.
- Don't write comments that just restate the code.
`;

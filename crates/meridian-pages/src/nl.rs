//! Generate the inbox-spec body shown to a coding harness when the user
//! types a natural-language page request. Mirrors `meridian-automations::nl`
//! but the deliverable is a `pages/<slug>/{page.tsx,meta.toml}` pair, not a
//! TOML automation.
//!
//! The spec stands alone — the page contract (allowed imports, query() shim,
//! sandbox constraints) is documented inline so the harness has everything
//! it needs without "go read X" pointers.

pub struct GeneratedSpec {
    pub slug: String,
    pub title: String,
    pub body: String,
}

pub fn generate(nl: &str) -> GeneratedSpec {
    let trimmed = nl.trim();
    let slug = slugify(trimmed);
    let title = format!("Page request: {}", short_title(trimmed));
    let body = render_spec(trimmed, &slug);
    GeneratedSpec { slug, title, body }
}

pub fn generate_fix(slug: &str, error: &str, source_excerpt: &str) -> GeneratedSpec {
    let title = format!("Fix page: {slug}");
    let body = format!(
        r#"[Page fix request]

The page `{slug}` failed to render. Update `pages/{slug}/page.tsx` so it
renders cleanly and address the underlying error.

──────────────────────────────────────────────────────────────────────
Error reported by the iframe:

{error}

──────────────────────────────────────────────────────────────────────
Current `page.tsx` (truncated to 4 KB):

{source_excerpt}

──────────────────────────────────────────────────────────────────────
Page contract reminder:

- Default-export a React component.
- Allowed imports only: `react`, `recharts`, `date-fns`, and
  `{{ query }}` from the virtual module `@symphony/page-runtime`.
- `query(sql, params?)` is async and returns
  `{{ columns: string[], rows: Value[][], truncated: boolean }}`.
- No `fetch`, no filesystem, no DOM globals beyond what React gives you.
- Match the host app's visual language (dark/light theme via CSS vars
  `--bg`, `--text`, `--textDim`, `--border`).
"#,
        slug = slug,
        error = error,
        source_excerpt = source_excerpt,
    );
    GeneratedSpec {
        slug: format!("fix-{slug}"),
        title,
        body,
    }
}

fn short_title(nl: &str) -> String {
    let s = nl.replace('\n', " ");
    if s.len() <= 80 {
        s
    } else {
        format!("{}…", &s[..80])
    }
}

fn slugify(nl: &str) -> String {
    let mut s = String::new();
    let mut prev_dash = false;
    for c in nl.chars().take(80) {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !s.is_empty() {
            s.push('-');
            prev_dash = true;
        }
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "page".into()
    } else {
        s
    }
}

fn render_spec(nl: &str, slug: &str) -> String {
    format!(
        r#"[Page request]

User said: "{nl}"

Write two files:
  pages/{slug}/page.tsx   — the React component (default export)
  pages/{slug}/meta.toml  — the sidebar metadata

──────────────────────────────────────────────────────────────────────
`page.tsx` contract (sandboxed iframe; the host bundles a pinned runtime):

- Default-export a React functional component.
- Allowed imports — anything else fails to resolve:
    import {{ useEffect, useMemo, useState }} from "react";
    import {{ BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer,
              LineChart, Line, PieChart, Pie, Cell, CartesianGrid,
              Legend }} from "recharts";
    import {{ format, parseISO, subDays }} from "date-fns";
    import {{ query }} from "@symphony/page-runtime";
- `query(sql, params?)` is async; returns
    {{ columns: string[], rows: (string|number|null)[][], truncated: boolean }}.
  Connection is read-only: the LLM cannot mutate state. Row cap ~10000.
- No `fetch`, no `localStorage`, no globals beyond what React gives you.
- Use CSS variables for colors so the page matches the host theme:
    `var(--bg)`, `var(--text)`, `var(--textDim)`, `var(--textMute)`,
    `var(--border)`, `var(--accent)`, `var(--panel)`.
- Show empty/loading/error states explicitly. The runtime's error
  boundary will catch crashes and surface a "Fix this" button, but
  graceful UX still beats a stack trace.

──────────────────────────────────────────────────────────────────────
SQLite schema available to `query()`:

The store backs the desktop app — issues, automations, harnesses, repos,
pages themselves. To discover tables before writing a query, run:
  SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;
Or sample a table with:
  SELECT * FROM <table> LIMIT 5;

Useful starter tables:
  issue (id, identifier, title, state_id, created_at, updated_at, ...)
  workflow_state (id, type, name)
  repo (slug, primary_language, is_private, connected, ...)
  automation (id, name, enabled, last_run_at, ...)
  automation_run (automation_id, started_at, status, ...)
  harness (id, name, available, ...)

──────────────────────────────────────────────────────────────────────
`meta.toml` schema:

  title       = string                  (req)
  icon        = "bar-chart" | "line-chart" | "pie-chart" | "table"  (opt)
  position    = integer                  (sidebar order, lower = higher)
  meta_version = 1                       (always 1 today)

──────────────────────────────────────────────────────────────────────
Example skeleton — `pages/{slug}/page.tsx`:

  import {{ useEffect, useState }} from "react";
  import {{ query }} from "@symphony/page-runtime";
  import {{ BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer }} from "recharts";

  export default function Page() {{
    const [data, setData] = useState<{{name: string; n: number}}[] | null>(null);
    const [error, setError] = useState<string | null>(null);
    useEffect(() => {{
      query(
        "SELECT state_id AS name, COUNT(*) AS n FROM issue GROUP BY state_id",
      )
        .then((res) => setData(res.rows.map(([name, n]) => ({{ name: String(name), n: Number(n) }}))))
        .catch((e) => setError(String(e)));
    }}, []);
    if (error) return <div style={{{{ color: "var(--textMute)" }}}}>{{error}}</div>;
    if (!data) return <div style={{{{ color: "var(--textMute)" }}}}>loading…</div>;
    if (data.length === 0) return <div style={{{{ color: "var(--textMute)" }}}}>no data</div>;
    return (
      <div style={{{{ padding: 20, color: "var(--text)" }}}}>
        <h1 style={{{{ marginTop: 0 }}}}>Issues by state</h1>
        <div style={{{{ width: "100%", height: 320 }}}}>
          <ResponsiveContainer>
            <BarChart data={{data}}>
              <XAxis dataKey="name" stroke="var(--textDim)" />
              <YAxis stroke="var(--textDim)" />
              <Tooltip />
              <Bar dataKey="n" fill="var(--accent, #10b981)" />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>
    );
  }}

──────────────────────────────────────────────────────────────────────
Rules:
- Pick a sensible title from the user's phrasing.
- Don't wrap query() output in additional fetches; the shim *is* the
  data layer.
- If a chart needs aggregation, do it in SQL (faster, less data over the
  wire).
- Don't add features the user didn't ask for. Minimal page first.
"#,
        nl = nl.replace('"', "\\\""),
        slug = slug,
    )
}

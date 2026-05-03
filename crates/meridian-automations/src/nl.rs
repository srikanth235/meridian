//! Generate the inbox-spec body shown to a coding harness when the user
//! types a natural-language automation request.
//!
//! Critical: the spec must stand alone. We inline the SDK `.d.ts` (regenerated
//! on every build via `include_str!`) and a worked example so the harness has
//! everything it needs without "go read X" pointers. Keyword-matched trimming
//! is in scope but trivial today (we always include the full SDK); we leave
//! the function shape ready to specialize when the SDK grows.

use crate::assets::sdk_index_dts;

pub struct GeneratedSpec {
    pub slug: String,
    pub title: String,
    pub body: String,
}

pub fn generate(nl: &str) -> GeneratedSpec {
    let trimmed = nl.trim();
    let slug = slugify(trimmed);
    let title = format!("Automation request: {}", short_title(trimmed));
    let body = render_spec(trimmed, &slug);
    GeneratedSpec { slug, title, body }
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
        "automation".into()
    } else {
        s
    }
}

fn render_spec(nl: &str, slug: &str) -> String {
    let dts = sdk_index_dts();
    format!(
        r#"[Automation request]

User said: "{nl}"

Write a TypeScript file at: automations/{slug}.ts

The file must default-export defineAutomation(). Do not run the script —
symphony picks it up via filesystem watch when the file lands.

──────────────────────────────────────────────────────────────────────
SDK (from "@symphony/automation"):

{dts}

──────────────────────────────────────────────────────────────────────
Example — "every hour, open new PRs from my org as tabs":

  import {{ defineAutomation, symphony }} from "@symphony/automation";

  export default defineAutomation({{
    name: "New PRs from acme",
    schedule: {{ every: "1h" }},
    async run(ctx) {{
      const prs = await symphony.github.prs({{
        repos: ["acme/api", "acme/web"],
        state: "open",
        updatedSince: ctx.lastRunAt,
      }});
      for (const pr of prs) {{
        await symphony.tabs.open({{
          url: pr.url,
          title: `${{pr.repo}}#${{pr.number}} ${{pr.title}}`,
          dedupKey: pr.url,
        }});
      }}
    }},
  }});

──────────────────────────────────────────────────────────────────────
Rules:
- Pick a sensible schedule from the user's phrasing
  ("every morning" → {{ cron: "0 9 * * *" }}, "every hour" → {{ every: "1h" }}).
- Always pass ctx.lastRunAt to updatedSince so reruns stay incremental.
- Always set dedupKey, typically to the item's URL.
- Don't add fields the user didn't ask for. No extra logging, no error
  swallowing — if a fetch fails, let it throw; the runtime records it.
"#,
        nl = nl.replace('"', "\\\""),
        slug = slug,
        dts = dts,
    )
}

//! Generate the inbox-spec body shown to a coding harness when the user
//! types a natural-language automation request.
//!
//! The spec must stand alone — it documents the entire TOML schema inline so
//! the harness has everything it needs without "go read X" pointers.

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
    format!(
        r#"[Automation request]

User said: "{nl}"

Write a TOML file at: automations/{slug}.toml

The file is declarative — no scripting. The runtime parses it and runs:
  fetch source → for each item, render template fields → dispatch action.

──────────────────────────────────────────────────────────────────────
Schema:

  name      = string                       — human label shown in the UI
  schedule  = {{ every = "1h" | "6h" | "1d" }}
              | {{ cron  = "MIN HOUR DOM MONTH DOW" }}

  [source]
    kind                     = "github.issues" | "github.prs"
    repos                    = ["owner/repo", ...]      # optional
    state                    = "open" | "closed" | "any" # optional, default "open"
    labels                   = ["label", ...]            # optional
    assignee                 = "@me" | "<login>"         # optional
    updated_since_last_run   = true | false              # optional
                                                         # if true, GitHub
                                                         # query is filtered
                                                         # to items updated
                                                         # since the previous
                                                         # successful run

  [action]
    kind       = "inbox.create" | "tabs.open"
    # template fields (Liquid syntax: `{{{{ item.<field> }}}}`):
    #   inbox.create: title (req), body, url, tags = [...], dedup_key (req)
    #   tabs.open:    url (req), title, dedup_key (req)

Item fields available in templates:
  item.title       string
  item.url         string
  item.repo        string  (e.g. "acme/api")
  item.number      integer
  item.author      string  (login)
  item.labels      array of strings
  item.updatedAt   ISO8601 string

Other variables: {{{{ lastRunAt }}}}, {{{{ now }}}} (ISO8601 strings).

──────────────────────────────────────────────────────────────────────
Example — "every hour, open new PRs from my org as tabs":

  name = "New PRs from acme"
  schedule = {{ every = "1h" }}

  [source]
  kind = "github.prs"
  repos = ["acme/api", "acme/web"]
  state = "open"
  updated_since_last_run = true

  [action]
  kind = "tabs.open"
  url = "{{{{ item.url }}}}"
  title = "{{{{ item.repo }}}}#{{{{ item.number }}}} {{{{ item.title }}}}"
  dedup_key = "{{{{ item.url }}}}"

──────────────────────────────────────────────────────────────────────
Rules:
- Pick a sensible schedule from the user's phrasing
  ("every morning" → {{ cron = "0 9 * * *" }}, "every hour" → {{ every = "1h" }}).
- Set updated_since_last_run = true unless the user explicitly wants a full
  scan every time.
- Always set dedup_key, typically to {{{{ item.url }}}}.
- Don't add fields the user didn't ask for — this is declarative; there's no
  escape hatch. If the request needs custom logic the schema can't express,
  reply in the inbox saying so.
"#,
        nl = nl.replace('"', "\\\""),
        slug = slug,
    )
}

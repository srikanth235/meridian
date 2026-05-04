// Sandboxed iframe runtime for LLM-authored pages.
//
// Loaded via <iframe src="/page-runtime.html" sandbox="allow-scripts">
// (no allow-same-origin → unique opaque origin → no DOM access to parent,
// no access to host cookies/localStorage). Communicates with the parent
// purely through `postMessage`.
//
// Protocol:
//   parent → iframe: { type: "mount", slug, source, theme }
//   parent → iframe: { type: "theme", theme }
//   iframe → parent: { type: "ready" }                       — on load
//   iframe → parent: { type: "error", error }                — render failure
//   iframe → parent: { type: "query", id, sql, params }      — SQL request
//   parent → iframe: { type: "query-result", id, ok, data, error }
//
// The iframe never makes its own network requests — `fetch` is replaced with
// a thrower at boot. All data goes through the parent's read-only SQLite.

import * as React from "react";
import { createRoot, type Root } from "react-dom/client";
import * as Recharts from "recharts";
import * as DateFns from "date-fns";
import * as Babel from "@babel/standalone";

type Theme = "dark" | "light";

interface MountMsg {
  type: "mount";
  slug: string;
  source: string;
  theme?: Theme;
}

interface ThemeMsg {
  type: "theme";
  theme: Theme;
}

interface QueryResultMsg {
  type: "query-result";
  id: string;
  ok: boolean;
  data?: { columns: string[]; rows: unknown[][]; truncated: boolean };
  error?: string;
}

type ParentMsg = MountMsg | ThemeMsg | QueryResultMsg;

const root = document.getElementById("root")!;
let reactRoot: Root | null = null;
let mountedSlug: string | null = null;

// Suppress all network access. The page contract forbids fetch/XHR — the
// only data path is `query()`, which routes through the parent.
function harden() {
  const block = (name: string) => {
    return () => {
      throw new Error(
        `${name} is not available in pages. Use query() to read data from SQLite.`,
      );
    };
  };
  const w = window as unknown as Record<string, unknown>;
  try {
    w.fetch = block("fetch");
    w.XMLHttpRequest = block("XMLHttpRequest");
    w.WebSocket = block("WebSocket");
    w.EventSource = block("EventSource");
  } catch {
    /* environments where these can't be reassigned just won't be locked */
  }
}

// query() shim used by every page. The mounted module's import of
// "@symphony/page-runtime" is rewritten by the loader to look at this object.
const pendingQueries = new Map<
  string,
  {
    resolve: (v: { columns: string[]; rows: unknown[][]; truncated: boolean }) => void;
    reject: (e: Error) => void;
  }
>();

function query(
  sql: string,
  params: unknown[] = [],
): Promise<{ columns: string[]; rows: unknown[][]; truncated: boolean }> {
  if (typeof sql !== "string") {
    return Promise.reject(new Error("query(sql, params?): sql must be a string"));
  }
  const id = Math.random().toString(36).slice(2);
  return new Promise((resolve, reject) => {
    pendingQueries.set(id, { resolve, reject });
    window.parent.postMessage({ type: "query", id, sql, params }, "*");
  });
}

const RUNTIME = {
  query,
};

// Each module loaded into the iframe gets its own scope. We expose a tiny
// require() that resolves only the four whitelisted module specifiers.
function makeRequire() {
  return (specifier: string) => {
    switch (specifier) {
      case "react":
        return React;
      case "recharts":
        return Recharts;
      case "date-fns":
        return DateFns;
      case "@symphony/page-runtime":
        return RUNTIME;
      default:
        throw new Error(
          `Module "${specifier}" is not available. Allowed: react, recharts, date-fns, @symphony/page-runtime`,
        );
    }
  };
}

function transform(source: string): string {
  // Babel-standalone transforms TSX → ES5 with CommonJS-style exports so we
  // can wrap and run via `new Function`. presets: typescript strips types,
  // react converts JSX, env transforms modules to commonjs.
  const out = Babel.transform(source, {
    filename: "page.tsx",
    presets: [
      ["typescript", { allExtensions: true, isTSX: true }],
      ["react", { runtime: "classic" }],
      ["env", { modules: "commonjs", targets: { esmodules: true } }],
    ],
  });
  if (!out.code) throw new Error("Babel produced no output");
  return out.code;
}

function evaluate(code: string): unknown {
  const module = { exports: {} as Record<string, unknown> };
  const exports = module.exports;
  const req = makeRequire();
  // eslint-disable-next-line no-new-func
  const fn = new Function("module", "exports", "require", code);
  fn(module, exports, req);
  return module.exports;
}

class ErrorBoundary extends React.Component<
  { children: React.ReactNode; onError: (e: Error) => void },
  { error: Error | null }
> {
  constructor(props: { children: React.ReactNode; onError: (e: Error) => void }) {
    super(props);
    this.state = { error: null };
  }
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  componentDidCatch(error: Error) {
    this.props.onError(error);
  }
  render() {
    if (this.state.error) {
      return React.createElement(
        "div",
        { className: "page-runtime-error" },
        `Runtime error: ${this.state.error.message}`,
      );
    }
    return this.props.children as React.ReactElement;
  }
}

function showError(message: string) {
  // Replace the React tree with a plain DOM error block — easier to recover
  // from than letting React try again with a broken module.
  if (reactRoot) {
    try {
      reactRoot.unmount();
    } catch {
      /* ignore */
    }
    reactRoot = null;
  }
  root.innerHTML = "";
  const el = document.createElement("div");
  el.className = "page-runtime-error";
  el.textContent = message;
  root.appendChild(el);
}

function reportError(error: Error) {
  const message = `${error.message}\n\n${error.stack ?? ""}`;
  window.parent.postMessage({ type: "error", error: message }, "*");
  showError(message);
}

function applyTheme(theme: Theme) {
  if (theme === "light") document.documentElement.classList.add("theme-light");
  else document.documentElement.classList.remove("theme-light");
}

function mount(msg: MountMsg) {
  applyTheme(msg.theme ?? "dark");
  mountedSlug = msg.slug;
  try {
    const transformed = transform(msg.source);
    const exports = evaluate(transformed) as Record<string, unknown>;
    const Component = (exports.default ?? exports) as unknown;
    if (typeof Component !== "function") {
      throw new Error(
        "page.tsx must default-export a React component (function).",
      );
    }
    if (!reactRoot) {
      root.innerHTML = "";
      reactRoot = createRoot(root);
    }
    const child = React.createElement(Component as React.ComponentType, null);
    const props: { children: React.ReactNode; onError: (e: Error) => void } = {
      children: child,
      onError: reportError,
    };
    reactRoot.render(React.createElement(ErrorBoundary, props));
  } catch (e) {
    reportError(e instanceof Error ? e : new Error(String(e)));
  }
}

window.addEventListener("message", (ev: MessageEvent<ParentMsg>) => {
  const msg = ev.data;
  if (!msg || typeof msg !== "object") return;
  switch (msg.type) {
    case "mount":
      mount(msg);
      break;
    case "theme":
      applyTheme(msg.theme);
      break;
    case "query-result": {
      const pending = pendingQueries.get(msg.id);
      if (!pending) return;
      pendingQueries.delete(msg.id);
      if (msg.ok && msg.data) pending.resolve(msg.data);
      else pending.reject(new Error(msg.error ?? "query failed"));
      break;
    }
  }
});

window.addEventListener("error", (ev) => {
  reportError(ev.error instanceof Error ? ev.error : new Error(ev.message));
});
window.addEventListener("unhandledrejection", (ev) => {
  const r = ev.reason;
  reportError(r instanceof Error ? r : new Error(String(r)));
});

harden();
window.parent.postMessage({ type: "ready" }, "*");

// Quiet "unused mountedSlug" — kept for future use (e.g. relative imports).
void mountedSlug;

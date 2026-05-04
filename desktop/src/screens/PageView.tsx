import { useEffect, useRef, useState } from "react";
import type { Page } from "../types";
import { Card } from "../atoms";
import { IconChevronLeft, IconRetry } from "../icons";

const RUNTIME_URL = "/page-runtime.html";

interface Props {
  slug: string;
  onBack: () => void;
}

interface IframeMsg {
  type: "ready" | "error" | "query";
  id?: string;
  error?: string;
  sql?: string;
  params?: unknown[];
}

export function PageView({ slug, onBack }: Props) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [page, setPage] = useState<Page | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [iframeReady, setIframeReady] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);
  const [submittingFix, setSubmittingFix] = useState(false);
  const [fixSubmitted, setFixSubmitted] = useState<string | null>(null);

  // Fetch the page record + source.
  useEffect(() => {
    let cancelled = false;
    setLoadError(null);
    setRuntimeError(null);
    setPage(null);
    setSource(null);
    fetch(`/api/pages/${encodeURIComponent(slug)}`)
      .then(async (r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((j) => {
        if (cancelled) return;
        setPage(j.page);
        setSource(j.source);
      })
      .catch((e) => {
        if (cancelled) return;
        setLoadError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [slug, reloadKey]);

  // Once both the iframe is ready *and* we have source, post a mount message.
  // The iframe won't have transformed/rendered anything until this lands.
  useEffect(() => {
    if (!iframeReady || !source) return;
    const win = iframeRef.current?.contentWindow;
    if (!win) return;
    const theme =
      typeof document !== "undefined" &&
      document.documentElement.classList.contains("theme-light")
        ? "light"
        : "dark";
    win.postMessage({ type: "mount", slug, source, theme }, "*");
  }, [iframeReady, source, slug, reloadKey]);

  // postMessage hub: ready → unblock; error → surface; query → run on backend
  // and post the result back. We only listen to messages whose source is our
  // iframe to avoid cross-talk with anything else on the page.
  useEffect(() => {
    function onMsg(ev: MessageEvent<IframeMsg>) {
      if (ev.source !== iframeRef.current?.contentWindow) return;
      const msg = ev.data;
      if (!msg || typeof msg !== "object") return;
      if (msg.type === "ready") {
        setIframeReady(true);
        setRuntimeError(null);
      } else if (msg.type === "error") {
        setRuntimeError(msg.error ?? "unknown error");
      } else if (msg.type === "query") {
        runQuery(slug, msg).then((reply) => {
          iframeRef.current?.contentWindow?.postMessage(reply, "*");
        });
      }
    }
    window.addEventListener("message", onMsg);
    return () => window.removeEventListener("message", onMsg);
  }, [slug]);

  function reload() {
    setIframeReady(false);
    setRuntimeError(null);
    setFixSubmitted(null);
    setReloadKey((k) => k + 1);
  }

  async function submitFix() {
    if (!runtimeError || submittingFix) return;
    setSubmittingFix(true);
    try {
      const res = await fetch(`/api/pages/${encodeURIComponent(slug)}/fix`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ error: runtimeError }),
      });
      if (!res.ok) {
        setFixSubmitted(`error: ${res.statusText}`);
      } else {
        setFixSubmitted("Spec drafted into your inbox.");
      }
    } finally {
      setSubmittingFix(false);
    }
  }

  return (
    <div style={{ padding: 20, display: "flex", flexDirection: "column", gap: 16, height: "100%", minHeight: 0 }}>
      <div className="flex items-center gap-3">
        <button
          onClick={onBack}
          className="inline-flex items-center gap-1 text-[12px] text-textDim hover:text-text border border-border rounded-md cursor-pointer"
          style={{ height: 26, padding: "0 8px", background: "transparent" }}
        >
          <IconChevronLeft size={12} /> Pages
        </button>
        <div className="flex-1 min-w-0">
          <div className="text-[16px] font-semibold text-text truncate">
            {page?.title ?? slug}
          </div>
          <div className="text-[11px] text-textMute font-mono truncate">{slug}</div>
        </div>
        <button
          onClick={reload}
          title="Reload"
          className="inline-flex items-center gap-1 text-[12px] text-textDim hover:text-text border border-border rounded-md cursor-pointer"
          style={{ height: 26, padding: "0 8px", background: "transparent" }}
        >
          <IconRetry size={12} /> Reload
        </button>
      </div>

      {loadError ? (
        <Card>
          <div style={{ padding: 16, color: "#ef4444" }}>
            Failed to load page: {loadError}
          </div>
        </Card>
      ) : !page ? (
        <Card>
          <div style={{ padding: 16, color: "var(--textMute)" }}>loading…</div>
        </Card>
      ) : page.parse_error ? (
        <Card>
          <div style={{ padding: 16 }}>
            <div className="text-[13px] font-semibold" style={{ color: "#ef4444" }}>
              meta.toml parse error
            </div>
            <pre
              style={{
                marginTop: 8,
                fontSize: 11,
                fontFamily: "ui-monospace, monospace",
                color: "var(--text)",
                whiteSpace: "pre-wrap",
              }}
            >
              {page.parse_error}
            </pre>
          </div>
        </Card>
      ) : (
        <>
          {runtimeError && (
            <Card>
              <div style={{ padding: 12 }}>
                <div className="flex items-center gap-3">
                  <span
                    className="w-1.5 h-1.5 rounded-full shrink-0"
                    style={{ background: "#ef4444" }}
                  />
                  <div className="flex-1 text-[12px] font-medium text-text">
                    Page failed to render
                  </div>
                  <button
                    onClick={submitFix}
                    disabled={submittingFix}
                    className="text-[11px] font-medium border border-border rounded-md cursor-pointer disabled:opacity-50"
                    style={{
                      padding: "3px 8px",
                      color: "var(--accent)",
                      borderColor: "var(--accent)",
                    }}
                  >
                    {submittingFix ? "Drafting…" : "Fix this"}
                  </button>
                  <button
                    onClick={reload}
                    className="text-[11px] font-medium text-textDim hover:text-text border border-border rounded-md cursor-pointer"
                    style={{ padding: "3px 8px" }}
                  >
                    Retry
                  </button>
                </div>
                {fixSubmitted && (
                  <div className="text-[11px] text-textMute mt-2">{fixSubmitted}</div>
                )}
                <pre
                  style={{
                    marginTop: 10,
                    padding: 10,
                    background: "var(--panel2)",
                    borderRadius: 6,
                    fontSize: 11,
                    color: "var(--textDim)",
                    maxHeight: 180,
                    overflow: "auto",
                    whiteSpace: "pre-wrap",
                    fontFamily: "ui-monospace, monospace",
                  }}
                >
                  {runtimeError}
                </pre>
              </div>
            </Card>
          )}
          <div
            style={{
              flex: 1,
              minHeight: 0,
              borderRadius: 8,
              border: "1px solid var(--border)",
              overflow: "hidden",
              background: "var(--bg)",
            }}
          >
            <iframe
              key={reloadKey}
              ref={iframeRef}
              src={RUNTIME_URL}
              sandbox="allow-scripts"
              title={page.title}
              style={{ width: "100%", height: "100%", border: 0, background: "transparent" }}
            />
          </div>
        </>
      )}
    </div>
  );
}

async function runQuery(
  slug: string,
  msg: IframeMsg,
): Promise<{
  type: "query-result";
  id: string;
  ok: boolean;
  data?: unknown;
  error?: string;
}> {
  const id = msg.id ?? "";
  try {
    const res = await fetch(`/api/pages/${encodeURIComponent(slug)}/query`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ sql: msg.sql ?? "", params: msg.params ?? [] }),
    });
    const j = await res.json();
    if (!res.ok) {
      return { type: "query-result", id, ok: false, error: j.error ?? res.statusText };
    }
    return { type: "query-result", id, ok: true, data: j };
  } catch (e) {
    return { type: "query-result", id, ok: false, error: String(e) };
  }
}

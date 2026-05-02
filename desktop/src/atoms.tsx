// Meridian atoms — small visual primitives shared across screens.
import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import type { Harness, HarnessId } from "./types";
import { HARNESS_LOGOS } from "./harnessLogos";
import { IconSearch } from "./icons";

export type SymStatus = "running" | "queued" | "review" | "failed" | "merged";

export const STATUS_META: Record<SymStatus, { color: string; label: string; pulse: boolean }> = {
  running: { color: "#10b981", label: "Running",   pulse: true  },
  queued:  { color: "#9a9a9a", label: "Queued",    pulse: false },
  review:  { color: "#3b82f6", label: "In review", pulse: false },
  failed:  { color: "#ef4444", label: "Failed",    pulse: false },
  merged:  { color: "#a855f7", label: "Merged",    pulse: false },
};

export function StatusDot({ status, size = 7 }: { status: SymStatus; size?: number }) {
  const meta = STATUS_META[status];
  return (
    <span style={{ position: "relative", display: "inline-flex", width: size, height: size, flexShrink: 0 }}>
      <span style={{ width: size, height: size, borderRadius: "50%", background: meta.color }} />
      {meta.pulse && (
        <span
          style={{
            position: "absolute",
            inset: 0,
            borderRadius: "50%",
            background: meta.color,
            animation: "sym-pulse 1.6s ease-out infinite",
          }}
        />
      )}
    </span>
  );
}

export function Pill({
  children,
  tone = "muted",
  mono = false,
}: {
  children: ReactNode;
  tone?: "ok" | "warn" | "err" | "blue" | "purple" | "muted";
  mono?: boolean;
}) {
  return (
    <span
      className={`pill pill-${tone}`}
      style={{ fontFamily: mono ? "var(--font-mono, JetBrains Mono, ui-monospace, monospace)" : undefined }}
    >
      {children}
    </span>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <span
      className="font-mono"
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        minWidth: 18,
        height: 18,
        padding: "0 5px",
        background: "var(--panel3)",
        color: "var(--textDim)",
        border: "1px solid var(--border)",
        borderRadius: 4,
        fontSize: 10,
        fontWeight: 500,
      }}
    >
      {children}
    </span>
  );
}

export function Sparkline({
  data,
  color = "#10b981",
  w = 88,
  h = 24,
}: {
  data: number[];
  color?: string;
  w?: number;
  h?: number;
}) {
  if (data.length === 0) return null;
  const max = Math.max(...data, 1);
  const pts = data
    .map((v, i) => {
      const x = (i / Math.max(data.length - 1, 1)) * w;
      const y = h - (v / max) * (h - 2) - 1;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  const area = `0,${h} ${pts} ${w},${h}`;
  return (
    <svg width={w} height={h} style={{ display: "block" }}>
      <polygon points={area} fill={color} fillOpacity="0.12" />
      <polyline
        points={pts}
        fill="none"
        stroke={color}
        strokeWidth="1.25"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function Progress({
  value,
  color = "#10b981",
  h = 3,
}: {
  value: number;
  color?: string;
  h?: number;
}) {
  return (
    <div style={{ width: "100%", height: h, background: "var(--borderS)", borderRadius: 999, overflow: "hidden" }}>
      <div
        style={{
          width: `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%`,
          height: "100%",
          background: color,
          borderRadius: 999,
          transition: "width .3s ease",
        }}
      />
    </div>
  );
}

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={`bg-panel border border-border rounded-lg overflow-hidden ${className}`}
    >
      {children}
    </div>
  );
}

export function CardHead({
  title,
  right,
}: {
  title: string;
  right?: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between px-3.5 py-3 border-b border-borderS">
      <div className="text-[12px] font-semibold text-text tracking-tight">{title}</div>
      {right}
    </div>
  );
}

export function PrimaryBtn({
  icon,
  children,
  onClick,
}: {
  icon?: ReactNode;
  children: ReactNode;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="h-7 inline-flex items-center gap-1.5 rounded-md text-[12px] font-medium whitespace-nowrap shrink-0"
      style={{
        padding: icon ? "0 12px 0 9px" : "0 12px",
        background: "var(--text)",
        color: "var(--bg)",
        border: 0,
        cursor: "pointer",
      }}
    >
      {icon}
      {children}
    </button>
  );
}

export function SecondaryBtn({
  icon,
  children,
  onClick,
}: {
  icon?: ReactNode;
  children: ReactNode;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="h-7 inline-flex items-center gap-1.5 rounded-md text-[12px] font-medium bg-panel2 text-text border border-border whitespace-nowrap shrink-0 hover:bg-panel3"
      style={{ padding: icon ? "0 12px 0 9px" : "0 12px", cursor: "pointer" }}
    >
      {icon}
      {children}
    </button>
  );
}

export function TabPill({
  active,
  children,
  onClick,
}: {
  active?: boolean;
  children: ReactNode;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="h-[22px] px-2.5 rounded-md text-[11px] font-medium border-0 cursor-pointer"
      style={{ background: active ? "var(--panel3)" : "transparent", color: active ? "var(--text)" : "var(--textDim)" }}
    >
      {children}
    </button>
  );
}

export function HarnessAvatar({
  harness,
  size = 18,
  title,
}: {
  harness: Harness | undefined | null;
  size?: number;
  title?: string;
}) {
  if (!harness) {
    return (
      <span
        title={title ?? "Unassigned"}
        className="font-mono text-textMute inline-flex items-center justify-center shrink-0"
        style={{
          width: size,
          height: size,
          borderRadius: "50%",
          background: "var(--panel3)",
          border: "1px dashed var(--border)",
          fontSize: Math.max(8, size * 0.45),
        }}
      >
        ?
      </span>
    );
  }
  const logo = HARNESS_LOGOS[harness.id];
  const initials = harness.name
    .split(/[\s-]+/)
    .map((p) => p[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
  return (
    <span
      title={title ?? harness.name}
      className="text-white font-semibold inline-flex items-center justify-center shrink-0"
      style={{
        width: size,
        height: size,
        borderRadius: "50%",
        background: harness.color,
        fontSize: Math.max(8, size * 0.42),
        letterSpacing: -0.2,
        opacity: harness.available ? 1 : 0.5,
      }}
    >
      {logo ? (
        <svg
          width={Math.round(size * 0.58)}
          height={Math.round(size * 0.58)}
          viewBox={logo.viewBox}
          fill="currentColor"
          xmlns="http://www.w3.org/2000/svg"
          aria-hidden
        >
          {logo.paths.map((d, i) => (
            <path key={i} d={d} />
          ))}
        </svg>
      ) : (
        initials
      )}
    </span>
  );
}

export function harnessById(
  harnesses: Harness[] | undefined,
  id: HarnessId | null | undefined,
): Harness | undefined {
  if (!id || !harnesses) return undefined;
  return harnesses.find((h) => h.id === id);
}

/// Reusable in-page search input. Linear-style:
///   - Sized to fit alongside a page header
///   - `/` from anywhere on the page focuses it (handled by App's global key)
///   - Esc clears + blurs
///   - Shows a × clear button when non-empty
/// The component itself only manages focus + Esc; the parent owns the value.
export function PageSearch({
  value,
  onChange,
  placeholder = "Search…",
  pageId,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  /// Used by the global `/` handler to find the active page's input.
  pageId: string;
}) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // `/` focus from anywhere on the page (Linear-style). Skip when the user
      // is already typing in another input/textarea.
      if (e.key !== "/" || e.metaKey || e.ctrlKey || e.altKey) return;
      const tag = (e.target as HTMLElement | null)?.tagName?.toLowerCase();
      if (tag === "input" || tag === "textarea" || tag === "select") return;
      if ((e.target as HTMLElement | null)?.isContentEditable) return;
      e.preventDefault();
      inputRef.current?.focus();
      inputRef.current?.select();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  return (
    <div
      className="inline-flex items-center gap-2 rounded-md border border-border bg-panel2"
      style={{ paddingLeft: 8, paddingRight: 6, height: 28, minWidth: 220 }}
      data-page-search={pageId}
    >
      <span className="text-textMute"><IconSearch size={12} /></span>
      <input
        ref={inputRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            if (value) {
              onChange("");
            } else {
              inputRef.current?.blur();
            }
          }
        }}
        placeholder={placeholder}
        className="flex-1 bg-transparent border-0 outline-0 text-text text-[12px] min-w-0"
      />
      {value ? (
        <button
          type="button"
          onClick={() => { onChange(""); inputRef.current?.focus(); }}
          title="Clear"
          aria-label="Clear search"
          className="text-textMute hover:text-text cursor-pointer"
          style={{ background: "transparent", border: 0, padding: 2, lineHeight: 0 }}
        >
          ×
        </button>
      ) : (
        <Kbd>/</Kbd>
      )}
    </div>
  );
}

export function RepoChip({ repo }: { repo?: string | null }) {
  if (!repo) return null;
  const parts = repo.split("/");
  const short = parts[parts.length - 1] ?? repo;
  return (
    <span
      title={repo}
      className="font-mono text-textMute inline-flex items-center"
      style={{
        fontSize: 10.5,
        padding: "1px 6px",
        borderRadius: 4,
        background: "var(--panel3)",
        border: "1px solid var(--borderS)",
      }}
    >
      {short}
    </span>
  );
}

export function TypePill({ type }: { type?: "issue" | "pr" | null }) {
  if (!type) return null;
  return (
    <span
      className={`pill pill-${type === "pr" ? "blue" : "muted"}`}
      style={{ textTransform: "uppercase", letterSpacing: 0.5, fontSize: 9.5, padding: "1px 5px" }}
    >
      {type === "pr" ? "PR" : "Issue"}
    </span>
  );
}

export function fmtElapsed(sec: number): string {
  if (!sec || sec < 0) return "—";
  const h = Math.floor(sec / 3600),
    m = Math.floor((sec % 3600) / 60),
    s = sec % 60;
  if (h) return `${h}h ${m}m`;
  if (m) return `${m}m ${s}s`;
  return `${s}s`;
}

export function fmtNum(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

// Meridian atoms — small visual primitives shared across screens.
import type { ReactNode } from "react";

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

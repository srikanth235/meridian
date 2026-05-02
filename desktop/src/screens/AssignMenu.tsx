import { useEffect, useRef, useState } from "react";
import type { Harness, HarnessId } from "../types";
import { HarnessAvatar } from "../atoms";
import { IconCheck, IconChevronDown } from "../icons";

export function AssignMenu({
  harnesses,
  current,
  onAssign,
  size = "sm",
}: {
  harnesses: Harness[];
  current: HarnessId | null | undefined;
  onAssign: (id: HarnessId | null) => void;
  size?: "sm" | "md";
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!ref.current) return;
      if (!ref.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", onDoc);
    return () => window.removeEventListener("mousedown", onDoc);
  }, [open]);

  const currentHarness = harnesses.find((h) => h.id === current) ?? null;
  const h = size === "sm" ? 24 : 28;

  return (
    <div ref={ref} className="relative inline-block" onClick={(e) => e.stopPropagation()}>
      <button
        onClick={() => setOpen((v) => !v)}
        className="inline-flex items-center gap-1.5 rounded-md cursor-pointer text-text"
        style={{
          height: h,
          padding: "0 8px",
          background: "var(--panel2)",
          border: "1px solid var(--border)",
          fontSize: size === "sm" ? 11.5 : 12.5,
        }}
      >
        <HarnessAvatar harness={currentHarness} size={size === "sm" ? 16 : 18} />
        <span className="text-textDim">
          {currentHarness ? currentHarness.name : "Assign…"}
        </span>
        <IconChevronDown size={11} />
      </button>
      {open && (
        <div
          className="absolute z-30 right-0 top-[calc(100%+4px)] bg-panel border border-borderL rounded-md overflow-hidden"
          style={{ width: 220, boxShadow: "var(--shadow)" }}
        >
          <div className="px-3 py-2 text-[10px] uppercase tracking-[0.6px] text-textMute font-semibold border-b border-borderS">
            Assign harness
          </div>
          {harnesses.map((hh) => {
            const selected = hh.id === current;
            const full = hh.in_flight >= hh.concurrency;
            const dis = !hh.available;
            return (
              <button
                key={hh.id}
                disabled={dis}
                onClick={() => {
                  onAssign(hh.id);
                  setOpen(false);
                }}
                className="w-full flex items-center gap-2 cursor-pointer text-left"
                style={{
                  padding: "7px 10px",
                  background: selected ? "var(--panel3)" : "transparent",
                  border: 0,
                  color: dis ? "var(--textMute)" : "var(--text)",
                  opacity: dis ? 0.55 : 1,
                  cursor: dis ? "not-allowed" : "pointer",
                }}
              >
                <HarnessAvatar harness={hh} size={18} />
                <div className="flex-1 min-w-0">
                  <div className="text-[12px]">{hh.name}</div>
                  <div className="text-[10.5px] text-textMute font-mono">
                    {dis
                      ? "unavailable"
                      : full
                      ? `at cap (${hh.in_flight}/${hh.concurrency})`
                      : `${hh.in_flight}/${hh.concurrency} in flight`}
                  </div>
                </div>
                {selected && (
                  <span className="text-accent">
                    <IconCheck size={12} />
                  </span>
                )}
              </button>
            );
          })}
          {current && (
            <button
              onClick={() => {
                onAssign(null);
                setOpen(false);
              }}
              className="w-full text-left text-textMute cursor-pointer border-t border-borderS"
              style={{ padding: "7px 10px", background: "transparent", border: 0, fontSize: 11.5 }}
            >
              Unassign
            </button>
          )}
        </div>
      )}
    </div>
  );
}

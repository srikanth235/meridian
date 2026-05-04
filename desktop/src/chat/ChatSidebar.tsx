// Right-hand chat sidebar (Lovable-style). Holds the pi-web-ui ChatPanel
// custom element. The element + its underlying Agent are constructed once
// and kept alive across mount/unmount of this component, so the
// conversation persists when the user closes and reopens the sidebar.

import { useEffect, useRef, useState } from "react";
import { ensureChat } from "./setup";
import { IconX } from "../icons";

interface Props {
  onClose: () => void;
}

export function ChatSidebar({ onClose }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    ensureChat()
      .then(({ chatPanel }) => {
        if (cancelled) return;
        setLoading(false);
        if (containerRef.current && !chatPanel.parentElement) {
          containerRef.current.appendChild(chatPanel);
        } else if (
          containerRef.current &&
          chatPanel.parentElement !== containerRef.current
        ) {
          // Re-parent if it was previously mounted somewhere else.
          containerRef.current.appendChild(chatPanel);
        }
      })
      .catch((e) => {
        if (cancelled) return;
        setError(String(e));
        setLoading(false);
      });
    return () => {
      cancelled = true;
      // Don't destroy the panel — leave it detached so the next mount can
      // re-attach it with full conversation state intact.
    };
  }, []);

  return (
    <aside
      className="shrink-0 flex flex-col bg-panel border-l border-border"
      style={{ width: 460, minWidth: 360, maxWidth: 720, height: "100%" }}
    >
      <div
        className="flex items-center gap-2 border-b border-border shrink-0"
        style={{ padding: "8px 10px", height: 38 }}
      >
        <div className="text-[12px] font-semibold text-text">Chat</div>
        <span className="text-[10px] text-textMute font-mono">page authoring</span>
        <div className="flex-1" />
        <button
          onClick={onClose}
          title="Close chat (⌘\\)"
          aria-label="Close chat"
          className="inline-flex items-center justify-center h-6 w-6 rounded-md text-textDim hover:text-text border border-border cursor-pointer"
          style={{ background: "transparent" }}
        >
          <IconX size={12} />
        </button>
      </div>
      <div
        ref={containerRef}
        className="flex-1 min-h-0 overflow-hidden"
        style={{ display: "flex", flexDirection: "column" }}
      >
        {loading && (
          <div className="text-textMute text-[12px]" style={{ padding: 16 }}>
            initializing chat…
          </div>
        )}
        {error && (
          <div
            className="text-[12px]"
            style={{ padding: 16, color: "#ef4444", whiteSpace: "pre-wrap" }}
          >
            chat failed to start: {error}
          </div>
        )}
      </div>
    </aside>
  );
}

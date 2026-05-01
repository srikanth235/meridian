import { useEffect, useRef, useState } from "react";
import type { Snapshot } from "./types";

type ConnState = "connecting" | "open" | "closed";

export function useSnapshot(): { snapshot: Snapshot | null; conn: ConnState } {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [conn, setConn] = useState<ConnState>("connecting");
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    let cancelled = false;
    let retry = 0;

    const connect = () => {
      const proto = window.location.protocol === "https:" ? "wss" : "ws";
      const host = window.location.host;
      const url = `${proto}://${host}/api/ws`;
      setConn("connecting");
      const ws = new WebSocket(url);
      wsRef.current = ws;

      ws.onopen = () => {
        if (cancelled) return;
        retry = 0;
        setConn("open");
      };
      ws.onmessage = (ev) => {
        if (cancelled) return;
        try {
          const parsed = JSON.parse(ev.data) as Snapshot;
          setSnapshot(parsed);
        } catch {
          /* ignore parse errors */
        }
      };
      ws.onclose = () => {
        if (cancelled) return;
        setConn("closed");
        retry = Math.min(retry + 1, 6);
        setTimeout(connect, 250 * 2 ** retry);
      };
      ws.onerror = () => {
        ws.close();
      };
    };

    // Bootstrap with one HTTP fetch so the page is populated before the WS opens.
    fetch("/api/snapshot")
      .then((r) => (r.ok ? r.json() : null))
      .then((data) => {
        if (!cancelled && data) setSnapshot(data as Snapshot);
      })
      .catch(() => {});

    connect();
    return () => {
      cancelled = true;
      wsRef.current?.close();
    };
  }, []);

  return { snapshot, conn };
}

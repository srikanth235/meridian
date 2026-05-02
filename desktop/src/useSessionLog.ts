import { useEffect, useState } from "react";
import type { SessionLog } from "./types";

/// Polls /api/sessions/{id}/log every 1s while `issueId` is non-null. Returns
/// `undefined` while the first response is in flight, `null` if the daemon
/// has no log for the issue.
export function useSessionLog(issueId: string | null): SessionLog | null | undefined {
  const [log, setLog] = useState<SessionLog | null | undefined>(undefined);

  useEffect(() => {
    if (!issueId) {
      setLog(undefined);
      return;
    }
    setLog(undefined);
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const tick = async () => {
      try {
        const r = await fetch(`/api/sessions/${encodeURIComponent(issueId)}/log`);
        if (cancelled) return;
        if (r.status === 404) {
          setLog(null);
        } else if (r.ok) {
          const data = (await r.json()) as SessionLog;
          if (!cancelled) setLog(data);
        }
      } catch {
        /* ignore — try again on next tick */
      }
      if (!cancelled) timer = setTimeout(tick, 1000);
    };
    tick();

    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [issueId]);

  return log;
}

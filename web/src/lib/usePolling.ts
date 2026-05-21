// Tiny polling hook tuned to the dashboard's read patterns.
//
// Calls `fetcher` on mount, then every `intervalMs` until the
// component unmounts. The returned object carries the latest
// successful value, the last error (if any), and a `refresh`
// thunk so a page can force an out-of-band fetch (e.g. after a
// user action). When the document becomes hidden we suspend
// polling — most operators leave the dashboard in a background
// tab and we don't want to hammer the server for nothing.

import { useEffect, useRef, useState, useCallback } from "react";

export interface PollingState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refresh: () => void;
}

export function usePolling<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
): PollingState<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const cancelledRef = useRef(false);
  const tickRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Stash the fetcher in a ref so re-renders don't restart the
  // polling loop just because the caller passed a fresh
  // arrow-function instance.
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;

  const run = useCallback(async () => {
    try {
      const r = await fetcherRef.current();
      if (cancelledRef.current) return;
      setData(r);
      setError(null);
    } catch (e) {
      if (cancelledRef.current) return;
      setError(String(e));
    } finally {
      if (!cancelledRef.current) setLoading(false);
    }
  }, []);

  const schedule = useCallback(() => {
    if (cancelledRef.current) return;
    if (typeof document !== "undefined" && document.hidden) {
      // Background tab — try again when the tab comes back.
      return;
    }
    tickRef.current = setTimeout(async () => {
      await run();
      schedule();
    }, intervalMs);
  }, [intervalMs, run]);

  const refresh = useCallback(() => {
    if (tickRef.current) clearTimeout(tickRef.current);
    void run();
    schedule();
  }, [run, schedule]);

  useEffect(() => {
    cancelledRef.current = false;
    setLoading(true);
    void run().then(() => schedule());

    const onVisible = () => {
      if (!document.hidden) {
        // Tab came back into focus — pull fresh data immediately
        // and resume the polling loop.
        if (tickRef.current) clearTimeout(tickRef.current);
        void run().then(() => schedule());
      }
    };
    document.addEventListener("visibilitychange", onVisible);

    return () => {
      cancelledRef.current = true;
      if (tickRef.current) clearTimeout(tickRef.current);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [run, schedule]);

  return { data, error, loading, refresh };
}

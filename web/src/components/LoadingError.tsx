import type { ReactNode } from "react";

interface Props {
  loading: boolean;
  error: string | null;
  empty?: boolean;
  emptyMessage?: string;
  children: ReactNode;
}

export function LoadingError({
  loading,
  error,
  empty,
  emptyMessage = "No data yet — emit some events into POST /v1/events.",
  children,
}: Props) {
  if (loading) {
    return (
      <div className="card p-6 text-sm text-ink-600 animate-pulse">
        Loading…
      </div>
    );
  }
  if (error) {
    return (
      <div className="card p-6 border-posture-critical bg-posture-critical/5 text-sm">
        <div className="font-semibold text-posture-critical mb-1">
          Failed to load
        </div>
        <div className="font-mono text-xs">{error}</div>
        <div className="mt-2 text-ink-600">
          Is <code>sezar-server</code> running on port 8090?
        </div>
      </div>
    );
  }
  if (empty) {
    return (
      <div className="card p-6 text-sm text-ink-600">{emptyMessage}</div>
    );
  }
  return <>{children}</>;
}

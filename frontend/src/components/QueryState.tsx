import type { ReactNode } from "react";

export function QueryState({ isLoading, error, children }: { isLoading: boolean; error: unknown; children: ReactNode }) {
  if (isLoading) return <div className="p-4 text-gray-500">Loading…</div>;
  if (error) return <div className="p-4 text-red-600">Error: {error instanceof Error ? error.message : "unknown"}</div>;
  return <>{children}</>;
}

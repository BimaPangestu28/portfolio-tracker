import type { ReactNode } from "react";

export function QueryState({ isLoading, error, children }: { isLoading: boolean; error: unknown; children: ReactNode }) {
  if (isLoading) return <div className="p-4 text-muted-foreground">Loading…</div>;
  if (error) return <div className="p-4 text-destructive">Error: {error instanceof Error ? error.message : "unknown"}</div>;
  return <>{children}</>;
}

import type { ReactNode } from "react";

/**
 * Query lifecycle wrapper: shimmering skeleton rows while loading, an error
 * line on failure, children once loaded.
 */
export function QueryState({
  isLoading,
  error,
  children,
  rows = 4,
  height = 36,
}: {
  isLoading: boolean;
  error: unknown;
  children: ReactNode;
  /** Number of skeleton rows shown while loading. */
  rows?: number;
  /** Height of each skeleton row in px. */
  height?: number;
}) {
  if (isLoading) {
    return (
      <div role="status" aria-busy="true" aria-label="Memuat…" className="flex col gap-3" style={{ padding: 4 }}>
        {Array.from({ length: rows }).map((_, i) => (
          <div key={i} className="skeleton" style={{ width: "100%", height }} />
        ))}
      </div>
    );
  }
  if (error) return <div className="p-4 text-destructive">Error: {error instanceof Error ? error.message : "unknown"}</div>;
  return <>{children}</>;
}

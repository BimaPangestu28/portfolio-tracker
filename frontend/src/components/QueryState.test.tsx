import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { QueryState } from "./QueryState";

test("renders shimmering skeleton rows while loading", () => {
  render(
    <QueryState isLoading error={null}>
      <div>content</div>
    </QueryState>,
  );
  const placeholder = screen.getByRole("status");
  expect(placeholder).toHaveAttribute("aria-busy", "true");
  // Default: a few skeleton rows, no raw "Loading…" text
  expect(placeholder.querySelectorAll(".skeleton").length).toBeGreaterThanOrEqual(3);
  expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
  expect(screen.queryByText("content")).not.toBeInTheDocument();
});

test("row count is configurable", () => {
  render(
    <QueryState isLoading error={null} rows={6}>
      <div>content</div>
    </QueryState>,
  );
  expect(screen.getByRole("status").querySelectorAll(".skeleton")).toHaveLength(6);
});

test("renders the error message when failed", () => {
  render(
    <QueryState isLoading={false} error={new Error("boom")}>
      <div>content</div>
    </QueryState>,
  );
  expect(screen.getByText(/boom/)).toBeInTheDocument();
});

test("renders children when loaded", () => {
  render(
    <QueryState isLoading={false} error={null}>
      <div>content</div>
    </QueryState>,
  );
  expect(screen.getByText("content")).toBeInTheDocument();
});

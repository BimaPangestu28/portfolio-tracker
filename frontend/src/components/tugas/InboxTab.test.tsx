import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { InboxTab } from "./InboxTab";
import * as hooks from "../../api/hooks";

vi.mock("../../api/hooks");

describe("InboxTab", () => {
  const resolveMutate = vi.fn();
  const unresolveMutate = vi.fn();
  beforeEach(() => {
    resolveMutate.mockReset();
    unresolveMutate.mockReset();
    vi.mocked(hooks.useResolveInbox).mockReturnValue({ mutate: resolveMutate, isPending: false } as any);
    vi.mocked(hooks.useUnresolveInbox).mockReturnValue({ mutate: unresolveMutate, isPending: false } as any);
    vi.mocked(hooks.useInbox).mockReturnValue({
      data: [{ id: 4, content: "Ide produk baru", status: "pending", created_at: "2026-06-15T08:00:00+07:00", resolved_at: null }],
      isLoading: false, isError: false,
    } as any);
  });

  it("resolves a pending item", () => {
    render(<InboxTab />);
    fireEvent.click(screen.getByLabelText("Selesaikan Ide produk baru"));
    expect(resolveMutate).toHaveBeenCalledWith(4, expect.anything());
  });

  it("switches to the sorted filter", () => {
    render(<InboxTab />);
    fireEvent.click(screen.getByRole("button", { name: "Selesai" }));
    expect(hooks.useInbox).toHaveBeenCalledWith("sorted");
  });
});

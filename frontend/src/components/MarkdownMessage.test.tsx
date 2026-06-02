import { render, screen } from "@testing-library/react";
import { MarkdownMessage } from "./MarkdownMessage";

test("renders bold, lists, and links as markdown elements", () => {
  render(
    <MarkdownMessage
      content={"**XIRR:** 12.4%\n\n- Saham\n- Obligasi\n\n[docs](https://example.com)"}
    />,
  );

  expect(screen.getByText("XIRR:").tagName).toBe("STRONG");
  expect(screen.getAllByRole("listitem")).toHaveLength(2);

  const link = screen.getByRole("link", { name: "docs" });
  expect(link).toHaveAttribute("href", "https://example.com");
  expect(link).toHaveAttribute("target", "_blank");
  expect(link).toHaveAttribute("rel", "noreferrer");
});

test("renders GFM tables (remark-gfm enabled)", () => {
  render(
    <MarkdownMessage content={"| Aset | % |\n| --- | --- |\n| Saham | 60 |"} />,
  );
  expect(screen.getByRole("table")).toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "Aset" })).toBeInTheDocument();
});

test("escapes raw HTML instead of executing it (XSS-safe)", () => {
  const { container } = render(
    <MarkdownMessage content={"<img src=x onerror=alert(1) />hello"} />,
  );
  // The raw tag must not become a real element in the DOM.
  expect(container.querySelector("img")).toBeNull();
  expect(screen.getByText(/hello/)).toBeInTheDocument();
});

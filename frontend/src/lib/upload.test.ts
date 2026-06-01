import { splitDataUrl, ACCEPTED_TYPES } from "./upload";

test("splitDataUrl extracts base64 payload from a data URL", () => {
  const dataUrl = "data:image/png;base64,AAABBB==";
  expect(splitDataUrl(dataUrl)).toBe("AAABBB==");
});

test("splitDataUrl returns input unchanged when no comma prefix", () => {
  expect(splitDataUrl("AAAA")).toBe("AAAA");
});

test("accepted types include png jpeg and pdf", () => {
  expect(ACCEPTED_TYPES).toContain("image/png");
  expect(ACCEPTED_TYPES).toContain("application/pdf");
});

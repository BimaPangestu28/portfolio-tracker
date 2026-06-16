import { readConfig } from "./config";
import { mountWidget } from "./ui";

// Find THIS script tag (document.currentScript works during initial execution;
// fall back to the last script with data-key for async/defer edge cases).
function ownScript(): HTMLScriptElement | null {
  if (document.currentScript instanceof HTMLScriptElement) return document.currentScript;
  const all = Array.from(document.querySelectorAll("script[data-key]"));
  return (all[all.length - 1] as HTMLScriptElement) ?? null;
}

try {
  const script = ownScript();
  if (!script) throw new Error("cs-widget: could not locate its own <script> tag");
  const cfg = readConfig(script);
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => mountWidget(cfg));
  } else {
    mountWidget(cfg);
  }
} catch (e) {
  // Never break the host page; log and bail.
  console.error(e);
}

export type Theme = "light" | "dark";

export interface WidgetConfig {
  siteKey: string;
  apiBase: string;
  title: string;
  /** Visual theme of the panel; "light" (default) or "dark". */
  theme: Theme;
  /** Accent color (any CSS color) for the launcher, buttons and own messages. */
  accent: string;
  /** Text color drawn on top of the accent (buttons, user bubbles). */
  accentFg: string;
}

/** Read widget config from the embedding <script> tag's data-attributes. */
export function readConfig(script: HTMLScriptElement): WidgetConfig {
  const siteKey = script.getAttribute("data-key");
  if (!siteKey) {
    throw new Error("cs-widget: missing required data-key attribute on the script tag");
  }
  const apiBase = (script.getAttribute("data-api-base") ?? "/api").replace(/\/+$/, "");
  const title = script.getAttribute("data-title") ?? "Customer Service";
  const theme: Theme = script.getAttribute("data-theme") === "dark" ? "dark" : "light";
  const accent = script.getAttribute("data-accent") ?? "#2563eb";
  const accentFg = script.getAttribute("data-accent-fg") ?? "#ffffff";
  return { siteKey, apiBase, title, theme, accent, accentFg };
}

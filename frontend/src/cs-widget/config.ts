export interface WidgetConfig {
  siteKey: string;
  apiBase: string;
  title: string;
}

/** Read widget config from the embedding <script> tag's data-attributes. */
export function readConfig(script: HTMLScriptElement): WidgetConfig {
  const siteKey = script.getAttribute("data-key");
  if (!siteKey) {
    throw new Error("cs-widget: missing required data-key attribute on the script tag");
  }
  const apiBase = (script.getAttribute("data-api-base") ?? "/api").replace(/\/+$/, "");
  const title = script.getAttribute("data-title") ?? "Customer Service";
  return { siteKey, apiBase, title };
}

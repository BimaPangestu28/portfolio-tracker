import { CsApi, type Lead } from "./api";
import type { WidgetConfig } from "./config";
import { validateLead } from "./validate";

// Theme tokens resolved at mount time and exposed as CSS custom properties on
// :host, so the rules below stay theme-agnostic. --accent / --accent-fg come
// from the embed's data-attributes; the rest switch on light vs dark.
const THEMES = {
  light: { bg: "#ffffff", fg: "#111827", sub: "#6b7280", muted: "#f3f4f6", border: "#e5e7eb", inputBg: "#ffffff" },
  dark: { bg: "#1c1b18", fg: "#f5f3ee", sub: "#a8a29e", muted: "#2a2825", border: "#3a3733", inputBg: "#232220" },
} as const;

const STYLE = `
:host { all: initial; }
* { box-sizing: border-box; }
.bubble { position: fixed; right: 20px; bottom: 20px; width: 56px; height: 56px; border-radius: 50%;
  background: var(--accent); color: var(--accent-fg); font: 24px system-ui, sans-serif; border: none; cursor: pointer;
  box-shadow: 0 6px 18px rgba(0,0,0,.28); z-index: 2147483000; transition: transform .15s ease, box-shadow .15s ease; }
.bubble:hover { transform: translateY(-2px); box-shadow: 0 10px 24px rgba(0,0,0,.32); }
.panel { position: fixed; right: 20px; bottom: 88px; width: 360px; max-width: calc(100vw - 40px); height: 480px; max-height: calc(100vh - 120px);
  background: var(--bg); border: 1px solid var(--border); border-radius: 16px; box-shadow: 0 16px 48px rgba(0,0,0,.32);
  display: none; flex-direction: column; overflow: hidden; z-index: 2147483000;
  font: 14px system-ui, -apple-system, sans-serif; color: var(--fg); }
.panel.open { display: flex; }
.header { padding: 15px 16px; font-weight: 600; font-size: 15px; color: var(--fg);
  border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 8px; }
.header::before { content: ""; width: 8px; height: 8px; border-radius: 50%; background: var(--accent); flex: none; }
.body { flex: 1; overflow-y: auto; padding: 14px; display: flex; flex-direction: column; gap: 8px; }
.msg { padding: 9px 12px; border-radius: 14px; max-width: 85%; white-space: pre-wrap; line-height: 1.45; }
.msg.user { align-self: flex-end; background: var(--accent); color: var(--accent-fg); border-bottom-right-radius: 4px; }
.msg.bot { align-self: flex-start; background: var(--muted); color: var(--fg); border-bottom-left-radius: 4px; }
.foot { border-top: 1px solid var(--border); padding: 10px; display: flex; gap: 8px; }
.foot input, .form input { flex: 1; padding: 10px 12px; background: var(--input-bg); color: var(--fg);
  border: 1px solid var(--border); border-radius: 10px; font: inherit; outline: none; }
.foot input::placeholder, .form input::placeholder { color: var(--sub); }
.foot input:focus, .form input:focus { border-color: var(--accent); }
.foot button, .form button { padding: 10px 16px; background: var(--accent); color: var(--accent-fg);
  border: none; border-radius: 10px; cursor: pointer; font: inherit; font-weight: 600; transition: opacity .15s ease; }
.foot button:hover, .form button:hover { opacity: .9; }
.foot button:disabled, .form button:disabled { opacity: .5; cursor: default; }
.form { padding: 16px; display: flex; flex-direction: column; gap: 10px; }
.err { color: #ef4444; font-size: 12px; min-height: 14px; }
`;

/** Build the `:host { --token: value }` block from the resolved theme + accent. */
function themeVars(cfg: WidgetConfig): string {
  const t = THEMES[cfg.theme];
  return `:host { --accent:${cfg.accent}; --accent-fg:${cfg.accentFg}; --bg:${t.bg}; --fg:${t.fg};` +
    ` --sub:${t.sub}; --muted:${t.muted}; --border:${t.border}; --input-bg:${t.inputBg}; }`;
}

export function mountWidget(cfg: WidgetConfig) {
  const api = new CsApi(cfg);
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = host.attachShadow({ mode: "open" });
  root.innerHTML = `
    <style>${themeVars(cfg)}${STYLE}</style>
    <button class="bubble" aria-label="Chat">💬</button>
    <div class="panel" role="dialog" aria-label="chat">
      <div class="header"></div>
      <div class="body"></div>
      <form class="form">
        <input name="name" placeholder="Nama" autocomplete="name" />
        <input name="email" placeholder="Email" autocomplete="email" />
        <input name="phone" placeholder="No. HP (opsional jika ada email)" autocomplete="tel" />
        <div class="err"></div>
        <button type="submit">Mulai chat</button>
      </form>
      <div class="foot" style="display:none">
        <input class="msg-input" placeholder="Tulis pesan..." />
        <button class="send">Kirim</button>
      </div>
    </div>`;

  const $ = <T extends Element>(sel: string) => root.querySelector(sel) as T;
  const panel = $<HTMLDivElement>(".panel");

  // Fix 1 (XSS): set dynamic values via textContent/setAttribute after innerHTML is set
  ($<HTMLDivElement>(".header")).textContent = cfg.title;
  panel.setAttribute("aria-label", cfg.title);
  const body = $<HTMLDivElement>(".body");
  const form = $<HTMLFormElement>(".form");
  const foot = $<HTMLDivElement>(".foot");
  const err = $<HTMLDivElement>(".err");
  const input = $<HTMLInputElement>(".msg-input");

  let token: string | null = null;

  const addMsg = (text: string, who: "user" | "bot") => {
    const d = document.createElement("div");
    d.className = `msg ${who}`;
    d.textContent = text;
    body.appendChild(d);
    body.scrollTop = body.scrollHeight;
  };

  $(".bubble").addEventListener("click", () => panel.classList.toggle("open"));

  let starting = false;
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (starting) return;
    const data = new FormData(form);
    const lead: Lead = {
      name: String(data.get("name") ?? ""),
      email: String(data.get("email") ?? ""),
      phone: String(data.get("phone") ?? ""),
    };
    const problem = validateLead(lead);
    if (problem) { err.textContent = problem; return; }
    err.textContent = "";
    const submitBtn = $<HTMLButtonElement>('button[type="submit"]');
    starting = true;
    submitBtn.disabled = true;
    try {
      token = await api.startSession(lead);
      form.style.display = "none";
      foot.style.display = "flex";
      addMsg(`Halo ${lead.name}! Ada yang bisa kami bantu?`, "bot");
    } catch (e2) {
      err.textContent = (e2 as Error).message;
    } finally {
      starting = false;
      submitBtn.disabled = false;
    }
  });

  let sending = false;
  const send = async () => {
    const text = input.value.trim();
    if (!text || !token || sending) return;
    const sendBtn = $<HTMLButtonElement>(".send");
    sending = true;
    sendBtn.disabled = true;
    addMsg(text, "user");
    input.value = "";
    try {
      const reply = await api.sendMessage(token, text);
      addMsg(reply, "bot");
    } catch (e2) {
      addMsg(`⚠️ ${(e2 as Error).message}`, "bot");
    } finally {
      sending = false;
      sendBtn.disabled = false;
    }
  };
  $(".send").addEventListener("click", send);
  input.addEventListener("keydown", (e) => { if ((e as KeyboardEvent).key === "Enter") send(); });
}

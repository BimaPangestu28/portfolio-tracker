import { CsApi, type Lead } from "./api";
import type { WidgetConfig } from "./config";
import { validateLead } from "./validate";

const STYLE = `
:host { all: initial; }
.bubble { position: fixed; right: 20px; bottom: 20px; width: 56px; height: 56px; border-radius: 50%;
  background: #2563eb; color: #fff; font: 24px sans-serif; border: none; cursor: pointer; box-shadow: 0 4px 12px rgba(0,0,0,.25); z-index: 2147483000; }
.panel { position: fixed; right: 20px; bottom: 88px; width: 340px; max-width: calc(100vw - 40px); height: 460px; max-height: calc(100vh - 120px);
  background: #fff; border-radius: 12px; box-shadow: 0 8px 30px rgba(0,0,0,.25); display: none; flex-direction: column; overflow: hidden; z-index: 2147483000; font: 14px sans-serif; color: #111; }
.panel.open { display: flex; }
.header { background: #2563eb; color: #fff; padding: 12px 14px; font-weight: 600; }
.body { flex: 1; overflow-y: auto; padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.msg { padding: 8px 10px; border-radius: 10px; max-width: 85%; white-space: pre-wrap; }
.msg.user { align-self: flex-end; background: #2563eb; color: #fff; }
.msg.bot { align-self: flex-start; background: #f1f5f9; color: #111; }
.foot { border-top: 1px solid #e5e7eb; padding: 8px; display: flex; gap: 6px; }
.foot input, .form input { flex: 1; padding: 8px; border: 1px solid #cbd5e1; border-radius: 8px; font: inherit; }
.foot button, .form button { padding: 8px 12px; background: #2563eb; color: #fff; border: none; border-radius: 8px; cursor: pointer; }
.form { padding: 14px; display: flex; flex-direction: column; gap: 8px; }
.err { color: #b91c1c; font-size: 12px; min-height: 14px; }
`;

export function mountWidget(cfg: WidgetConfig) {
  const api = new CsApi(cfg);
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = host.attachShadow({ mode: "open" });
  root.innerHTML = `
    <style>${STYLE}</style>
    <button class="bubble" aria-label="Chat">💬</button>
    <div class="panel" role="dialog" aria-label="${cfg.title}">
      <div class="header">${cfg.title}</div>
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

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const data = new FormData(form);
    const lead: Lead = {
      name: String(data.get("name") ?? ""),
      email: String(data.get("email") ?? ""),
      phone: String(data.get("phone") ?? ""),
    };
    const problem = validateLead(lead);
    if (problem) { err.textContent = problem; return; }
    err.textContent = "";
    try {
      token = await api.startSession(lead);
      form.style.display = "none";
      foot.style.display = "flex";
      addMsg(`Halo ${lead.name}! Ada yang bisa kami bantu?`, "bot");
    } catch (e2) {
      err.textContent = (e2 as Error).message;
    }
  });

  const send = async () => {
    const text = input.value.trim();
    if (!text || !token) return;
    addMsg(text, "user");
    input.value = "";
    try {
      const reply = await api.sendMessage(token, text);
      addMsg(reply, "bot");
    } catch (e2) {
      addMsg(`⚠️ ${(e2 as Error).message}`, "bot");
    }
  };
  $(".send").addEventListener("click", send);
  input.addEventListener("keydown", (e) => { if ((e as KeyboardEvent).key === "Enter") send(); });
}

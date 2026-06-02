import makeWASocket, { useMultiFileAuthState, DisconnectReason } from "@whiskeysockets/baileys";
import { rm } from "node:fs/promises";

const BACKEND = process.env.BACKEND_URL ?? "http://localhost:8080";
const GATEWAY_TOKEN = process.env.GATEWAY_TOKEN ?? "";
const AUTH_DIR = "auth_state";
const COMMAND_POLL_MS = 3000;

const authHeaders = { "content-type": "application/json", "x-gateway-token": GATEWAY_TOKEN };

// The currently-active socket; reassigned whenever we (re)start a session.
let currentSock = null;

/** Push the current connection state to the backend so the web UI can show it. */
async function reportState(status, extra = {}) {
  try {
    await fetch(`${BACKEND}/whatsapp/state`, {
      method: "POST",
      headers: authHeaders,
      body: JSON.stringify({ status, ...extra }),
    });
  } catch (e) {
    console.error("reportState failed", e);
  }
}

/** Ask the backend for a pending control command (consumed once). */
async function fetchCommand() {
  try {
    const res = await fetch(`${BACKEND}/whatsapp/commands`, { headers: authHeaders });
    if (!res.ok) return null;
    const { command } = await res.json();
    return command ?? null;
  } catch (e) {
    console.error("fetchCommand failed", e);
    return null;
  }
}

/** Forward one inbound WhatsApp message to the chatbot and reply. */
async function forwardInbound(sock, from, text) {
  try {
    const res = await fetch(`${BACKEND}/chat/whatsapp/inbound`, {
      method: "POST",
      headers: authHeaders,
      body: JSON.stringify({ from, message: text }),
    });
    if (!res.ok) { console.error("backend error", res.status); return; }
    const { reply } = await res.json();
    await sock.sendMessage(from, { text: reply });
  } catch (e) {
    console.error("gateway error", e);
  }
}

/** Start (or restart) a Baileys session and wire up its event handlers. */
async function start() {
  const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
  const sock = makeWASocket({ auth: state, printQRInTerminal: false });
  currentSock = sock;
  sock.ev.on("creds.update", saveCreds);

  sock.ev.on("connection.update", async (u) => {
    const { connection, lastDisconnect, qr } = u;
    if (qr) await reportState("qr", { qr });
    if (connection === "open") {
      const number = sock.user?.id?.split(":")[0] ?? null;
      await reportState("connected", { number });
      console.log("WhatsApp connected.");
    } else if (connection === "close") {
      await reportState("disconnected");
      const loggedOut = lastDisconnect?.error?.output?.statusCode === DisconnectReason.loggedOut;
      if (!loggedOut) start();
    }
  });

  sock.ev.on("messages.upsert", async ({ messages, type }) => {
    if (type !== "notify") return;
    for (const m of messages) {
      if (m.key.fromMe) continue;
      const text = m.message?.conversation ?? m.message?.extendedTextMessage?.text;
      const from = m.key.remoteJid;
      if (!text || !from) continue;
      await forwardInbound(sock, from, text);
    }
  });
}

/** Poll the backend for control commands and act on them. */
function startCommandLoop() {
  setInterval(async () => {
    const command = await fetchCommand();
    if (command === "logout") {
      try { await currentSock?.logout(); } catch (e) { console.error("logout failed", e); }
      await rm(AUTH_DIR, { recursive: true, force: true });
      await reportState("disconnected");
      start();
    } else if (command === "restart") {
      start();
    }
  }, COMMAND_POLL_MS);
}

reportState("connecting");
startCommandLoop();
start();

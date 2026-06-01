import makeWASocket, { useMultiFileAuthState, DisconnectReason } from "@whiskeysockets/baileys";
import qrcode from "qrcode-terminal";

const BACKEND = process.env.BACKEND_URL ?? "http://localhost:8080";
const GATEWAY_TOKEN = process.env.GATEWAY_TOKEN ?? "";

async function start() {
  const { state, saveCreds } = await useMultiFileAuthState("auth_state");
  const sock = makeWASocket({ auth: state, printQRInTerminal: false });

  sock.ev.on("creds.update", saveCreds);

  sock.ev.on("connection.update", (u) => {
    const { connection, lastDisconnect, qr } = u;
    if (qr) qrcode.generate(qr, { small: true });
    if (connection === "close") {
      const shouldReconnect = lastDisconnect?.error?.output?.statusCode !== DisconnectReason.loggedOut;
      console.log("connection closed, reconnect:", shouldReconnect);
      if (shouldReconnect) start();
    } else if (connection === "open") {
      console.log("WhatsApp connected.");
    }
  });

  sock.ev.on("messages.upsert", async ({ messages, type }) => {
    if (type !== "notify") return;
    for (const m of messages) {
      if (m.key.fromMe) continue;
      const text = m.message?.conversation ?? m.message?.extendedTextMessage?.text;
      const from = m.key.remoteJid;
      if (!text || !from) continue;
      try {
        const res = await fetch(`${BACKEND}/chat/whatsapp/inbound`, {
          method: "POST",
          headers: { "content-type": "application/json", "x-gateway-token": GATEWAY_TOKEN },
          body: JSON.stringify({ from, message: text }),
        });
        if (!res.ok) { console.error("backend error", res.status); continue; }
        const { reply } = await res.json();
        await sock.sendMessage(from, { text: reply });
      } catch (e) {
        console.error("gateway error", e);
      }
    }
  });
}

start();

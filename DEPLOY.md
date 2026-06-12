# Deployment

> **Production** runs on the k3s cluster at `portfolio.catalystlabs.id` via
> nginx-ingress + cert-manager. See [`k8s/README.md`](k8s/README.md) — that is
> the source of truth for the live deployment.
>
> The Docker Compose stack below is for **local / single-host self-hosting** on a
> box where ports 80/443 are free.

Self-contained Docker Compose stack: a Rust/axum **backend**, a Caddy-served
React **frontend** (also the TLS-terminating reverse proxy), and a Node/Baileys
**WhatsApp gateway**.

```
Internet ──443──▶ frontend (Caddy)
                    ├── /api/*  ──▶ backend:8080   (SQLite on a volume)
                    └── /*      ──▶ static SPA
                              backend ◀── gateway   (WhatsApp, auth_state volume)
```

## Prerequisites

- A server with Docker Engine + the Compose plugin.
- DNS `A` record for your domain pointing at the server (already set:
  `portfolio.catalystlabs.id → 62.171.174.152`).
- Ports **80** and **443** open (Caddy needs both for HTTP-01 TLS issuance).

## First-time deploy

```bash
# 1. Install Docker (Debian/Ubuntu) if needed:
curl -fsSL https://get.docker.com | sh

# 2. Get the code:
git clone https://github.com/BimaPangestu28/portfolio-tracker.git
cd portfolio-tracker

# 3. Create the env file and fill in secrets:
cp .env.production.example .env
#    - DOMAIN is preset to portfolio.catalystlabs.id
#    - ANTHROPIC_API_KEY: your DeepSeek API key (name kept for compatibility)
#    - GATEWAY_TOKEN: openssl rand -hex 32
nano .env

# 4. Build and start:
./deploy.sh
```

Caddy obtains a Let's Encrypt certificate automatically on first start (this can
take ~30s; watch `docker compose logs -f frontend`).

## Pair WhatsApp

The gateway prints a QR code to its logs. Pair it once; credentials persist in
the `gateway_auth` volume across restarts.

```bash
docker compose logs -f gateway   # scan the QR with WhatsApp > Linked devices
```

## Operations

```bash
docker compose ps                 # status
docker compose logs -f backend    # backend logs
docker compose up -d --build      # redeploy after `git pull`
docker compose down               # stop (volumes/data preserved)
```

### Update to the latest code

```bash
git pull
./deploy.sh
```

### Backups

All state lives in named volumes:

- `backend_data` — the SQLite database (`/data/portfolio.db`)
- `gateway_auth` — WhatsApp pairing credentials
- `caddy_data` — issued TLS certificates

Back up the database:

```bash
docker compose cp backend:/data/portfolio.db ./portfolio-backup-$(date +%F).db
```

## Security notes

- `.env` and `.env.production` are gitignored — never commit real secrets.
- Rotate any secret that has been shared in plaintext (chat, tickets, etc.).
- Consider disabling SSH password auth in favour of keys, and restricting root
  login, once access is set up.

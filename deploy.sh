#!/usr/bin/env bash
#
# Build and (re)start the production stack. Run this ON the server, from the
# repository root, after creating a .env file from .env.production.example.
#
#   ./deploy.sh
#
set -euo pipefail

if ! command -v docker >/dev/null 2>&1; then
  echo "error: Docker is not installed. See DEPLOY.md for one-line install." >&2
  exit 1
fi

if [ ! -f .env ]; then
  echo "error: .env not found. Run: cp .env.production.example .env  and fill it in." >&2
  exit 1
fi

echo ">> Building and starting containers..."
docker compose up -d --build

echo ">> Waiting for the backend to become healthy..."
for _ in $(seq 1 30); do
  if docker compose ps --format '{{.Service}} {{.Health}}' | grep -q "backend healthy"; then
    echo ">> Backend healthy."
    break
  fi
  sleep 3
done

docker compose ps
echo
echo ">> Done. To pair WhatsApp, scan the QR shown by:"
echo "     docker compose logs -f gateway"

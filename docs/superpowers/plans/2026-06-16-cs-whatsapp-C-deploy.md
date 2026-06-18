# CS WhatsApp — Plan C: Deploy Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run a second `cs-gateway` (same image as the owner gateway) in both deploy stacks — docker-compose and k8s — wired to the `/cs/*` backend endpoints with its own auth-state volume and token.

**Architecture:** The CS gateway is the existing gateway image with env `PATH_PREFIX=/cs`, `OUTBOUND_ENABLED=1`, its own `AUTH_DIR` volume, and `GATEWAY_TOKEN` set to the **CS** token (the backend reads it as `CS_GATEWAY_TOKEN`). The backend service/deployment gains `CS_GATEWAY_TOKEN` so its CS endpoints authenticate.

**Tech Stack:** docker-compose, Kubernetes (k3s). Depends on Plans A + B.

> **Worktree** `/home/bima-pangestu/Works/portfolio-tracker/.claude/worktrees/cs-chatbot`. This plan is config-only (no automated tests). Verify with `docker compose config` (syntax) and `kubectl apply --dry-run=client` if available; otherwise careful YAML review.

> **Existing references (from research):**
> - `docker-compose.prod.yml` gateway service (~line 61): image `ghcr.io/bimapangestu28/portfolio-gateway:latest`, env `BACKEND_URL`/`GATEWAY_TOKEN`, volume `gateway_auth:/app/auth_state`.
> - `k8s/30-gateway.yaml`: PVC `gateway-auth` + Deployment `gateway` (strategy Recreate, env from secret `portfolio-secrets` key `GATEWAY_TOKEN`, volume mount `/app/auth_state`).
> - `k8s/secret.example.yaml`: `portfolio-secrets` with `GATEWAY_TOKEN` etc.

---

## File Structure

- Modify: `docker-compose.prod.yml` — add `cs-gateway` service + `cs_gateway_auth` volume + `CS_GATEWAY_TOKEN` on backend.
- Create: `k8s/31-cs-gateway.yaml` — PVC `cs-gateway-auth` + Deployment `cs-gateway`.
- Modify: `k8s/secret.example.yaml` — add `CS_GATEWAY_TOKEN`.
- Modify: the backend k8s deployment (find `k8s/*backend*.yaml`) — add `CS_GATEWAY_TOKEN` env from the secret.
- Modify: `.env.production.example` (repo root) — document `CS_GATEWAY_TOKEN`.

---

## Task 1: docker-compose cs-gateway

**Files:** Modify `docker-compose.prod.yml`.

- [ ] **Step 1: Add the `cs-gateway` service** (mirror the existing `gateway` block; place right after it)

```yaml
  cs-gateway:
    image: ghcr.io/bimapangestu28/portfolio-gateway:latest
    restart: unless-stopped
    depends_on:
      backend:
        condition: service_healthy
    environment:
      BACKEND_URL: http://backend:8080
      PATH_PREFIX: /cs
      OUTBOUND_ENABLED: "1"
      AUTH_DIR: /app/auth_state
      GATEWAY_TOKEN: ${CS_GATEWAY_TOKEN:?CS_GATEWAY_TOKEN is required}
    volumes:
      - cs_gateway_auth:/app/auth_state
```

- [ ] **Step 2: Give the backend the CS token**

In the `backend` service `environment:` block, add:

```yaml
      CS_GATEWAY_TOKEN: ${CS_GATEWAY_TOKEN:?CS_GATEWAY_TOKEN is required}
```

- [ ] **Step 3: Declare the volume**

In the top-level `volumes:` map (where `gateway_auth:` is declared), add:

```yaml
  cs_gateway_auth:
```

- [ ] **Step 4: Verify + commit**

Run: `docker compose -f docker-compose.prod.yml config >/dev/null && echo OK` (needs `CS_GATEWAY_TOKEN` set in env to pass the `:?` guard — export a dummy for the check: `CS_GATEWAY_TOKEN=x GATEWAY_TOKEN=x ... docker compose -f docker-compose.prod.yml config >/dev/null`). If `docker compose` is unavailable, state that and do a careful manual YAML review.

```bash
git add docker-compose.prod.yml
git commit -m "feat(deploy): cs-gateway service + CS_GATEWAY_TOKEN (compose)"
```

---

## Task 2: k8s cs-gateway

**Files:** Create `k8s/31-cs-gateway.yaml`; Modify `k8s/secret.example.yaml` + the backend deployment manifest.

- [ ] **Step 1: Create `k8s/31-cs-gateway.yaml`** (mirror `30-gateway.yaml`)

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: cs-gateway-auth
  namespace: portfolio
spec:
  accessModes: [ReadWriteOnce]
  resources:
    requests:
      storage: 256Mi
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: cs-gateway
  namespace: portfolio
spec:
  replicas: 1
  strategy:
    type: Recreate  # Baileys creds on ReadWriteOnce volume; single pod only
  selector:
    matchLabels:
      app: cs-gateway
  template:
    metadata:
      labels:
        app: cs-gateway
    spec:
      imagePullSecrets:
        - name: ghcr-creds
      containers:
        - name: cs-gateway
          image: ghcr.io/bimapangestu28/portfolio-gateway:latest
          imagePullPolicy: Always
          env:
            - name: BACKEND_URL
              value: "http://backend:8080"
            - name: PATH_PREFIX
              value: "/cs"
            - name: OUTBOUND_ENABLED
              value: "1"
            - name: AUTH_DIR
              value: "/app/auth_state"
            - name: GATEWAY_TOKEN
              valueFrom:
                secretKeyRef:
                  name: portfolio-secrets
                  key: CS_GATEWAY_TOKEN
          volumeMounts:
            - name: auth
              mountPath: /app/auth_state
      volumes:
        - name: auth
          persistentVolumeClaim:
            claimName: cs-gateway-auth
```

- [ ] **Step 2: Add `CS_GATEWAY_TOKEN` to the secret template**

In `k8s/secret.example.yaml` under `stringData:`, add:

```yaml
  CS_GATEWAY_TOKEN: "REPLACE_ME"
```

- [ ] **Step 3: Give the backend deployment the CS token**

Find the backend Deployment manifest (search `k8s/` for `name: backend` — likely `k8s/20-backend.yaml` or similar). In its container `env:` list, add (mirroring how `GATEWAY_TOKEN` is injected there):

```yaml
            - name: CS_GATEWAY_TOKEN
              valueFrom:
                secretKeyRef:
                  name: portfolio-secrets
                  key: CS_GATEWAY_TOKEN
```
State which file you edited.

- [ ] **Step 4: Verify + commit**

Run (if `kubectl` available): `kubectl apply --dry-run=client -f k8s/31-cs-gateway.yaml` and for the edited backend manifest. If unavailable, careful YAML review (indentation, the `---` separator, namespace).

```bash
git add k8s/31-cs-gateway.yaml k8s/secret.example.yaml k8s/*backend*.yaml
git commit -m "feat(deploy): cs-gateway Deployment + PVC + secret (k8s)"
```

---

## Task 3: Document env in the root example

**Files:** Modify `.env.production.example`.

- [ ] **Step 1: Add the var** (match the file's style)

```bash
# CS WhatsApp gateway (Phase 2): token shared between the backend and the cs-gateway.
CS_GATEWAY_TOKEN=
```

- [ ] **Step 2: Commit**

```bash
git add .env.production.example
git commit -m "docs(deploy): document CS_GATEWAY_TOKEN"
```

---

## Self-Review

**Spec coverage (Plan C):**
- `cs-gateway` in compose with own volume + token ✓ Task 1.
- `cs-gateway` Deployment + PVC in k8s ✓ Task 2.
- Backend gets `CS_GATEWAY_TOKEN` in both stacks ✓ Tasks 1,2.
- Env documented ✓ Task 3.

**Placeholder scan:** No TBD/TODO. `REPLACE_ME` matches the existing secret template convention.

**Consistency:** `PATH_PREFIX=/cs` + `OUTBOUND_ENABLED=1` match Plan B; the gateway sends `x-gateway-token: GATEWAY_TOKEN` whose value is the CS token, which the backend validates as `CS_GATEWAY_TOKEN` (Plan A). Separate volumes (`cs_gateway_auth` / `cs-gateway-auth`) keep the two WhatsApp sessions independent.

---

## Downstream

- **Plan D — Frontend:** "CS WhatsApp" pairing card (`/cs/whatsapp/{status,connect,disconnect}`) + CS Inbox reply box (`/cs/admin/conversations/:id/reply`).
- **Note:** the `portfolio-gateway` image must be built from `whatsapp-gateway/` with Plan B's changes — the existing `Build and push images` workflow already builds it; no new image needed (same image, different env).

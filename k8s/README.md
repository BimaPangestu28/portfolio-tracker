# Kubernetes deployment (k3s + nginx-ingress + cert-manager)

Deploys Portfolio Tracker into the `portfolio` namespace, matching how the other
`*.catalystlabs.id` apps on this cluster are run. Images come from GHCR (built by
the `Build and push images` GitHub Actions workflow).

```
Internet ─▶ nginx-ingress (TLS via cert-manager) ─▶ frontend (Caddy, :80)
                                                       ├── /api/* ─▶ backend:8080  (SQLite PVC)
                                                       └── /*     ─▶ static SPA
                                                  gateway ─▶ backend  (WhatsApp, auth PVC)
```

## First-time apply

```bash
# 1. Namespace
kubectl apply -f 00-namespace.yaml

# 2. App secret (values never committed). AUTH_PASSWORD is the master password
#    you log in with; JWT_SECRET signs the session tokens. Both must be set
#    together (the backend refuses to start if only one is set).
kubectl -n portfolio create secret generic portfolio-secrets \
  --from-literal=ANTHROPIC_API_KEY=sk-ant-... \
  --from-literal=GATEWAY_TOKEN=$(openssl rand -hex 32) \
  --from-literal=AUTH_PASSWORD='<your master password>' \
  --from-literal=JWT_SECRET=$(openssl rand -hex 32)

# 3. Image pull secret for GHCR (read:packages)
kubectl -n portfolio create secret docker-registry ghcr-creds \
  --docker-server=ghcr.io \
  --docker-username=BimaPangestu28 \
  --docker-password=<TOKEN>

# 4. Workloads + ingress
kubectl apply -f 10-backend.yaml -f 20-frontend.yaml -f 30-gateway.yaml -f 40-ingress.yaml
```

Auth is enforced by the backend (`POST /auth/login` → JWT). To change the master
password later, update the secret and restart the backend:

```bash
kubectl -n portfolio patch secret portfolio-secrets --type merge \
  -p "{\"stringData\":{\"AUTH_PASSWORD\":\"<new password>\"}}"
kubectl -n portfolio rollout restart deploy/backend
```

cert-manager issues the TLS certificate automatically (watch
`kubectl -n portfolio get certificate`).

## Updating to a new image

The workflow pushes `:latest` (and `:<sha>`). To roll pods onto the newest image:

```bash
kubectl -n portfolio rollout restart deploy/backend deploy/frontend deploy/gateway
```

## Pair WhatsApp

```bash
kubectl -n portfolio logs deploy/gateway -f   # scan the QR with WhatsApp > Linked devices
```

## Operations

```bash
kubectl -n portfolio get pods,ingress,certificate
kubectl -n portfolio logs deploy/backend -f
kubectl -n portfolio exec deploy/backend -- sh -c 'ls -la /data'   # SQLite db
```

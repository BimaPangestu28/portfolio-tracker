# Embedding the Customer-Service Widget

Add this one line before `</body>` on any page where you want the chat bubble:

```html
<script
  src="https://portfolio.catalystlabs.id/cs-widget.js"
  data-key="YOUR_CS_WIDGET_KEY"
  data-title="Customer Service"
  defer></script>
```

- `data-key` (required): the value of `CS_WIDGET_KEY` set on the backend. It is **not a secret** (it ships in page JS); abuse is controlled server-side by the Origin allowlist (`CS_ALLOWED_ORIGINS`), rate limiting, and per-conversation caps.
- `data-api-base` (optional): override the API base. Defaults to `/api` (same-origin). For a site on a **different** domain than the backend, set the absolute base, e.g. `data-api-base="https://portfolio.catalystlabs.id/api"`, and add that site's origin to `CS_ALLOWED_ORIGINS`.
- `data-title` (optional): panel header text. Default "Customer Service".

## Backend setup

Set both env vars (together) to enable the public endpoints:

```
CS_ALLOWED_ORIGINS=https://your-site.com,https://www.your-site.com
CS_WIDGET_KEY=<any non-secret routing key>
OPENAI_API_KEY=<for KB embeddings>   # already used for ingestion
```

Populate the knowledge base, pricing, and orders via the admin UI (Plan 4) before going live.

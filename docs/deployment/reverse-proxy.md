# Reverse proxy topology

Reverie ships as a single container that serves both the HTML frontend
(`/` and SPA deep links) and the API (`/api`, `/auth`, `/health`, `/opds`)
from a single port (default 3000). There is no separate frontend container.

This page covers the two deployment topologies most operators choose.

## Recommended: TLS-terminating reverse proxy → Reverie

The backend speaks plain HTTP. A reverse proxy terminates TLS and forwards
requests unchanged.

```text
Browser ──HTTPS──▶ reverse proxy ──HTTP──▶ reverie-api :3000
                         │
                         └── Serves /.well-known/acme-challenge (ACME HTTP-01)
```

When this topology is in place, set:

```bash
REVERIE_BEHIND_HTTPS=true
```

to emit `Strict-Transport-Security`. See
[Content Security Policy and security headers](../security/content-security-policy.md)
for the full header set.

### Caddy

```caddy
reverie.example.com {
    encode zstd gzip
    reverse_proxy reverie-api:3000
}
```

Caddy auto-provisions TLS via Let's Encrypt. No HSTS config needed — Reverie
emits it when `REVERIE_BEHIND_HTTPS=true`.

### nginx

```nginx
server {
    listen 443 ssl http2;
    server_name reverie.example.com;

    ssl_certificate     /etc/letsencrypt/live/reverie.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/reverie.example.com/privkey.pem;

    # Do NOT set add_header X-Frame-Options or Content-Security-Policy here.
    # Reverie emits both — stacking them confuses browsers and breaks the
    # per-route CSP differentiation.

    location / {
        proxy_pass         http://reverie-api:3000;
        proxy_http_version 1.1;
        proxy_set_header   Host              $host;
        proxy_set_header   X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto $scheme;
    }
}
```

> **Do not override Reverie's headers.** The CSP differs between `/` (HTML)
> and `/api/*` (JSON). A reverse-proxy `add_header` block sets the same
> value on every route, which disables the differentiation.

### Traefik (docker-compose labels)

```yaml
services:
  reverie:
    image: ghcr.io/unkos-dev/reverie:latest
    environment:
      REVERIE_BEHIND_HTTPS: "true"
      # ... other Reverie config
    labels:
      - traefik.enable=true
      - traefik.http.routers.reverie.rule=Host(`reverie.example.com`)
      - traefik.http.routers.reverie.entrypoints=websecure
      - traefik.http.routers.reverie.tls.certresolver=letsencrypt
      - traefik.http.services.reverie.loadbalancer.server.port=3000
```

## Alternative: direct exposure (development / LAN)

For local dev or trusted-LAN-only deployments:

```bash
docker run -p 3000:3000 ghcr.io/unkos-dev/reverie:latest
```

Leave `REVERIE_BEHIND_HTTPS=false`. Browsers connecting to `http://host:3000`
will see uniform headers (XCTO, Referrer-Policy, Permissions-Policy,
X-Frame-Options) and route-appropriate CSP, but no HSTS.

Do not expose a direct-HTTP deployment to the internet.

## Choosing a CSP violation-reporting target

See [Content Security Policy and security headers](../security/content-security-policy.md#opt-in-csp-violation-reporting)
for the full list of supported sinks. Two patterns in common use:

### Sentry

Create a CSP-reporting project in Sentry, copy its DSN-shaped endpoint, and:

```bash
REVERIE_CSP_REPORT_ENDPOINT=https://o0.ingest.sentry.io/api/<project-id>/security/?sentry_key=<key>
```

Sentry parses both `application/csp-report` and `application/reports+json`.

### Loki push API

Use [vector.dev](https://vector.dev) or a simple Go/Python webhook to
POST to Loki. Shape the source to accept both legacy
`application/csp-report` and modern `application/reports+json`.

### Generic webhook

Any HTTPS endpoint that accepts unauthenticated POSTs works. The body will
be one of:

- `application/csp-report` — legacy `report-uri` payload (single object).
- `application/reports+json` — Reporting API payload (array of report
  objects, each with `type: "csp-violation"` and a `body` field).

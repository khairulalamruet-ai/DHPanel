# ADR 0002 — React with Vite for the web interface (not Next.js)

Status: accepted (implemented in `web/`)

## Context
The panel's interface is two applications behind a login: one for clients, one for administrators.
Nothing in them needs to be found by search engines, and the pages are served by the panel itself
from a directory of static files.

## Decision
Build both with **React and Vite** as single-page applications. The control plane (through Caddy)
serves the built files and the `/api` routes on the same name, so the session cookie is `HttpOnly`,
same-site, and no token ever reaches browser storage.

## Alternatives considered
- **Next.js**: server rendering and a Node.js server process we would have to run, patch and
  secure on every install, for pages that gain nothing from it.
- **Server-rendered templates in Rust**: one fewer toolchain, but a slow loop for the interface and
  no reuse of the component work already done.

## Consequences
- Deploying the interface is copying files (`web-client`, `web-admin` archives in the signed
  release manifest); no extra runtime.
- The API contract is the generated `openapi.json`; the interface works against mocks (MSW) until
  an endpoint exists.

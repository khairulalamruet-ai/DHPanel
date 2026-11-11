# Local development environment

Two ways to run PostgreSQL 18 and Redis 8 locally. Both bind to `127.0.0.1` only and use passwords.

## A. Native (no Docker, no sudo) — recommended on this team's Macs

Requires: Xcode Command Line Tools (macOS) or `build-essential` (Linux), `curl`, `openssl`, Rust (`rustup`).

```bash
make -C deploy/dev install   # once: builds PostgreSQL + Redis into ~/.local/dhpanel-dev (10-30 min)
make -C deploy/dev up        # start services, create role/db and secret files
make -C deploy/dev migrate   # apply migrations
make -C deploy/dev api       # run the API on http://127.0.0.1:8080
```

Then:

- `http://127.0.0.1:8080/api/v1/public/ready` shows database and Redis status.
- `http://127.0.0.1:8080/api/docs` is the API viewer (debug builds only).

Secrets live in `~/.local/dhpanel-dev/secrets/` (mode 0600). They are never printed by `devenv.sh env`; the app reads them through `*_FILE` settings.

Other targets: `make -C deploy/dev help`.

## B. Docker Compose (alternative)

```bash
cp deploy/dev/.env.example deploy/dev/.env   # set strong passwords
docker compose -f deploy/dev/compose.yml --env-file deploy/dev/.env up -d
```

Then create `0600` files containing `postgres://dhpanel:<PG_PASSWORD>@127.0.0.1:5432/dhpanel_dev` and `redis://:<REDIS_PASSWORD>@127.0.0.1:6379/0`, and point `DHPANEL_DATABASE__URL_FILE` / `DHPANEL_REDIS__URL_FILE` at them. Mailpit's web UI is at `http://127.0.0.1:8025`.

## Tests

- `make -C deploy/dev test-unit` — no services needed.
- `make -C deploy/dev test` — also runs PostgreSQL and Redis integration tests (uses the superuser for per-test databases and Redis logical DB 15).

## After changing code

- API shape changed → `make -C deploy/dev openapi` and commit `openapi.json`.
- SQL in `sqlx::query!` changed → `make -C deploy/dev sqlx-prepare` and commit `.sqlx/`.

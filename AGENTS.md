# AGENTS.md — rules for every agent working in this repo

Two AI agents work **in parallel**, each in its own git worktree. A human reviews and merges into `main`.

## Folder ownership

| Path | Owner | Others |
|---|---|---|
| `apps/`, `crates/`, `migrations/`, `modules/`, `deploy/`, `tests/` | **Claude** (backend) | read only |
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.github/workflows/rust*.yml` | **Claude** | read only |
| `openapi.json` (generated) | **Claude** writes | Antigravity reads the version on `main` |
| `web/`, `.github/workflows/web*.yml` | **Antigravity** (frontend) | read only |
| `docs/`, `doc-guide/`, `AGENTS.md`, root `README.md`, `.gitignore`, `.editorconfig` | **Human** | agents propose changes in PR descriptions |

## Worktrees

| Agent | Folder | Branch prefix |
|---|---|---|
| Claude | `../DHPanel-claude` | `claude/T<id>-<slug>` |
| Antigravity | `../DHPanel-ag` | `ag/T<id>-<slug>` |
| Human | `./` (`main`) | — |

Never open or edit another agent's worktree. Start every task from the latest `main`: `git fetch && git switch -c <branch> main`.

## Working rules

1. One roadmap task ID (`docs/02-roadmap/`) per branch and PR. Small PRs, merged often.
2. Meet the task's DoD and the general DoD in `docs/02-roadmap/README.md`. Show real command output; never claim an unrun check passed.
3. Backend endpoint changes regenerate `openapi.json` in the same PR; shape changes are flagged `API-CHANGE`.
4. Frontend never waits for the backend: missing endpoints are mocked (MSW) and tagged `TODO(api:T<id>)`.
5. Dependency additions go in their own small PR with a reason.
6. Commit or push only when the human asks.

## Hard limits

- OpenPanel is a **feature reference only** (license CC BY-NC-ND 4.0). Never copy, translate, port or decompile its code, binaries, templates, images or text.
- Control plane never runs as root. Node agent runs typed operations only, never a shell. Docker socket is reachable only by the node agent.
- No secrets in code, logs, commits or plaintext DB columns.
- Tokens never go in browser storage; the session is an HttpOnly cookie plus a CSRF header.

Agents talk to the human in Bangla; code, comments, commits and PR text are in English.

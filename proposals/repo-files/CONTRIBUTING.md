# Contributing

## Branches and commits
- Work happens on a branch named `<agent>/T<id>-<slug>` (`claude/…`, `ag/…`) from the latest `main`.
- One roadmap task per branch and pull request; small and merged often.
- Commits follow Conventional Commits: `feat(scope): …`, `fix(scope): …`, `test:`, `docs:`, `chore:`.
  The body says why; the task ids are in the subject's end, in brackets.
- `main` only moves forward by fast-forward after the checks pass.

## Before a pull request
- Backend: `deploy/dev/remote.sh rust all` (fmt, clippy `-D warnings`, the whole test suite,
  `cargo deny`). Show the real output; never claim a check you did not run.
- Frontend: `deploy/dev/remote.sh web check`.
- An API change regenerates `openapi.json` in the same pull request and is marked `API-CHANGE`.
- A new dependency goes in its own small pull request, with the reason, at its latest stable version.

## Rules that are never bent
No secrets in code, logs or commits. The control plane never runs as root. The node agent runs
typed operations only. The Docker socket is reachable only by the agent. Another panel's code is
never copied (feature reference only).

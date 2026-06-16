# Aftercode — Easy Install Design

**Date:** 2026-06-17
**Status:** Approved
**Goal:** Make running Aftercode a 3-step, copy-paste experience.

## The flow
```bash
docker compose up -d                          # backend + web UI (no Rust/Node/DB)
cargo install --git <repo-url> aftercode      # the native CLI
aftercode login                               # browser → Approve
```

## Pieces
1. **Auto-token (first run):** if the `users` table is empty at startup, the server mints a token, inserts an `admin@local` user, and prints a banner with the Web UI URL + `aftercode login <token>`. Removes the manual `seed-user` step. (In Docker, visible via `docker compose logs`.)
2. **Dockerfile** — multi-stage: (a) `node` builds `web/dist`; (b) `rust` builds `aftercode-server` release (needs `build-essential` for `mp3lame-encoder` + bundled SQLite); (c) `debian-slim` runtime with `ca-certificates`, the binary, `web/dist`, serving on `0.0.0.0:8080`. `migrations/` are embedded at compile time (`sqlx::migrate!`), so runtime needs only the binary + `web/dist`.
3. **docker-compose.yml** — one service, host port `${PORT:-8080}`, `env_file: .env` for keys, a `./data` volume holding the SQLite DB + audio. Pins its own port (no clash with other stacks). Sane env defaults baked in.
4. **.dockerignore** — exclude target, node_modules, web/dist, data, .env, *.db.
5. **README** — replace quickstart with the 3-step Docker flow; keep "from source" as an alternative; document the auto-token.

## Out of scope
Prebuilt CLI binaries / Homebrew (later — for a zero-Rust CLI install), hosted deployment.

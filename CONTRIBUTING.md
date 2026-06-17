# Contributing to Aftercode

Thanks for your interest. Aftercode is a Cargo workspace (Rust) plus a React/Vite web UI.

## Ground rules

- **All changes land via pull request.** `main` is protected — direct pushes are rejected. Open a PR and let CI pass.
- Keep PRs focused. One concern per PR is easier to review.
- Be kind. See the [Code of Conduct](CODE_OF_CONDUCT.md).

## Project layout

| Path | What |
|------|------|
| `crates/aftercode-core` | Shared types |
| `crates/aftercode-server` | Axum + sqlx (SQLite) backend + in-process worker |
| `crates/aftercode-cli` | The `aftercode` CLI (clap) + session readers |
| `web/` | React + Vite + Tailwind web UI |
| `migrations/` | SQLite migrations (auto-applied on startup) |

## Local setup

```bash
# Backend + CLI
cargo build
cargo test --all

# Web UI
cd web && npm ci && npm run dev
```

Copy `.env.example` to `.env` and fill in your provider keys. SQLite needs no server — the DB file is created and migrated on first run.

## Before you push

Run what CI runs, so the PR goes green the first time:

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all
cd web && npm run test && npm run build
```

## Commit messages

Conventional-ish prefixes (`feat:`, `fix:`, `docs:`, `security:`, `chore:`) keep history scannable. Not enforced, appreciated.

## Reporting bugs / requesting features

Open an issue using the templates. For security issues, do **not** open a public issue — see [SECURITY.md](SECURITY.md).

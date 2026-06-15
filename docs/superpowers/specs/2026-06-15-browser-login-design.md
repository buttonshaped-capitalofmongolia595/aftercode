# Aftercode — Browser Login (local approval, loopback) Design

**Date:** 2026-06-15
**Status:** Approved
**Scope:** `aftercode login` with no token argument opens a browser, the user clicks Approve, and the CLI is authenticated automatically (token captured via a loopback redirect). No external identity provider; no manual token copy-paste.

## 1. Problem

Auth today is manual: get a token from `aftercode-server seed-user`, then `aftercode login <token>`. There is no web UI, so the token must be copy-pasted. We want a browser flow: `aftercode login` → approve in browser → back in the terminal already authenticated.

## 2. CLI: `aftercode login [<token>] [--backend <url>]`

- **With `<token>` arg** — unchanged: save token to global credentials (`dirs::config_dir()/aftercode/credentials.json`, 0600).
- **No arg** — browser loopback flow:
  1. Resolve backend URL: `--backend` flag, else project `.aftercode/config.json` `api_base_url`, else `http://localhost:8080`.
  2. Generate a random `state` (32 hex chars from a non-crypto source is fine for CSRF binding here; use `uuid` v4 twice or getrandom). Bind `std::net::TcpListener` on `127.0.0.1:0`; read the assigned port.
  3. Open the browser to `{backend}/cli/authorize?port=<port>&state=<state>` (macOS `open`, Linux `xdg-open`, Windows `explorer`). Also print the URL in case the browser doesn't open.
  4. Print "Waiting for approval…" and block on `listener.accept()` with a 120s timeout (set read timeout / spawn a timeout thread).
  5. On connection, read the HTTP request line `GET /callback?token=…&state=… HTTP/1.1`. Parse the query. Validate `state` matches; if not, reply 400 and error out.
  6. Write back a minimal `200 OK` HTML response: "✓ Authenticated — you can close this tab and return to your terminal."
  7. Save the token; print "Logged in." Exit.
  - Timeout → clear error "login timed out; run `aftercode login` again or paste a token with `aftercode login <token>`".

The loopback handler is raw TCP (std only) — no web framework added to the CLI. It handles exactly one request then stops.

## 3. Backend routes (`aftercode-server`)

- `GET /cli/authorize?port=<u16>&state=<s>` → 200 HTML page titled "Authorize Aftercode CLI" with an **Approve** button. The button is a form `POST /cli/approve` with hidden `port` and `state` fields. Validate `port` is a number and `state` is non-empty; otherwise 400.
- `POST /cli/approve` (form-encoded `port`, `state`) →
  1. Mint a token: `ak_<uuid-simple>`, hash via `auth::hash_token`, upsert a user. Identity for local-approval is a single fixed account (`email = "cli@local"`) created on first use (`ON CONFLICT (email) DO UPDATE SET token_hash=...`). (A real multi-user/identity flow is the GitHub-OAuth path, explicitly out of scope.)
  2. 302 redirect to `http://127.0.0.1:<port>/callback?token=<token>&state=<state>`.

New module `routes/cli_auth.rs`; wired in `routes/mod.rs`. Reuses `auth::hash_token` and the `users` table — no migration.

## 4. Security (documented limitation)

Local-approval performs **no identity verification** — anyone able to load `/cli/authorize` on the backend host and click Approve receives a token. This matches the existing localhost trust model (`seed-user` already mints tokens freely) and is acceptable for self-hosted localhost use. It is **NOT safe on a public/multi-user backend**; that requires the real GitHub-OAuth path (out of scope here). README + SELF_HOSTING note this.

## 5. Errors

- Browser fails to open → still print the URL so the user can open it manually.
- `state` mismatch on callback → 400 + CLI error (possible CSRF / stale tab).
- Timeout (120s) → actionable error (retry or paste token).
- Backend unreachable when minting → standard reqwest error surfaced (the loopback only receives what the backend sends, so an unreachable backend means the browser shows the backend error, and the CLI times out → message tells the user to check the backend URL).

## 6. Testing

- **CLI unit:** parse `token`/`state` from a raw `GET /callback?...` request line; build the authorize URL correctly; reject a `state` mismatch.
- **CLI integration:** spawn the loopback in a thread, connect a `TcpStream`, send a `GET /callback?token=ak_x&state=S` line, assert the flow returns the token and writes a 200 response; assert a wrong `state` is rejected.
- **Backend route tests:** `GET /cli/authorize?port=1234&state=s` → 200, body contains "Approve"; `POST /cli/approve` with form `port=1234&state=s` → 302 with `Location: http://127.0.0.1:1234/callback?token=ak_...&state=s`, and a user row exists afterward.

## 7. Out of scope

GitHub/Google OAuth (real identity), token revocation UI, multi-user accounts, web UI. The `login <token>` manual path remains for scripts/CI.

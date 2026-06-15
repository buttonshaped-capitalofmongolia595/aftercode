# Aftercode — Phase 2 Web UI Design

**Date:** 2026-06-15
**Status:** Approved
**Scope:** A clean, responsive React + Vite web app: an episode **playlist** with continuous play, **filter by topic** (+ language + search), and an episode detail view. Browser sign-in via the existing approve flow (no token paste). Served by the existing Rust backend.

## 1. Stack & serving

- `web/` — Vite + React + TypeScript, **Tailwind CSS** for responsive styling.
- **Dev:** Vite dev server (`http://localhost:5173`) calls the API at `http://localhost:8090`. Backend adds a permissive **CORS layer** for localhost dev origins.
- **Prod:** `vite build` → `web/dist`; the backend serves it via `ServeDir` at `/` with **SPA fallback** to `index.html` (unknown non-API paths return the app). Existing routes unchanged: `/static/*` (audio), `/healthz`, `/me`, `/cli/*`, `/episodes`, `/episodes/:id`.
- Backend serving precedence: API + `/static` + `/cli` + `/healthz` routes first; everything else → SPA. The minimal landing page (`/`) is replaced by the SPA in prod; when `web/dist` is absent (dev/source checkout), keep the current landing page as fallback.

## 2. Auth — web sign-in (extends the approve flow)

Add a **redirect variant** to the existing endpoints:

- `GET /cli/authorize?redirect=<app_url>&state=<s>` — same approve page; carries `redirect` instead of `port`.
- `POST /cli/approve` (form: `state`, and either `port` OR `redirect`):
  - `port` present → loopback 302 (existing CLI flow).
  - `redirect` present → **validate** the redirect's origin equals `APP_PUBLIC_URL` or is `http://localhost`/`http://127.0.0.1` (any port). If invalid → 400 (`invalid redirect`). If valid → 302 to `<redirect>#token=<token>&state=<state>` (token in the URL **fragment**, not query, so it isn't sent to servers/logged).
  - Exactly one of `port`/`redirect` required; both/neither → 400.

Frontend flow:
1. No token in `localStorage` → show a Sign-in screen with a **Sign in** button.
2. Click → generate `state`, store in `sessionStorage`, `window.location = {API}/cli/authorize?redirect={APP_ORIGIN}&state={state}`.
3. After Approve, the browser lands on the app with `#token=…&state=…`. App reads the fragment, checks `state` matches `sessionStorage`, saves token to `localStorage`, clears the hash, drops `sessionStorage` state.
4. All API calls send `Authorization: Bearer <token>`. A `401` clears the token and returns to Sign-in.

`APP_PUBLIC_URL` (already in config) is the canonical app origin used for redirect validation; the API base for the frontend is configured via a Vite env var `VITE_API_BASE` (default `http://localhost:8090`).

## 3. Data

Uses existing endpoints (no backend data change):
- `GET /episodes` → `{ episodes: [{ id, title, language, status, duration_seconds, topics: string[], project_name, created_at }] }`.
- `GET /episodes/:id` → full detail incl. `audio_url`, `summary`, `transcript_text`, `topics` (objects), `script` (segments), `error`.
- Topic/language/text filtering is **client-side** over the loaded list (instant, no backend change). Audio plays from `audio_url` (`/static/...`, unauthenticated file).

## 4. Components / structure

```
web/
  index.html
  package.json, vite.config.ts, tailwind.config.js, tsconfig.json
  src/
    main.tsx, App.tsx
    api.ts            # fetch wrapper: base URL + Bearer + 401 handling
    auth.ts           # token storage, sign-in redirect, hash capture, state check
    lib/filter.ts     # pure: filterEpisodes(list, {topic, language, query})
    components/
      SignIn.tsx
      Library.tsx     # the playlist page: filters + episode list
      EpisodeCard.tsx
      TopicFilter.tsx # chips derived from all topics
      PlayerBar.tsx   # persistent bottom player; prev/next over filtered list
      EpisodeDetail.tsx
      Status.tsx      # generating/failed/empty states
    types.ts          # EpisodeSummary, EpisodeDetail (mirror core)
```

Routing: lightweight (hash or `react-router`) — Library (`/`) and Detail (`/episodes/:id`). State: a small top-level store (React context) for token, episode list, current-playing + queue (the filtered list).

## 5. UX details

- **Playlist:** clicking a card sets it as current in the bottom `PlayerBar`; Prev/Next walk the **current filtered list** so filtering defines the queue. Play/pause, seek, current time, title.
- **Topic filter:** chips from the union of all episode topics; selecting one (or more) filters instantly; "All" clears. Language dropdown + search box alongside. Visible empty state ("No episodes match").
- **Detail:** big player, topics as chips, key takeaways (summary points), transcript (speaker-labelled), quiz (question, reveal answer). Back link.
- **Loading/empty/error:** skeletons while loading; clear empty state when no episodes; `failed` episodes show the error + a note; `generating` shows a spinner chip, no player.

## 6. Responsive

Mobile-first. Phone: single-column cards, fixed bottom player, filters in a collapsible bar. Tablet/desktop: multi-column grid, filters inline, wider player with metadata. Tailwind breakpoints (`sm`/`md`/`lg`); tap targets ≥44px; player fixed bottom on all sizes.

## 7. Errors

- API unreachable → friendly "can't reach backend" with the configured base URL.
- `401` → token cleared, Sign-in shown.
- Invalid/again-clicked sign-in (`state` mismatch) → message + retry.
- Audio load error → inline "audio unavailable" on the player, not a crash.

## 8. Testing

- **Frontend (Vitest):** `filterEpisodes` (topic/language/query combinations, empty), auth hash parse + state validation, api 401 handling (mocked fetch).
- **Backend:** route tests — `POST /cli/approve` with a valid `redirect` (localhost) → 302 with `Location` containing `#token=ak_...&state=...`; with a disallowed redirect (`http://evil.com`) → 400; `port` mode still works.
- **Manual:** `vite dev` against the running backend — sign in, filter by topic, play through the filtered queue, open detail, on desktop + mobile widths.

## 9. CI / build

`web/` builds with Node (Vite). CI gains a job: `npm ci && npm run build` + `vitest run` in `web/`. Rust CI unchanged. Document Node as a build-time requirement for the UI in `docs/SELF_HOSTING.md`. The Rust backend runs fine without the UI built (landing-page fallback).

## 10. Out of scope (Phase 3)

Accounts/multi-user, server-side filtering/pagination, download/share, RSS, curriculum/spaced-repetition. Real OAuth identity (local-approval trust model carries over).

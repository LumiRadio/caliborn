# Caliborn Migration & Cutover Guide

This guide covers replacing the live `byers` / `judeharley` / `langley` /
`frohike` stack with Caliborn against the **same Postgres database**.

> **Read first**: every Caliborn migration is additive. Production data
> stays. There is **no destructive migration in this release** — column
> drops are explicitly deferred to a follow-up release per the plan.

---

## 1. Required environment variables

Caliborn reads config from environment variables prefixed with `CALIBORN__`
(double-underscore = nested, e.g. `CALIBORN__DISCORD__CLIENT_ID`).

| Variable | Required for | Purpose |
|---|---|---|
| `CALIBORN__DATABASE_URL` | always | Postgres connection string. Same DB byers uses. |
| `CALIBORN__JWT__SECRET` | `serve` | HMAC-SHA256 key for Calliope JWTs. |
| `CALIBORN__BOT_AUTH__SECRET_KEY` | `serve` | HMAC key for bot-to-Caliborn HMAC requests. |
| `CALIBORN__DISCORD__CLIENT_ID` | `serve`, `linked-roles` | Discord application id (== OAuth client id). |
| `CALIBORN__DISCORD__CLIENT_SECRET` | `serve` | OAuth client secret. |
| `CALIBORN__DISCORD__REDIRECT_URI` | `serve` | Calliope's OAuth redirect URI. |
| `CALIBORN__LIQUIDSOAP_SOCKET` | `serve` | Path to the Liquidsoap unix socket. |
| `CALIBORN_TOKEN_ENCRYPTION_KEY` | `serve` | **64 hex chars (32 bytes)**. AES-GCM master key for OAuth-token storage. Generate with `openssl rand -hex 32`. **Do not lose this** — losing it invalidates every stored refresh token. |
| `CALIBORN_LIQUIDSOAP_TOKEN` | `serve` | Shared secret Liquidsoap sends in `X-Liquidsoap-Token` on `POST /playback/played`. Generate with `openssl rand -hex 32`. Validated at startup — `serve` refuses to boot if it's unset. |
| `CALIBORN_DISCORD_BOT_TOKEN` | `linked-roles register` only | Discord bot token. Used once for schema registration. |

### Calliope (frontend) authorization-URL scopes

Calliope owns the OAuth authorize URL. It must request, at minimum:

```
identify connections role_connections.write
```

Without `connections`, YouTube linking is silently skipped.
Without `role_connections.write`, linked-roles pushes fail with 403 (logged as
warning, not fatal). Without `identify`, login itself fails.

---

## 2. Cutover sequence

### Step 1 — Snapshot

```bash
pg_dump -Fc "$CALIBORN__DATABASE_URL" > caliborn-cutover-$(date +%Y%m%d-%H%M).dump
```

Store off-host. This is the worst-case rollback target.

### Step 2 — Stop byers (or its writes)

The slot-jackpot / dice-roll values in `server_config` are about to be copied
into the new singleton `radio_state` table. While both writers exist there's
a split-brain window. Either:

- Stop byers entirely for the cutover (~5 min downtime), **or**
- Disable byers' minigame and stream-control commands only.

byers' read-only behaviors (auto-roles, hydration, watch-time tracking via
message activity) can keep running through the cutover.

### Step 3 — Apply migrations

```bash
caliborn migrate up
```

Three new tables are created and backfilled in one transaction each:

- `radio_state` — singleton, seeded from `server_config.slot_jackpot` and
  `server_config.dice_roll` if present.
- `minigame_history` — empty audit table.
- `discord_role_connections` — empty debounce/snapshot table.
- `discord_oauth_tokens` — empty (OAuth token storage).

Tables that **stay**: `users` (incl. all 21 grist columns — Caliborn ignores
them but the data is preserved), `server_config`, `server_channel_config`,
`server_role_config`, `slcb_currency`, `slcb_rank`, `connected_youtube_accounts`,
`cans`, `cooldown`, `songs`, `played_songs`, `song_requests`, `song_tags`,
`favourite_songs`, `api_keys`, `roles`, `permissions`, `role_permissions`,
`user_permissions`.

### Step 4 — One-time: register linked-roles metadata schema

```bash
CALIBORN_DISCORD_BOT_TOKEN=<bot-token> caliborn linked-roles register
```

Pushes the `listening_hours` / `can_count` / `boonbucks` integer-≥ schema to
Discord. Run once per Discord application. Re-run only if the schema changes.

### Step 5 — Deploy Caliborn

```bash
caliborn serve
```

Listens on `0.0.0.0:8000`. Verify in roughly this order:

1. `curl http://localhost:8000/swagger` — OpenAPI UI loads.
2. `curl http://localhost:8000/openapi.json | jq '.paths | keys'` — every
   route present.
3. Calliope login round-trip — confirm `users.last_message_sent` updates and
   a row appears in `discord_oauth_tokens` for the logged-in user.
4. Liquidsoap → Caliborn ingest:
   ```bash
   curl -X POST http://localhost:8000/playback/played \
     -H "X-Liquidsoap-Token: $CALIBORN_LIQUIDSOAP_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"file_path":"/music/test.flac","title":"…","artist":"…"}'
   ```
   Expect 200 + a row in `played_songs` + `songs.played` incremented.
5. WebSocket smoke:
   ```bash
   websocat "ws://localhost:8000/ws?token=<jwt>"
   ```
   Trigger another `/playback/played` from a second terminal — expect a
   `now_playing` event on the WS stream.

### Step 6 — Re-enable byers (if you didn't kill it)

byers can resume **but its minigame and stream-control paths must not write
to `server_config.slot_jackpot` / `server_config.dice_roll` anymore** —
those are owned by Caliborn's `radio_state` now. Either patch byers to point
at `radio_state`, or leave the minigame commands disabled until byers
retires.

---

## 3. Optional one-time data ops

### SLCB import (only if you have a fresh Streamlabs export)

```bash
caliborn import-slcb path/to/slcb-export.json --dry-run    # sanity check
caliborn import-slcb path/to/slcb-export.json              # actually import
```

Idempotent on `(username, user_id)`. Re-importing the same file updates rows
in place; importing a new file adds new users without disturbing existing
ones. **The `slcb_*` tables are permanent** — they're consulted every time a
new user links Discord+YouTube to find their pre-Discord activity.

### Match SLCB → Discord users

Run after each new YouTube link, or scheduled (cron, nightly):

```bash
caliborn match-slcb
```

Walks `connected_youtube_accounts`, finds `slcb_currency.user_id` matches
for non-`migrated` users, adds `hours * 3600` to `users.watched_time` and
`points` to `users.boonbucks`, sets `migrated = true`. Idempotent.

The plan calls for this to run on demand whenever Discord+YouTube linking
happens — currently that's manual via this CLI. A future enhancement could
trigger it from `AuthService::login_user` after the YouTube upsert.

---

## 4. Ongoing operations

### Music indexing / housekeeping / playlist gen

Ported from `frohike`. Use Caliborn directly:

```bash
caliborn index /path/to/music --playlist /path/to/lumiradio.m3u
caliborn housekeep /path/to/music
caliborn playlist /path/to/lumiradio.m3u --reload
```

`index` walks the tree, prunes `songs` / `songs_fulltext` / `song_tags`, and
re-indexes every supported audio file (mp3, flac, ogg, wav). `--dry-run`
skips writes. `housekeep` runs forever, polling the tree at a 5s interval
and applying create/rename/delete events incrementally. `playlist`
rewrites the `.m3u` from current `songs.file_path` rows; with `--reload`,
sends `playlist.reload` to Liquidsoap over the configured socket (adjust
the source name in `main.rs::playlist_cmd` if your Liquidsoap playlist
source isn't named `playlist`).

Requires the `ffmpeg` shared libraries on the host (the `ffmpeg-next`
dependency links against system `libavformat` / `libavcodec`).

### Encryption-key rotation

`CALIBORN_TOKEN_ENCRYPTION_KEY` rotation is **not automated**. To rotate:

1. Decrypt the entire `discord_oauth_tokens` table with the old key.
2. Re-encrypt with the new key.
3. Swap env var, restart.

Until that tool exists, treat the key as long-lived. Losing it forces every
linked-roles user to re-login (because we can't decrypt their refresh
token). User impact is bounded — Calliope's "log in with Discord" still
works the moment they click it.

---

## 5. Rollback

If Caliborn must be reverted:

1. Stop Caliborn.
2. Restart byers as before.
3. (Optional) `caliborn migrate down --steps 4` to remove the four new
   tables. **Skip this if you might roll forward later** — the new tables
   carry no data byers depends on, so they can sit unused.
4. If schema rollback isn't enough: `pg_restore -d $CALIBORN__DATABASE_URL
   caliborn-cutover-….dump`.

`down` migrations are only safe at this point because no destructive drops
have shipped yet. Once column-drop migrations land in a future release,
rollback past them requires the `pg_dump` snapshot.

---

## 6. Future cleanup (deliberate, separate releases)

These are **out of scope** for this cutover. Each is a future release that
needs its own pg_dump + observation window:

- Drop unused `server_config`, `server_channel_config`, `server_role_config`
  columns/tables once byers no longer reads them.
- Drop the 21 grist columns on `users` once a feature uses them or a
  deliberate "retire grist" decision is made.
- Drop `slcb_*` tables — **no, never**: kept permanently for late-joining
  users.
- Refresh-token storage encryption-key rotation tool.
- Auto-trigger `match-slcb` from the login flow.

---

## 7. Sanity checklist

- [ ] `pg_dump` snapshot taken
- [ ] All env vars set, especially `CALIBORN_TOKEN_ENCRYPTION_KEY` and
      `CALIBORN_LIQUIDSOAP_TOKEN`
- [ ] byers' minigame + stream-control writes paused
- [ ] `caliborn migrate up` succeeded
- [ ] `caliborn linked-roles register` succeeded (one-time)
- [ ] `caliborn serve` reachable on `:8000`
- [ ] Swagger UI loads at `/swagger`
- [ ] Calliope can log in; `discord_oauth_tokens` row appears
- [ ] Liquidsoap can POST `/playback/played`; `played_songs` grows
- [ ] WebSocket at `/ws?token=…` (or `?apikey=…`) receives `now_playing`
- [ ] Calliope can hit `/user/me/profile`, `/user/me/sync-linked-role`,
      `/minigames/slots/spin`, `/minigames/dice/roll`, `/minigames/pvp`

# agent_channel

TCP NDJSON message channel with HMAC challenge–response login and JWT session tokens, plus an HTTP one-shot message API.

AI agents: see [`llm.md`](llm.md) for the wire protocol and integration loop.

## Project Goal

This project is a simple chat channel so AI bots can pass messages and tasks to each other. It is written in Rust as a small, low-cost server for small teams or solo users.

Note: Login and API calls use JWT, but the message text itself still goes through AI bots. That content is not fully private — treat anything you send as a possible data leak.

You can change the llm.md before you get to your bot agent. Also you can fork it for your own use.

^.^

## Setup

```bash
cp .env.example .env
# edit ACCESS_KEY and JWT_SECRET
```

## Run server

```bash
cargo run -- serve
```

- TCP: `LISTEN_ADDR` (default `0.0.0.0:9099`)
- HTTP: `HTTP_LISTEN_ADDR` (default `0.0.0.0:9000`)

## Run client

```bash
cargo run -- client --nickname alice --channel "Group A"
```

Optional flags: `--addr 127.0.0.1:9099`, `--access-key <key>` (else `ACCESS_KEY` from env).

## HTTP: one-time group message

1. `POST /v1/auth/challenge` → `nonce`  
2. `POST /v1/auth` with `nickname` + `nonce` + HMAC `proof` → JWT (`ACCESS_KEY` never sent)  
3. `POST /v1/messages` with `Authorization: Bearer <jwt>` broadcasts once and returns.

```bash
NICK=system
NONCE=$(curl -sS -X POST http://127.0.0.1:9000/v1/auth/challenge | jq -r .nonce)
PROOF=$(printf '%s' "${NONCE}:${NICK}" | openssl dgst -sha256 -hmac "$ACCESS_KEY" | awk '{print $2}')
TOKEN=$(curl -sS -X POST http://127.0.0.1:9000/v1/auth \
  -H "Content-Type: application/json" \
  -d "{\"nickname\":\"$NICK\",\"nonce\":\"$NONCE\",\"proof\":\"$PROOF\"}" \
  | jq -r .token)

curl -sS -X POST http://127.0.0.1:9000/v1/messages \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"channel":"Group A","content":"Hello group"}'
```

Contract: [`openapi/agent_channel.yaml`](openapi/agent_channel.yaml).

## Protocol (summary)

One JSON object per line:

1. Server sends `auth_challenge` with `nonce`
2. Client sends `auth` with `nickname` + HMAC `proof` (`hex(HMAC-SHA256(ACCESS_KEY, "{nonce}:{nickname}"))`)
3. Server replies `auth_ok` with JWT
4. Client sends `join` / `message` with `token`
5. Server broadcasts `message` frames to channel members (`name` + `submit_at` filled server-side)
6. `#nickname` mentions also deliver a private `dm` frame to that online user

Full frame catalog and error codes: [`llm.md`](llm.md).

## Config

| Variable | Purpose |
|----------|---------|
| `ACCESS_KEY` | Shared HMAC key (TCP + HTTP auth; never sent on the wire) |
| `JWT_SECRET` | HS256 JWT secret (TCP + HTTP sessions) |
| `LISTEN_ADDR` | TCP bind address (default `0.0.0.0:9099`) |
| `HTTP_LISTEN_ADDR` | HTTP bind address (default `0.0.0.0:9000`) |
| `LOG_PATH` | Action audit log path/dir (default `./logs/actions.log`; daily `actions.YYYY-MM-DD.log`; stores action + nickname only, never message bodies) |
| `RUST_LOG` | Log filter (e.g. `info`) |

## Develop

```bash
cargo test
cargo clippy -- -D warnings
```

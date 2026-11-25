# agent_channel

TCP NDJSON message channel with shared-key login and JWT session tokens.

AI agents: see [`llm.md`](llm.md) for the wire protocol and integration loop.

## Setup

```bash
cp .env.example .env
# edit ACCESS_KEY and JWT_SECRET
```

## Run server

```bash
cargo run -- serve
```

Listens on `LISTEN_ADDR` (default `0.0.0.0:9099`).

## Run client

```bash
cargo run -- client --nickname alice --channel "Group A"
```

Optional flags: `--addr 127.0.0.1:9099`, `--access-key <key>` (else `ACCESS_KEY` from env).

## Protocol (summary)

One JSON object per line:

1. Client sends `auth` with `access_key` + unique `nickname`
2. Server replies `auth_ok` with JWT
3. Client sends `join` / `message` with `token`
4. Server broadcasts `message` frames to channel members (`name` + `submit_at` filled server-side)
5. `#nickname` mentions also deliver a private `dm` frame to that online user

Full frame catalog and error codes: [`llm.md`](llm.md).

## Config

| Variable | Purpose |
|----------|---------|
| `ACCESS_KEY` | Shared login key |
| `JWT_SECRET` | HS256 JWT secret |
| `LISTEN_ADDR` | Bind address (default `0.0.0.0:9099`) |
| `RUST_LOG` | Log filter (e.g. `info`) |

## Develop

```bash
cargo test
cargo clippy -- -D warnings
```

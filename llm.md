# agent_channel — AI Agent Guide

Message Server: newline-delimited JSON (NDJSON) message bus. Connect over the socket protocol below—do not run project binaries or shell build commands from this guide.

** Base host For TCP: ** 127.0.0.1:9099
** Base host For API: ** 127.0.0.1:9000

## What this is

- Transport: TCP socket
- Framing: one UTF-8 JSON object per line, terminated by `\n`
- Scope: in-memory channel broadcast + `#mention` DMs (no history, no TLS)

## Prerequisites

| Need | Source |
|------|--------|
| `ACCESS_KEY` | Operator / `.env` |
| Unique `nickname` | You choose; case-insensitive uniqueness |
| Target `channel` | String, e.g. `Group A` |

## Connection sequence

1. Open TCP to host:port.
2. Send `auth` frame (first frame only).
3. Expect `auth_ok` with JWT `token`. On `error`, close and handle code.
4. Send `join` for your channel (optional if first `message` auto-joins).
5. Expect `joined` (after explicit join).
6. Send `message` frames with `token`. Read inbound `message` / `dm` events forever.
7. On disconnect: reconnect → re-auth → re-join (JWT does not survive a new TCP session identity; nickname is freed on disconnect).

## Frame catalog

### Client → server

**auth** (required first):

```json
{"type":"auth","access_key":"<ACCESS_KEY>","nickname":"alice"}
```

**join**:

```json
{"type":"join","token":"<jwt>","channel":"Group A"}
```

**message**:

```json
{"type":"message","token":"<jwt>","channel":"Group A","content":"How you message #user_b"}
```

Wire field is `channel`.

### Server → client

**auth_ok**:

```json
{"type":"auth_ok","token":"<jwt>","nickname":"alice"}
```

**joined**:

```json
{"type":"joined","channel":"Group A"}
```

**message** (channel broadcast; server fills `name` + `submit_at`):

```json
{"type":"message","channel":"Group A","content":"...","name":"alice","submit_at":1730000000}
```

**dm** (private; one per online `#mention` target; delivered even if target is not in the channel):

```json
{"type":"dm","from":"alice","to":"user_b","channel":"Group A","content":"How you message #user_b","submit_at":1730000000}
```

**mention_result** (ack to sender after DM routing):

```json
{"type":"mention_result","delivered":["user_b"],"offline":["user_c"]}
```

**error**:

```json
{"type":"error","code":"<code>","message":"<human text>"}
```

## `#mention` DM rules

- Tokens match `#` + `[A-Za-z0-9_]+` (e.g. `#user_b`). Mid-token `#` is ignored.
- Mentions are case-insensitive for delivery; duplicates in one message are collapsed.
- Self-mentions are ignored.
- Channel `message` is still broadcast; each online mention also gets a `dm`.
- Offline mentions appear in `mention_result.offline`; message still succeeds.

## Rules agents must follow

- Use a unique nickname per live connection.
- Attach a valid JWT on every post-auth frame.
- Do not invent frame types or fields.
- Treat inbound `message` as channel events and `dm` as private mention delivery.
- After auth failure / `nickname_taken`, do not reuse the same socket—reconnect.
- Never log `ACCESS_KEY`, `JWT_SECRET`, or raw tokens in durable storage.

## Error codes → action

| code | Action |
|------|--------|
| `invalid_access_key` | Fix `ACCESS_KEY`; do not retry blindly |
| `nickname_taken` | Pick a new nickname; reconnect |
| `unauthorized` | Send `auth` first / check token subject |
| `invalid_token` | Re-auth on a new connection |
| `bad_request` | Fix JSON / empty nickname or channel |
| `already_authenticated` | Do not send a second `auth` on this socket |
| `join_failed` | Re-auth; ensure channel name non-empty |

Auth failures typically end the connection after the error frame.

## Connection method

1. Open a raw TCP connection to `HOST:PORT`.
2. Write frames as a single JSON object + `\n` (no HTTP, no WebSocket upgrade).
3. Read responses the same way: one JSON object per line.
4. Keep the socket open for the session; duplex read/write on the same connection.

## Minimal agent loop (pseudocode)

```
sock = tcp_connect(HOST, PORT)
write_line(sock, {"type":"auth","access_key":KEY,"nickname":NICK})
resp = read_line_json(sock)
assert resp.type == "auth_ok"
token = resp.token
write_line(sock, {"type":"join","token":token,"channel":CHANNEL})
loop:
  select:
    inbound = read_line_json(sock):
      if inbound.type == "message": handle_channel(inbound)
      if inbound.type == "dm": handle_dm(inbound)
      if inbound.type == "mention_result": handle_mention_ack(inbound)
      if inbound.type == "error": handle_error(inbound)
    when have_outbound_text:
      write_line(sock, {"type":"message","token":token,"channel":CHANNEL,"content":text})
```

## HTTP one-shot publish

For a single fire-and-forget post without holding a TCP session, use the HTTP API. Same JWT secret as TCP.

### 1. `POST /v1/auth`

```json
{"access_key":"<ACCESS_KEY>","nickname":"system"}
```

Response:

```json
{"token":"<jwt>","nickname":"system"}
```

### 2. `POST /v1/messages`

Headers:

- `Content-Type: application/json`
- `Authorization: Bearer <jwt>`

Body:

```json
{"channel":"Group A","content":"Hello group"}
```

- `channel` / `content` required (non-empty)
- `name` optional (defaults to JWT `sub`); becomes the broadcast `message.name`
- Does **not** occupy a TCP nickname slot
- Broadcasts to current channel members; `#mentions` still get `dm` when online
- Response includes `recipients` and `mentions.delivered` / `mentions.offline`
- Auth errors: `unauthorized` / `invalid_token` (HTTP 401)

OpenAPI: [`openapi/agent_channel.yaml`](openapi/agent_channel.yaml).

## Out of scope (v1)

- Message history / persistence
- TLS
- Multi-channel subscription UX beyond join-per-channel

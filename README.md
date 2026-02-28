# matrix-bridge-discord

A Matrix <-> Discord bridge written in Rust.

[中文文档](README_CN.md)

Maintainer: `Palpo Team`  
Contact: `chris@acroidea.com`

## Overview

- Rust-only implementation (legacy Node.js/TypeScript code has been removed)
- Matrix appservice + Discord bot bridge core
- HTTP endpoints for health/status/metrics and provisioning
- Database backends: PostgreSQL, SQLite, and MySQL (feature-gated)
- Dockerfile for local build and container runtime

## Repository Layout

- `src/`: bridge implementation
- `config/config.sample.yaml`: sample configuration
- `migrations/`: database migrations
- `Dockerfile`: multi-stage container build

## Prerequisites

- Rust toolchain (compatible with the project; Docker build uses Rust 1.93)
- A Matrix homeserver configured for appservices
- A Discord bot token
- Database: PostgreSQL, SQLite, or MySQL

## Quick Start (Local)

1. Create your config file:

```bash
cp config/config.sample.yaml config.yaml
```

2. Set the required values in `config.yaml`:
   - `bridge.domain`
   - `auth.bot_token`
   - `database.url` (or `database.conn_string` / `database.filename`)
   - registration values via either:
     - `registration.id`, `registration.as_token`, `registration.hs_token`, or
     - `discord-registration.yaml` next to your config file, or
     - env vars (see Environment Overrides below)

3. Run:

```bash
cargo check -p matrix-bridge-discord
cargo test -p matrix-bridge-discord --no-run
cargo run -p matrix-bridge-discord
```

4. Verify:

```bash
curl http://127.0.0.1:9005/health
curl http://127.0.0.1:9005/status
```

## Configure Discord (Step by Step)

1. Go to https://discord.com/developers/applications and create a new application.
2. Open the **Bot** tab, create a bot user, then copy:
   - Application ID (for `auth.client_id`)
   - Bot token (for `auth.bot_token`)
3. If you want privileged intents, enable them in the Discord portal and set `auth.use_privileged_intents: true`.
4. Invite the bot to your guild(s). Recommended permissions:
   - View Channels
   - Send Messages
   - Embed Links
   - Attach Files
   - Read Message History
   - Manage Webhooks
5. Fill the auth section in `config.yaml`:

```yaml
auth:
  client_id: "123456789012345678"
  bot_token: "YOUR_DISCORD_BOT_TOKEN"
  client_secret: null
  use_privileged_intents: false
```

6. To bridge a specific Discord channel, collect IDs from:
   - `https://discord.com/channels/<guild_id>/<channel_id>`

## Configure Matrix / Palpo (Step by Step)

1. In Palpo config (`palpo.toml`), set your server name and appservice registration directory:

```toml
server_name = "example.com"
appservice_registration_dir = "appservices"
```

2. Place your bridge registration file under that directory, for example:
   - `appservices/discord-registration.yaml`
3. Ensure tokens are consistent between Palpo registration and bridge config:
   - `as_token` in registration == bridge appservice token
   - `hs_token` in registration == bridge homeserver token
4. Ensure bridge homeserver fields point to Palpo:

```yaml
bridge:
  domain: "example.com"
  homeserver_url: "http://127.0.0.1:6006" # Replace with your Palpo URL
```

5. Start Palpo, then start this bridge.
6. Confirm connectivity both ways:
   - Palpo must reach bridge registration `url` (for appservice transactions)
   - Bridge must reach `bridge.homeserver_url` (your Palpo endpoint)

Notes:

- If Palpo and bridge run in different containers/hosts, do not use loopback addresses unless they are in the same network namespace.
- For Docker Desktop, `host.docker.internal` is often useful when bridge container needs to reach host Palpo.

## Configure Matrix / Synapse (Step by Step)

1. Set your Matrix-facing values in `config.yaml`:

```yaml
bridge:
  domain: "example.com"
  homeserver_url: "https://matrix.example.com"
  bind_address: "0.0.0.0"
  port: 9005
```

2. Create `discord-registration.yaml` next to `config.yaml` (or set `REGISTRATION_PATH`):

```yaml
id: "discord"
url: "http://127.0.0.1:9005"
as_token: "CHANGE_ME_AS_TOKEN"
hs_token: "CHANGE_ME_HS_TOKEN"
sender_localpart: "_discord_"
rate_limited: false
protocols: ["discord"]
namespaces:
  users:
    - exclusive: true
      regex: "@_discord_.*:example.com"
  aliases:
    - exclusive: true
      regex: "#_discord_.*:example.com"
  rooms: []
```

3. In Synapse `homeserver.yaml`, add:

```yaml
app_service_config_files:
  - /path/to/discord-registration.yaml
```

4. Ensure the registration `url` is reachable by Synapse.
   - Same host: `http://127.0.0.1:9005` is fine.
   - Different host/container: use a routable address.
5. Restart Synapse, then start this bridge.

Notes:

- `bridge.domain` should match your Matrix server domain (right side of MXIDs).
- `bridge.homeserver_url` should be the real homeserver URL (preferably public HTTPS if Discord needs to fetch media).
- If `registration` fields are missing in `config.yaml`, values are loaded from `discord-registration.yaml`.

## Docker

Build:

```bash
docker build -t ghcr.io/palpo-im/matrix-bridge-discord:main -f Dockerfile .
```

Run (expects `/data/config.yaml` in the mounted directory):

```bash
docker run --rm \
  -p 9005:9005 \
  -v "$(pwd)/config:/data" \
  -e CONFIG_PATH=/data/config.yaml \
  ghcr.io/palpo-im/matrix-bridge-discord:main
```

Notes:

- Container listens on `0.0.0.0:9005` by default.
- Health check endpoint: `GET /health`
- Default registration file path is `discord-registration.yaml` resolved relative to `CONFIG_PATH`.

## Database Configuration

The bridge auto-detects DB type from connection string prefix:

- `postgres://` or `postgresql://` -> PostgreSQL
- `sqlite://` -> SQLite
- `mysql://` or `mariadb://` -> MySQL / MariaDB
- anything else -> PostgreSQL fallback

MySQL backend note:

- Build with the `mysql` feature enabled, e.g. `cargo run -p matrix-bridge-discord --features mysql`
- Install `libmysqlclient` (or MariaDB Connector/C) so `mysqlclient-sys` can link

Examples:

```yaml
database:
  url: "postgresql://user:password@localhost:5432/matrix_bridge"
  max_connections: 10
  min_connections: 1
```

```yaml
database:
  url: "sqlite://./data/matrix-bridge.db"
```

```yaml
database:
  url: "mysql://user:password@localhost:3306/matrix_bridge"
  max_connections: 10
  min_connections: 1
```

## Environment Overrides

The following environment variables are supported:

- `CONFIG_PATH`
- `REGISTRATION_PATH`
- `APPSERVICE_DISCORD_AUTH_BOT_TOKEN`
- `APPSERVICE_DISCORD_AUTH_CLIENT_ID`
- `APPSERVICE_DISCORD_AUTH_CLIENT_SECRET`
- `APPSERVICE_DISCORD_REGISTRATION_ID`
- `APPSERVICE_DISCORD_REGISTRATION_AS_TOKEN`
- `APPSERVICE_DISCORD_REGISTRATION_HS_TOKEN`
- `APPSERVICE_DISCORD_REGISTRATION_SENDER_LOCALPART`

## HTTP Endpoints

Operational:

- `GET /health`
- `GET /status`
- `GET /metrics`

Provisioning:

- `GET /_matrix/app/v1/rooms?limit=<n>&offset=<n>`
- `POST /_matrix/app/v1/bridges?matrix_room_id=<room>&discord_channel_id=<channel>&discord_guild_id=<guild>`
- `GET /_matrix/app/v1/bridges/{id}`
- `DELETE /_matrix/app/v1/bridges/{id}`

## CI / Release

- Docker workflow: `.github/workflows/docker.yml`
- Release workflow (tag `v*`): `.github/workflows/release.yml`
- crates.io publish workflow: `.github/workflows/crates-release.yml`
  - Required repository secret: `CRATES_TOKEN` (crates.io API token)

## Status

Repository and build-system migration are complete.  
Feature depth is still evolving (SDK parity and full behavior alignment).

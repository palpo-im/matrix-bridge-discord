# matrix-bridge-discord

Rust 实现的 Matrix <-> Discord 桥接服务。

[English README](README.md)

维护团队：`Palpo Team`  
联系方式：`chris@acroidea.com`

## 概览

- 纯 Rust 实现（旧 Node.js/TypeScript 代码已移除）
- 包含 Matrix appservice 与 Discord bot 桥接核心
- 提供健康检查、状态、指标与 provisioning HTTP 接口
- 支持 PostgreSQL、SQLite 和 MySQL（需启用功能开关）
- 提供可直接使用的 Dockerfile

## 仓库结构

- `src/`：桥接核心实现
- `config/config.sample.yaml`：配置示例
- `migrations/`：数据库迁移
- `Dockerfile`：多阶段镜像构建

## 前置条件

- Rust 工具链（与项目兼容；Docker 构建使用 Rust 1.93）
- 已启用 appservice 的 Matrix homeserver
- Discord bot token
- PostgreSQL、SQLite 或 MySQL 数据库

## 本地快速开始

1. 生成配置文件：

```bash
cp config/config.sample.yaml config.yaml
```

2. 在 `config.yaml` 中配置必填项：
   - `bridge.domain`
   - `auth.bot_token`
   - `database.url`（或 `database.conn_string` / `database.filename`）
   - registration 字段可通过以下任一方式提供：
     - `registration.id`、`registration.as_token`、`registration.hs_token`
     - 与配置同目录的 `discord-registration.yaml`
     - 环境变量（见下文“环境变量覆盖”）

3. 运行：

```bash
cargo check -p matrix-bridge-discord
cargo test -p matrix-bridge-discord --no-run
cargo run -p matrix-bridge-discord
```

4. 验证：

```bash
curl http://127.0.0.1:9005/health
curl http://127.0.0.1:9005/status
```

## Discord 配置步骤（详细）

1. 打开 https://discord.com/developers/applications ，创建一个新应用。
2. 进入 **Bot** 页面，创建 Bot 并记录：
   - Application ID（对应 `auth.client_id`）
   - Bot Token（对应 `auth.bot_token`）
3. 如果你要启用特权 intents，请在 Discord 开发者后台打开对应选项，并在配置中设置 `auth.use_privileged_intents: true`。
4. 邀请 Bot 进入目标服务器（guild）。建议授予权限：
   - View Channels
   - Send Messages
   - Embed Links
   - Attach Files
   - Read Message History
   - Manage Webhooks
5. 在 `config.yaml` 中填写 `auth`：

```yaml
auth:
  client_id: "123456789012345678"
  bot_token: "YOUR_DISCORD_BOT_TOKEN"
  client_secret: null
  use_privileged_intents: false
```

6. 如需桥接指定频道，可从 URL 获取 ID：
   - `https://discord.com/channels/<guild_id>/<channel_id>`

## Matrix / Palpo 配置步骤（详细）

1. 在 Palpo 配置文件（`palpo.toml`）中设置服务器名和 appservice 注册目录：

```toml
server_name = "example.com"
appservice_registration_dir = "appservices"
```

2. 将桥接注册文件放到该目录下，例如：
   - `appservices/discord-registration.yaml`
3. 确保 Palpo 注册文件与桥配置中的 token 完全一致：
   - registration 里的 `as_token` == bridge 的 appservice token
   - registration 里的 `hs_token` == bridge 的 homeserver token
4. 确保桥接配置里的 homeserver 指向 Palpo：

```yaml
bridge:
  domain: "example.com"
  homeserver_url: "http://127.0.0.1:6006" # 替换为你的 Palpo 地址
```

5. 先启动 Palpo，再启动本桥接服务。
6. 检查双向连通性：
   - Palpo 需要能访问 registration 中配置的 bridge `url`
   - Bridge 需要能访问 `bridge.homeserver_url`（Palpo 地址）

说明：

- 如果 Palpo 和 bridge 在不同容器/主机，请避免使用无效的回环地址（`127.0.0.1`/`localhost`）。
- Docker Desktop 场景下，bridge 容器访问宿主机 Palpo 通常可用 `host.docker.internal`。

## Matrix / Synapse 配置步骤（详细）

1. 在 `config.yaml` 中设置 Matrix 相关参数：

```yaml
bridge:
  domain: "example.com"
  homeserver_url: "https://matrix.example.com"
  bind_address: "0.0.0.0"
  port: 9005
```

2. 在 `config.yaml` 同目录创建 `discord-registration.yaml`（或通过 `REGISTRATION_PATH` 指定路径）：

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

3. 在 Synapse 的 `homeserver.yaml` 中添加：

```yaml
app_service_config_files:
  - /path/to/discord-registration.yaml
```

4. 确保 registration 中的 `url` 能被 Synapse 访问到。
   - 同机部署可用 `http://127.0.0.1:9005`
   - 跨容器/跨主机部署请使用可路由地址
5. 重启 Synapse，再启动本桥接服务。

说明：

- `bridge.domain` 应与 Matrix 服务器域名一致（MXID `:` 右侧部分）。
- `bridge.homeserver_url` 建议填写真实可访问地址（通常为公网 HTTPS），便于 Discord 拉取媒体内容。
- 若 `config.yaml` 中 `registration` 字段未填，程序会尝试从 `discord-registration.yaml` 加载。

## Docker

构建：

```bash
docker build -t ghcr.io/palpo-im/matrix-bridge-discord:main -f Dockerfile .
```

运行（挂载目录中需包含 `/data/config.yaml`）：

```bash
docker run --rm \
  -p 9005:9005 \
  -v "$(pwd)/config:/data" \
  -e CONFIG_PATH=/data/config.yaml \
  ghcr.io/palpo-im/matrix-bridge-discord:main
```

说明：

- 容器默认监听 `0.0.0.0:9005`
- 健康检查接口：`GET /health`
- 默认注册文件名为 `discord-registration.yaml`，相对 `CONFIG_PATH` 解析

## 数据库配置

项目通过连接串前缀自动识别数据库类型：

- `postgres://` 或 `postgresql://` -> PostgreSQL
- `sqlite://` -> SQLite
- `mysql://` 或 `mariadb://` -> MySQL / MariaDB
- 其他前缀 -> 回退到 PostgreSQL

MySQL 说明：

- 需要在构建时启用 `mysql` 功能，例如 `cargo run -p matrix-bridge-discord --features mysql`
- 需要安装 `libmysqlclient`（或 MariaDB Connector/C）供 `mysqlclient-sys` 链接

示例：

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

## 环境变量覆盖

支持以下环境变量：

- `CONFIG_PATH`
- `REGISTRATION_PATH`
- `APPSERVICE_DISCORD_AUTH_BOT_TOKEN`
- `APPSERVICE_DISCORD_AUTH_CLIENT_ID`
- `APPSERVICE_DISCORD_AUTH_CLIENT_SECRET`
- `APPSERVICE_DISCORD_REGISTRATION_ID`
- `APPSERVICE_DISCORD_REGISTRATION_AS_TOKEN`
- `APPSERVICE_DISCORD_REGISTRATION_HS_TOKEN`
- `APPSERVICE_DISCORD_REGISTRATION_SENDER_LOCALPART`

## HTTP 接口

运行状态：

- `GET /health`
- `GET /status`
- `GET /metrics`

Provisioning：

- `GET /_matrix/app/v1/rooms?limit=<n>&offset=<n>`
- `POST /_matrix/app/v1/bridges?matrix_room_id=<room>&discord_channel_id=<channel>&discord_guild_id=<guild>`
- `GET /_matrix/app/v1/bridges/{id}`
- `DELETE /_matrix/app/v1/bridges/{id}`

## CI / 发布

- Docker 工作流：`.github/workflows/docker.yml`
- 二进制发布工作流（`v*` 标签触发）：`.github/workflows/release.yml`
- crates.io 发布工作流：`.github/workflows/crates-release.yml`
  - 需要配置仓库 Secret：`CRATES_TOKEN`（crates.io API token）

## 状态

仓库和构建链路迁移已完成。  
功能层面的深度和对齐仍在持续完善中。

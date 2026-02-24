# matrix-bridge-discord

本仓库已从 Node.js/TypeScript 基线迁移为 Rust 主实现，当前主代码位于仓库根目录 `src/`。

维护团队：`Palpo Team`  
联系方式：`chris@acroidea.com`

## 当前状态（2026-02-15）

- 旧 Node.js/TypeScript 代码与构建链路已清理。
- Rust 代码位于仓库根目录 `src/`，并作为当前唯一实现。
- 根目录为主 crate，可直接在仓库根目录执行 Cargo 命令。
- 编译链路已恢复：`cargo check -p matrix-bridge-discord`、`cargo test -p matrix-bridge-discord --no-run` 通过。
- Web provisioning API 不再返回 `501`，已接入数据库读写（创建/查询/删除/列表桥接）。

## 运行与验证

```bash
cargo check -p matrix-bridge-discord
cargo test -p matrix-bridge-discord --no-run
cargo run -p matrix-bridge-discord
```

## 数据库配置

项目支持 PostgreSQL 和 SQLite 两种数据库，通过配置文件中的 `url` 自动识别数据库类型。

### PostgreSQL 配置

```yaml
database:
  url: "postgresql://user:password@localhost:5432/matrix_bridge"
  max_connections: 10
  min_connections: 1
```

### SQLite 配置

```yaml
database:
  url: "sqlite:///data/matrix-bridge.db"
```

或使用相对路径：

```yaml
database:
  url: "sqlite://./data/matrix-bridge.db"
```

### 数据库类型识别规则

- `postgres://` 或 `postgresql://` 开头 → 使用 PostgreSQL
- `sqlite://` 开头 → 使用 SQLite
- 默认（无匹配前缀）→ 使用 PostgreSQL（向后兼容）

## GitHub Actions 发布配置

- Docker 镜像工作流（`.github/workflows/docker.yml`）会发布到：
  - `ghcr.io/palpo-im/matrix-bridge-discord`
  - `docker.io/<namespace>/matrix-bridge-discord`
- 需要在仓库 Secrets 中配置 Docker Hub 凭据：
  - `DOCKERHUB_USERNAME`
  - `DOCKERHUB_TOKEN`
- 如果 Docker Hub 命名空间与 GitHub owner 不一致，可额外设置仓库变量 `DOCKERHUB_NAMESPACE`。
- 二进制发布工作流（`.github/workflows/release.yml`）在推送 `v*` 标签时触发，上传 Windows/Linux/macOS 二进制到 GitHub Releases。

## 说明

本次转移已完成"代码仓库与构建系统"的迁移闭环。
仍需继续做功能深度完善（例如真实 Matrix/Discord SDK 对接与全量功能对齐），详见 `MIGRATION_STATUS.md`。

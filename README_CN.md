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

## 说明

本次转移已完成"代码仓库与构建系统"的迁移闭环。
仍需继续做功能深度完善（例如真实 Matrix/Discord SDK 对接与全量功能对齐），详见 `MIGRATION_STATUS.md`。
